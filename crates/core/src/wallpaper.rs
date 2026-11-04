//! Journaled wallpaper changes.
//!
//! Protocol (see `veneer-state-journal`): capture the original, persist it,
//! only then change the wallpaper. If Veneer dies in between, the next start
//! calls [`WallpaperController::recover`] and the original comes back.

use std::{fmt, io, path::Path};

use serde::Serialize;
use serde_json::{Value, json};
use veneer_state_journal::{Entry, Journal, RecoveryReport, RestoreError, Restorer};
use veneer_windows_platform::backend::{MonitorWallpaper, WallpaperBackend};

const MODULE: &str = "wallpaper";
const KEY: &str = "desktop";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WallpaperError {
    NotAnImage(String),
    Capture(String),
    /// A monitor shows a solid color or slideshow, which cannot be put back.
    NotRestorable,
    Journal(String),
    Set(String),
}

impl fmt::Display for WallpaperError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAnImage(path) => write!(f, "il file non esiste: {path}"),
            Self::Capture(e) => write!(f, "impossibile leggere lo sfondo attuale: {e}"),
            Self::NotRestorable => write!(
                f,
                "lo sfondo attuale (tinta unita o slideshow) non è ripristinabile: nessuna modifica fatta"
            ),
            Self::Journal(e) => write!(f, "impossibile salvare lo stato originale: {e}"),
            Self::Set(e) => write!(f, "Windows ha rifiutato lo sfondo: {e}"),
        }
    }
}

impl std::error::Error for WallpaperError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WallpaperRevert {
    pub was_changed: bool,
    pub failed: Vec<String>,
}

#[derive(Debug)]
pub struct WallpaperController<B> {
    backend: B,
    journal: Journal,
}

fn to_json(snapshot: &[MonitorWallpaper]) -> Value {
    Value::Array(
        snapshot
            .iter()
            .map(|m| json!({ "monitorId": m.monitor_id, "path": m.path }))
            .collect(),
    )
}

fn from_json(value: &Value) -> Option<Vec<MonitorWallpaper>> {
    value
        .as_array()?
        .iter()
        .map(|item| {
            Some(MonitorWallpaper {
                monitor_id: item.get("monitorId")?.as_str()?.to_owned(),
                path: item.get("path")?.as_str()?.to_owned(),
            })
        })
        .collect()
}

struct WallpaperRestorer<'a, B>(&'a mut B);

impl<B: WallpaperBackend> Restorer for WallpaperRestorer<'_, B> {
    fn restore(&mut self, entry: &Entry) -> Result<(), RestoreError> {
        if entry.module != MODULE {
            return Err(RestoreError::Gone(format!(
                "unknown module {}",
                entry.module
            )));
        }
        let snapshot = from_json(&entry.original)
            .ok_or_else(|| RestoreError::Gone("unreadable snapshot".into()))?;
        let failures = self.0.restore(&snapshot);
        if failures.is_empty() {
            Ok(())
        } else {
            Err(RestoreError::Retry(failures.join("; ")))
        }
    }
}

impl<B: WallpaperBackend> WallpaperController<B> {
    pub fn new(backend: B, journal: Journal) -> Self {
        Self { backend, journal }
    }

    /// Restores whatever a previous, uncleanly ended run left in the journal.
    pub fn recover(&mut self) -> io::Result<RecoveryReport> {
        self.journal
            .recover(&mut WallpaperRestorer(&mut self.backend))
    }

    pub fn is_changed(&self) -> bool {
        !self.journal.is_empty()
    }

    /// Sets `image` on every monitor. All checks and the journal write happen
    /// before the first change, so any error leaves the desktop untouched.
    pub fn set(&mut self, image: &Path) -> Result<(), WallpaperError> {
        if !image.is_file() {
            return Err(WallpaperError::NotAnImage(image.display().to_string()));
        }
        let snapshot = self.backend.capture().map_err(WallpaperError::Capture)?;
        if snapshot.is_empty() || snapshot.iter().any(|m| m.path.is_empty()) {
            return Err(WallpaperError::NotRestorable);
        }
        // `record` keeps the first original, so repeated sets stay reversible
        // to the true original.
        let newly_recorded = self
            .journal
            .record(MODULE, KEY, to_json(&snapshot))
            .map_err(|e| WallpaperError::Journal(e.to_string()))?;
        if let Err(error) = self.backend.set_all(image) {
            if newly_recorded {
                let _ = self.journal.forget(MODULE, KEY);
            }
            return Err(WallpaperError::Set(error));
        }
        tracing::info!(image = %image.display(), "wallpaper set");
        Ok(())
    }

    /// Puts the original wallpaper back. If a monitor fails, the journal entry
    /// is kept so the next attempt (or start) can retry.
    pub fn revert(&mut self) -> WallpaperRevert {
        let Some(snapshot) = self
            .journal
            .entries()
            .iter()
            .find(|e| e.module == MODULE && e.key == KEY)
            .and_then(|e| from_json(&e.original))
        else {
            return WallpaperRevert {
                was_changed: false,
                failed: Vec::new(),
            };
        };
        let failed = self.backend.restore(&snapshot);
        if failed.is_empty() {
            let _ = self.journal.forget(MODULE, KEY);
        } else {
            tracing::warn!(?failed, "wallpaper restore incomplete");
        }
        WallpaperRevert {
            was_changed: true,
            failed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default)]
    struct Fake {
        current: Vec<MonitorWallpaper>,
        fail_set: bool,
        fail_restore: bool,
    }

    impl WallpaperBackend for Fake {
        fn capture(&mut self) -> Result<Vec<MonitorWallpaper>, String> {
            Ok(self.current.clone())
        }
        fn set_all(&mut self, image: &Path) -> Result<(), String> {
            if self.fail_set {
                return Err("denied".into());
            }
            for m in &mut self.current {
                m.path = image.display().to_string();
            }
            Ok(())
        }
        fn restore(&mut self, snapshot: &[MonitorWallpaper]) -> Vec<String> {
            if self.fail_restore {
                return vec!["m1: busy".into()];
            }
            self.current = snapshot.to_vec();
            Vec::new()
        }
    }

    fn monitor(path: &str) -> MonitorWallpaper {
        MonitorWallpaper {
            monitor_id: "m1".into(),
            path: path.into(),
        }
    }

    struct Env {
        dir: tempfile::TempDir,
        image: std::path::PathBuf,
    }

    fn env() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("new.png");
        std::fs::write(&image, b"x").unwrap();
        Env { dir, image }
    }

    fn controller(env: &Env, fake: Fake) -> WallpaperController<Fake> {
        let (journal, _) = Journal::open(env.dir.path().join("journal.json")).unwrap();
        WallpaperController::new(fake, journal)
    }

    fn original() -> Fake {
        Fake {
            current: vec![monitor("C:\\orig.png")],
            ..Fake::default()
        }
    }

    #[test]
    fn set_then_revert_restores_the_original() {
        let env = env();
        let mut c = controller(&env, original());
        c.set(&env.image).unwrap();
        assert!(c.is_changed());
        assert_eq!(c.backend.current[0].path, env.image.display().to_string());

        let result = c.revert();
        assert!(result.was_changed && result.failed.is_empty());
        assert_eq!(c.backend.current, vec![monitor("C:\\orig.png")]);
        assert!(!c.is_changed());
    }

    #[test]
    fn setting_twice_still_reverts_to_the_true_original() {
        let env = env();
        let mut c = controller(&env, original());
        c.set(&env.image).unwrap();
        c.set(&env.image).unwrap();
        c.revert();
        assert_eq!(c.backend.current, vec![monitor("C:\\orig.png")]);
    }

    #[test]
    fn missing_image_changes_nothing() {
        let env = env();
        let mut c = controller(&env, original());
        let err = c.set(&env.dir.path().join("nope.png")).unwrap_err();
        assert!(matches!(err, WallpaperError::NotAnImage(_)));
        assert!(!c.is_changed());
    }

    #[test]
    fn unrestorable_current_wallpaper_is_refused() {
        let env = env();
        let fake = Fake {
            current: vec![monitor("")],
            ..Fake::default()
        };
        let mut c = controller(&env, fake);
        assert_eq!(
            c.set(&env.image).unwrap_err(),
            WallpaperError::NotRestorable
        );
        assert!(!c.is_changed());
        assert_eq!(c.backend.current, vec![monitor("")]);
    }

    #[test]
    fn rejected_set_leaves_no_journal_entry() {
        let env = env();
        let mut fake = original();
        fake.fail_set = true;
        let mut c = controller(&env, fake);
        assert!(matches!(c.set(&env.image), Err(WallpaperError::Set(_))));
        assert!(!c.is_changed());
    }

    #[test]
    fn failed_restore_keeps_the_entry_for_a_retry() {
        let env = env();
        let mut c = controller(&env, original());
        c.set(&env.image).unwrap();
        c.backend.fail_restore = true;
        let result = c.revert();
        assert_eq!(result.failed.len(), 1);
        assert!(c.is_changed());

        c.backend.fail_restore = false;
        assert!(c.revert().failed.is_empty());
        assert!(!c.is_changed());
    }

    #[test]
    fn revert_without_changes_is_a_noop() {
        let env = env();
        let mut c = controller(&env, original());
        assert!(!c.revert().was_changed);
    }

    #[test]
    fn crash_recovery_restores_on_next_start() {
        let env = env();
        {
            let mut c = controller(&env, original());
            c.set(&env.image).unwrap();
            // Dropped without revert: simulates a crash.
        }
        let (journal, opened) = Journal::open(env.dir.path().join("journal.json")).unwrap();
        assert_eq!(opened, veneer_state_journal::Opened::Pending(1));
        let fake = Fake {
            current: vec![monitor(&env.image.display().to_string())],
            ..Fake::default()
        };
        let mut c = WallpaperController::new(fake, journal);
        let report = c.recover().unwrap();
        assert_eq!(report.restored, 1);
        assert_eq!(c.backend.current, vec![monitor("C:\\orig.png")]);
        assert!(!c.is_changed());
    }
}
