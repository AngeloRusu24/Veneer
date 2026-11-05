//! Journaled changes to simple global settings (appearance mode, cursors).
//!
//! Same protocol as the wallpaper (capture, persist, only then change), but
//! generic over a [`SettingBackend`], so a new setting is a backend plus a
//! function that picks its values out of a theme.

use std::io;

use serde::Serialize;
use serde_json::{Value, json};
use veneer_state_journal::{Entry, Journal, RecoveryReport, RestoreError, Restorer};
use veneer_theme_schema::{ColorMode, Theme};
use veneer_windows_platform::backend::{SettingBackend, Settings, Snapshot};

const KEY: &str = "setting";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingRevert {
    pub name: String,
    pub was_changed: bool,
    pub failed: Vec<String>,
}

pub struct SettingController {
    module: &'static str,
    backend: Box<dyn SettingBackend + Send>,
    journal: Journal,
}

impl std::fmt::Debug for SettingController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingController")
            .field("module", &self.module)
            .finish_non_exhaustive()
    }
}

fn to_json(snapshot: &Snapshot) -> Value {
    Value::Array(
        snapshot
            .iter()
            .map(|(name, value)| json!({ "name": name, "value": value }))
            .collect(),
    )
}

fn from_json(value: &Value) -> Option<Snapshot> {
    value
        .as_array()?
        .iter()
        .map(|item| {
            let value = match item.get("value")? {
                Value::Null => None,
                Value::String(s) => Some(s.clone()),
                _ => return None,
            };
            Some((item.get("name")?.as_str()?.to_owned(), value))
        })
        .collect()
}

struct SettingRestorer<'a> {
    module: &'static str,
    backend: &'a mut (dyn SettingBackend + Send),
}

impl Restorer for SettingRestorer<'_> {
    fn restore(&mut self, entry: &Entry) -> Result<(), RestoreError> {
        if entry.module != self.module {
            return Err(RestoreError::Gone(format!(
                "unknown module {}",
                entry.module
            )));
        }
        let snapshot = from_json(&entry.original)
            .ok_or_else(|| RestoreError::Gone("unreadable snapshot".into()))?;
        let failures = self.backend.restore(&snapshot);
        if failures.is_empty() {
            Ok(())
        } else {
            Err(RestoreError::Retry(failures.join("; ")))
        }
    }
}

impl SettingController {
    pub fn new(
        module: &'static str,
        backend: impl SettingBackend + Send + 'static,
        journal: Journal,
    ) -> Self {
        Self {
            module,
            backend: Box::new(backend),
            journal,
        }
    }

    pub fn name(&self) -> &'static str {
        self.module
    }

    /// Restores whatever a previous, uncleanly ended run left in the journal.
    pub fn recover(&mut self) -> io::Result<RecoveryReport> {
        let mut restorer = SettingRestorer {
            module: self.module,
            backend: self.backend.as_mut(),
        };
        self.journal.recover(&mut restorer)
    }

    pub fn is_changed(&self) -> bool {
        !self.journal.is_empty()
    }

    /// Applies `desired`. The original is persisted first and kept across
    /// repeated calls, so the true original is always what `revert` restores.
    pub fn set(&mut self, desired: &Settings) -> Result<(), String> {
        let snapshot = self
            .backend
            .capture()
            .map_err(|e| format!("impossibile leggere lo stato attuale: {e}"))?;
        let newly_recorded = self
            .journal
            .record(self.module, KEY, to_json(&snapshot))
            .map_err(|e| format!("impossibile salvare lo stato originale: {e}"))?;
        if let Err(error) = self.backend.apply(desired) {
            // The change may be partial: put back what we can, and only then
            // drop the journal entry.
            let mut failures = self.backend.restore(&snapshot);
            if newly_recorded
                && failures.is_empty()
                && let Err(journal_error) = self.journal.forget(self.module, KEY)
            {
                tracing::error!(
                    module = self.module,
                    %journal_error,
                    "could not clear the journal after rolling back a failed setting"
                );
                failures.push(format!("journal cleanup failed: {journal_error}"));
            }
            let detail = if failures.is_empty() {
                String::new()
            } else {
                format!("; rollback incompleto: {}", failures.join("; "))
            };
            return Err(format!("Windows ha rifiutato la modifica: {error}{detail}"));
        }
        tracing::info!(module = self.module, "setting applied");
        Ok(())
    }

    /// Puts the original back. If that fails the journal entry is kept so the
    /// next attempt (or start) can retry.
    pub fn revert(&mut self) -> SettingRevert {
        let name = self.module.to_owned();
        let Some(entry) = self
            .journal
            .entries()
            .iter()
            .find(|e| e.module == self.module && e.key == KEY)
        else {
            return SettingRevert {
                name,
                was_changed: false,
                failed: Vec::new(),
            };
        };
        let Some(snapshot) = from_json(&entry.original) else {
            return SettingRevert {
                name,
                was_changed: true,
                failed: vec!["lo snapshot salvato non è leggibile; journal mantenuto".into()],
            };
        };
        let mut failed = self.backend.restore(&snapshot);
        if failed.is_empty()
            && let Err(error) = self.journal.forget(self.module, KEY)
        {
            tracing::error!(
                module = self.module,
                %error,
                "setting restored but its journal entry could not be cleared"
            );
            failed.push(format!("journal cleanup failed: {error}"));
        }
        if !failed.is_empty() {
            tracing::warn!(module = self.module, ?failed, "restore incomplete");
        }
        SettingRevert {
            name,
            was_changed: true,
            failed,
        }
    }
}

/// What the theme asks for the `appearance` module, if anything.
pub fn appearance_of(theme: &Theme) -> Option<Settings> {
    let appearance = theme.appearance.as_ref()?;
    let mode = |m: ColorMode| match m {
        ColorMode::Light => "light",
        ColorMode::Dark => "dark",
    };
    let mut settings = Settings::new();
    if let Some(m) = appearance.apps {
        settings.push(("apps".into(), mode(m).into()));
    }
    if let Some(m) = appearance.system {
        settings.push(("system".into(), mode(m).into()));
    }
    Some(settings)
}

/// What the theme asks for the `cursors` module, if anything.
pub fn cursors_of(theme: &Theme) -> Option<Settings> {
    let cursors = theme.cursors.as_ref()?;
    Some(
        cursors
            .entries()
            .into_iter()
            .map(|(role, path)| (role.to_owned(), path.to_owned()))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Debug, Default)]
    pub(crate) struct State {
        value: Option<String>,
        fail_apply: bool,
        fail_restore: bool,
    }

    #[derive(Clone, Default)]
    pub(crate) struct Fake(pub(crate) Arc<Mutex<State>>);

    impl SettingBackend for Fake {
        fn capture(&mut self) -> Result<Snapshot, String> {
            Ok(vec![("v".into(), self.0.lock().unwrap().value.clone())])
        }
        fn apply(&mut self, desired: &[(String, String)]) -> Result<(), String> {
            let mut state = self.0.lock().unwrap();
            state.value = Some(desired[0].1.clone());
            if state.fail_apply {
                return Err("denied".into());
            }
            Ok(())
        }
        fn restore(&mut self, snapshot: &Snapshot) -> Vec<String> {
            let mut state = self.0.lock().unwrap();
            if state.fail_restore {
                return vec!["busy".into()];
            }
            state.value = snapshot[0].1.clone();
            Vec::new()
        }
    }

    fn controller(fake: &Fake, dir: &std::path::Path) -> SettingController {
        let (journal, _) = Journal::open(dir.join("j.json")).unwrap();
        SettingController::new("test", fake.clone(), journal)
    }

    fn want(value: &str) -> Settings {
        vec![("v".into(), value.into())]
    }

    fn value(fake: &Fake) -> Option<String> {
        fake.0.lock().unwrap().value.clone()
    }

    #[test]
    fn set_then_revert_restores_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::default();
        fake.0.lock().unwrap().value = Some("orig".into());
        let mut c = controller(&fake, dir.path());
        c.set(&want("new")).unwrap();
        assert_eq!(value(&fake).as_deref(), Some("new"));
        assert!(c.is_changed());
        let r = c.revert();
        assert!(r.was_changed && r.failed.is_empty());
        assert_eq!(value(&fake).as_deref(), Some("orig"));
        assert!(!c.is_changed());
    }

    #[test]
    fn repeated_sets_still_revert_to_the_true_original() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::default();
        fake.0.lock().unwrap().value = Some("orig".into());
        let mut c = controller(&fake, dir.path());
        c.set(&want("a")).unwrap();
        c.set(&want("b")).unwrap();
        c.revert();
        assert_eq!(value(&fake).as_deref(), Some("orig"));
    }

    #[test]
    fn a_failed_apply_is_rolled_back_and_forgotten() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::default();
        {
            let mut s = fake.0.lock().unwrap();
            s.value = Some("orig".into());
            s.fail_apply = true;
        }
        let mut c = controller(&fake, dir.path());
        assert!(c.set(&want("new")).is_err());
        assert_eq!(value(&fake).as_deref(), Some("orig"));
        assert!(!c.is_changed());
    }

    #[test]
    fn a_failed_revert_keeps_the_entry_for_a_retry() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::default();
        fake.0.lock().unwrap().value = Some("orig".into());
        let mut c = controller(&fake, dir.path());
        c.set(&want("new")).unwrap();
        fake.0.lock().unwrap().fail_restore = true;
        assert!(!c.revert().failed.is_empty());
        assert!(c.is_changed());
        fake.0.lock().unwrap().fail_restore = false;
        assert!(c.revert().failed.is_empty());
        assert_eq!(value(&fake).as_deref(), Some("orig"));
    }

    #[test]
    fn recovery_after_a_crash_restores_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::default();
        fake.0.lock().unwrap().value = Some("orig".into());
        {
            let mut c = controller(&fake, dir.path());
            c.set(&want("new")).unwrap();
            // dropped without revert: simulates a crash
        }
        assert_eq!(value(&fake).as_deref(), Some("new"));
        let mut again = controller(&fake, dir.path());
        again.recover().unwrap();
        assert_eq!(value(&fake).as_deref(), Some("orig"));
        assert!(!again.is_changed());
    }

    #[test]
    fn selectors_read_the_theme() {
        let theme = veneer_theme_schema::parse_theme(
            r#"{"schemaVersion":1,"id":"t","name":"T",
                "appearance":{"apps":"dark"},
                "cursors":{"arrow":"C:\\a.cur"}}"#,
        )
        .unwrap();
        assert_eq!(
            appearance_of(&theme).unwrap(),
            vec![("apps".to_owned(), "dark".to_owned())]
        );
        assert_eq!(cursors_of(&theme).unwrap()[0].0, "arrow");

        let plain =
            veneer_theme_schema::parse_theme(r#"{"schemaVersion":1,"id":"t","name":"T"}"#).unwrap();
        assert!(appearance_of(&plain).is_none() && cursors_of(&plain).is_none());
    }
}
