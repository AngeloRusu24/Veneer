//! One call to apply a whole theme (window frame + wallpaper), one to undo it.

use std::path::Path;

use serde::Serialize;
use veneer_rule_engine::WindowContext;
use veneer_theme_schema::{TimeOfDay, UnsupportedPolicy, parse_theme};
use veneer_windows_platform::backend::{FrameBackend, WallpaperBackend};

use crate::{
    ApplyError, ApplyReport, Controller, RevertReport,
    setting::{SettingController, SettingRevert},
    wallpaper::{WallpaperController, WallpaperRevert},
};
use veneer_theme_schema::Theme;
use veneer_windows_platform::backend::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeStatus {
    /// The theme asks for nothing here and nothing was active.
    NotRequested,
    Applied,
    /// The theme asks for nothing here, so the previous theme's change was undone.
    Reverted,
    /// Could not be changed; the rest of the theme is applied (policy `warn`).
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingReport {
    pub name: String,
    pub status: ChangeStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeReport {
    #[serde(flatten)]
    pub frame: ApplyReport,
    pub wallpaper: ChangeStatus,
    pub settings: Vec<SettingReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeRevert {
    pub frame: RevertReport,
    pub wallpaper: WallpaperRevert,
    pub settings: Vec<SettingRevert>,
}

/// A simple global setting driven by a theme section.
#[derive(Debug)]
struct Slot {
    name: &'static str,
    select: fn(&Theme) -> Option<Settings>,
    /// `None` when the journal could not be opened: changes are then refused
    /// rather than made without a way back.
    controller: Option<SettingController>,
}

#[derive(Debug)]
pub struct ThemeApplier<F, W> {
    frames: Controller<F>,
    /// `None` when the journal could not be opened: wallpaper changes are then
    /// refused rather than made without a way back.
    wallpaper: Option<WallpaperController<W>>,
    slots: Vec<Slot>,
}

impl<F: FrameBackend, W: WallpaperBackend> ThemeApplier<F, W> {
    pub fn new(frames: Controller<F>, wallpaper: Option<WallpaperController<W>>) -> Self {
        Self {
            frames,
            wallpaper,
            slots: Vec::new(),
        }
    }

    /// Registers a global setting (for example appearance or cursors).
    #[must_use]
    pub fn with_setting(
        mut self,
        name: &'static str,
        select: fn(&Theme) -> Option<Settings>,
        controller: Option<SettingController>,
    ) -> Self {
        self.slots.push(Slot {
            name,
            select,
            controller,
        });
        self
    }

    /// Applies the window-frame part, then the wallpaper and the global
    /// settings. Applying a theme replaces the previous one completely.
    ///
    /// With policy `error`, any global failure rolls everything back and
    /// returns an error; with `warn` it is reported and the rest stays applied.
    pub fn apply(
        &mut self,
        window: isize,
        theme_json: &str,
        context: &WindowContext,
        now: TimeOfDay,
    ) -> Result<ThemeReport, ApplyError> {
        let theme = parse_theme(theme_json)?;
        let mut frame = self.frames.apply_theme(window, theme_json, context, now)?;
        let strict = theme.unsupported == UnsupportedPolicy::Error;

        let wallpaper = match &theme.wallpaper {
            Some(wallpaper) => match self.set_wallpaper(Path::new(&wallpaper.path)) {
                Ok(()) => ChangeStatus::Applied,
                Err(reason) if strict => {
                    let reason = self.rollback_failures(window, reason);
                    return Err(ApplyError::Wallpaper(reason));
                }
                Err(reason) => {
                    frame.warnings.push(reason);
                    ChangeStatus::Failed
                }
            },
            None => match self.wallpaper.as_mut().map(WallpaperController::revert) {
                Some(result) if !result.failed.is_empty() => {
                    let reason = format!(
                        "sfondo precedente non ripristinato: {}",
                        result.failed.join("; ")
                    );
                    if strict {
                        let reason = self.rollback_failures(window, reason);
                        return Err(ApplyError::Wallpaper(reason));
                    }
                    frame.warnings.push(reason);
                    ChangeStatus::Failed
                }
                Some(result) if result.was_changed => ChangeStatus::Reverted,
                _ => ChangeStatus::NotRequested,
            },
        };

        let mut settings = Vec::with_capacity(self.slots.len());
        for index in 0..self.slots.len() {
            let slot = &mut self.slots[index];
            let name = slot.name;
            let desired = (slot.select)(&theme);
            let outcome = match (desired, slot.controller.as_mut()) {
                (Some(desired), Some(controller)) => controller
                    .set(&desired)
                    .map(|()| ChangeStatus::Applied)
                    .map_err(|reason| format!("{name}: {reason}")),
                (Some(_), None) => Err(format!("{name} non disponibile: journal non aperto")),
                (None, Some(controller)) => {
                    let result = controller.revert();
                    if !result.failed.is_empty() {
                        Err(format!(
                            "{name} precedente non ripristinato: {}",
                            result.failed.join("; ")
                        ))
                    } else if result.was_changed {
                        Ok(ChangeStatus::Reverted)
                    } else {
                        Ok(ChangeStatus::NotRequested)
                    }
                }
                (None, None) => Ok(ChangeStatus::NotRequested),
            };
            let status = match outcome {
                Ok(status) => status,
                Err(reason) if strict => {
                    let reason = self.rollback_failures(window, reason);
                    return Err(ApplyError::Setting(reason));
                }
                Err(reason) => {
                    frame.warnings.push(reason);
                    ChangeStatus::Failed
                }
            };
            settings.push(SettingReport {
                name: name.to_owned(),
                status,
            });
        }
        Ok(ThemeReport {
            frame,
            wallpaper,
            settings,
        })
    }

    fn set_wallpaper(&mut self, path: &Path) -> Result<(), String> {
        self.wallpaper
            .as_mut()
            .ok_or_else(|| "sfondo non disponibile: journal non aperto".to_owned())?
            .set(path)
            .map_err(|error| error.to_string())
    }

    fn rollback_failures(&mut self, window: isize, reason: String) -> String {
        let reverted = self.revert(window);
        let mut failures = reverted
            .frame
            .failed
            .into_iter()
            .map(|item| format!("cornice {} ({})", item.property, item.hresult))
            .collect::<Vec<_>>();
        failures.extend(
            reverted
                .wallpaper
                .failed
                .into_iter()
                .map(|item| format!("sfondo: {item}")),
        );
        for setting in reverted.settings {
            failures.extend(
                setting
                    .failed
                    .into_iter()
                    .map(|item| format!("{}: {item}", setting.name)),
            );
        }
        if failures.is_empty() {
            reason
        } else {
            format!("{reason}; rollback incompleto: {}", failures.join("; "))
        }
    }

    pub fn revert(&mut self, window: isize) -> ThemeRevert {
        ThemeRevert {
            frame: self.frames.revert(window),
            wallpaper: self.revert_wallpaper(),
            settings: self.revert_settings(),
        }
    }

    /// Undoes every style, the wallpaper and the settings; used on exit.
    pub fn revert_all(&mut self) -> (Vec<RevertReport>, WallpaperRevert, Vec<SettingRevert>) {
        (
            self.frames.revert_all(),
            self.revert_wallpaper(),
            self.revert_settings(),
        )
    }

    fn revert_settings(&mut self) -> Vec<SettingRevert> {
        self.slots
            .iter_mut()
            .filter_map(|slot| slot.controller.as_mut().map(SettingController::revert))
            .collect()
    }

    fn revert_wallpaper(&mut self) -> WallpaperRevert {
        self.wallpaper
            .as_mut()
            .map(WallpaperController::revert)
            .unwrap_or(WallpaperRevert {
                was_changed: false,
                failed: Vec::new(),
            })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use veneer_state_journal::Journal;
    use veneer_windows_platform::{
        backend::{ApplyOutcome, MonitorWallpaper, SettingBackend, Snapshot},
        plan::{Failure, Plan},
    };

    use super::*;

    #[derive(Debug, Default)]
    struct Frames;

    impl FrameBackend for Frames {
        fn apply(&mut self, _window: isize, plan: &Plan) -> ApplyOutcome {
            ApplyOutcome {
                applied: plan.operations.clone(),
                failed: Vec::new(),
            }
        }
        fn restore(&mut self, _window: isize) -> Vec<Failure> {
            Vec::new()
        }
    }

    #[derive(Debug)]
    struct Wall {
        current: String,
        fail_set: bool,
    }

    impl WallpaperBackend for Wall {
        fn capture(&mut self) -> Result<Vec<MonitorWallpaper>, String> {
            Ok(vec![MonitorWallpaper {
                monitor_id: "m".into(),
                path: self.current.clone(),
            }])
        }
        fn set_all(&mut self, image: &Path) -> Result<(), String> {
            if self.fail_set {
                return Err("denied".into());
            }
            self.current = image.display().to_string();
            Ok(())
        }
        fn restore(&mut self, snapshot: &[MonitorWallpaper]) -> Vec<String> {
            self.current = snapshot[0].path.clone();
            Vec::new()
        }
    }

    #[derive(Debug, Clone)]
    struct Global {
        state: std::sync::Arc<std::sync::Mutex<String>>,
        fail: bool,
    }

    impl SettingBackend for Global {
        fn capture(&mut self) -> Result<Snapshot, String> {
            Ok(vec![(
                "mode".into(),
                Some(self.state.lock().unwrap().clone()),
            )])
        }

        fn apply(&mut self, desired: &[(String, String)]) -> Result<(), String> {
            if self.fail {
                return Err("denied".into());
            }
            *self.state.lock().unwrap() = desired[0].1.clone();
            Ok(())
        }

        fn restore(&mut self, snapshot: &Snapshot) -> Vec<String> {
            if let Some((_, Some(value))) = snapshot.first() {
                *self.state.lock().unwrap() = value.clone();
                Vec::new()
            } else {
                vec!["invalid test snapshot".into()]
            }
        }
    }

    struct Env {
        dir: tempfile::TempDir,
        image: PathBuf,
    }

    fn env() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("wall.png");
        std::fs::write(&image, b"x").unwrap();
        Env { dir, image }
    }

    fn applier(env: &Env, fail_set: bool, with_journal: bool) -> ThemeApplier<Frames, Wall> {
        let wallpaper = with_journal.then(|| {
            let (journal, _) = Journal::open(env.dir.path().join("journal.json")).unwrap();
            let backend = Wall {
                current: r"C:\orig.png".into(),
                fail_set,
            };
            WallpaperController::new(backend, journal)
        });
        ThemeApplier::new(Controller::new(Frames, Some(26100)), wallpaper)
    }

    fn context() -> WindowContext {
        WindowContext {
            process_name: "veneer-desktop.exe".into(),
            title: "Veneer".into(),
            on_primary_monitor: Some(true),
            is_foreground: true,
        }
    }

    fn now() -> TimeOfDay {
        "12:00".parse().unwrap()
    }

    fn theme(image: Option<&Path>, policy: &str) -> String {
        let wallpaper = image
            .map(|p| {
                format!(
                    r#","wallpaper":{{"path":{}}}"#,
                    serde_json::to_string(&p.display().to_string()).unwrap()
                )
            })
            .unwrap_or_default();
        format!(
            r#"{{"schemaVersion":1,"id":"t","name":"T","unsupported":"{policy}"{wallpaper},"rules":[{{"id":"r","apply":{{"windowFrame":{{"colorMode":"dark"}}}}}}]}}"#
        )
    }

    fn current(a: &mut ThemeApplier<Frames, Wall>) -> String {
        a.wallpaper.as_mut().unwrap().backend().current.clone()
    }

    #[test]
    fn applies_frame_and_wallpaper_then_reverts_both() {
        let env = env();
        let mut a = applier(&env, false, true);
        let report = a
            .apply(1, &theme(Some(&env.image), "warn"), &context(), now())
            .unwrap();
        assert_eq!(report.frame.applied, ["colorMode"]);
        assert_eq!(report.wallpaper, ChangeStatus::Applied);
        assert_eq!(current(&mut a), env.image.display().to_string());

        let undone = a.revert(1);
        assert!(undone.frame.was_styled && undone.wallpaper.was_changed);
        assert_eq!(current(&mut a), r"C:\orig.png");
    }

    #[test]
    fn a_theme_without_wallpaper_undoes_the_previous_one() {
        let env = env();
        let mut a = applier(&env, false, true);
        a.apply(1, &theme(Some(&env.image), "warn"), &context(), now())
            .unwrap();
        let report = a.apply(1, &theme(None, "warn"), &context(), now()).unwrap();
        assert_eq!(report.wallpaper, ChangeStatus::Reverted);
        assert_eq!(current(&mut a), r"C:\orig.png");

        let again = a.apply(1, &theme(None, "warn"), &context(), now()).unwrap();
        assert_eq!(again.wallpaper, ChangeStatus::NotRequested);
    }

    #[test]
    fn warn_policy_keeps_the_frame_when_the_wallpaper_fails() {
        let env = env();
        let mut a = applier(&env, true, true);
        let report = a
            .apply(1, &theme(Some(&env.image), "warn"), &context(), now())
            .unwrap();
        assert_eq!(report.wallpaper, ChangeStatus::Failed);
        assert_eq!(report.frame.applied, ["colorMode"]);
        assert!(report.frame.warnings.iter().any(|w| w.contains("denied")));
        assert!(a.frames.is_styled(1));
    }

    #[test]
    fn error_policy_rolls_the_frame_back_when_the_wallpaper_fails() {
        let env = env();
        let mut a = applier(&env, true, true);
        let err = a
            .apply(1, &theme(Some(&env.image), "error"), &context(), now())
            .unwrap_err();
        assert!(matches!(err, ApplyError::Wallpaper(_)));
        assert!(!a.frames.is_styled(1));
    }

    #[test]
    fn applies_and_reverts_theme_appearance_setting() {
        let env = env();
        let state = std::sync::Arc::new(std::sync::Mutex::new("light".to_owned()));
        let (journal, _) = Journal::open(env.dir.path().join("appearance.journal.json")).unwrap();
        let setting = SettingController::new(
            "appearance",
            Global {
                state: state.clone(),
                fail: false,
            },
            journal,
        );
        let mut a = ThemeApplier::<Frames, Wall>::new(Controller::new(Frames, Some(26100)), None)
            .with_setting("appearance", crate::setting::appearance_of, Some(setting));
        let json = theme(None, "warn")
            .replace("\"rules\"", "\"appearance\":{\"apps\":\"dark\"},\"rules\"");
        let report = a.apply(1, &json, &context(), now()).unwrap();
        assert_eq!(*state.lock().unwrap(), "dark");
        assert_eq!(report.settings[0].status, ChangeStatus::Applied);

        let reverted = a.revert(1);
        assert_eq!(*state.lock().unwrap(), "light");
        assert!(reverted.settings[0].failed.is_empty());
        assert!(reverted.settings[0].was_changed);
    }

    #[test]
    fn error_policy_rolls_back_frame_when_a_setting_fails() {
        let env = env();
        let state = std::sync::Arc::new(std::sync::Mutex::new("light".to_owned()));
        let (journal, _) = Journal::open(env.dir.path().join("appearance.journal.json")).unwrap();
        let setting = SettingController::new(
            "appearance",
            Global {
                state: state.clone(),
                fail: true,
            },
            journal,
        );
        let mut a = ThemeApplier::<Frames, Wall>::new(Controller::new(Frames, Some(26100)), None)
            .with_setting("appearance", crate::setting::appearance_of, Some(setting));
        let json = theme(None, "error")
            .replace("\"rules\"", "\"appearance\":{\"apps\":\"dark\"},\"rules\"");
        let error = a.apply(1, &json, &context(), now()).unwrap_err();
        assert!(matches!(error, ApplyError::Setting(_)));
        assert!(!a.frames.is_styled(1));
        assert_eq!(*state.lock().unwrap(), "light");
    }

    #[test]
    fn without_a_journal_the_wallpaper_is_refused_but_the_frame_applies() {
        let env = env();
        let mut a = applier(&env, false, false);
        let report = a
            .apply(1, &theme(Some(&env.image), "warn"), &context(), now())
            .unwrap();
        assert_eq!(report.wallpaper, ChangeStatus::Failed);
        assert!(a.frames.is_styled(1));
    }

    #[test]
    fn invalid_theme_touches_nothing() {
        let env = env();
        let mut a = applier(&env, false, true);
        assert!(a.apply(1, "{", &context(), now()).is_err());
        assert!(!a.frames.is_styled(1));
        assert!(!a.revert(1).wallpaper.was_changed);
    }

    #[test]
    fn revert_all_restores_the_wallpaper() {
        let env = env();
        let mut a = applier(&env, false, true);
        a.apply(1, &theme(Some(&env.image), "warn"), &context(), now())
            .unwrap();
        let (frames, wallpaper, _) = a.revert_all();
        assert_eq!(frames.len(), 1);
        assert!(wallpaper.was_changed);
        assert_eq!(current(&mut a), r"C:\orig.png");
    }

    #[test]
    fn report_serializes_flat_with_a_wallpaper_status() {
        let env = env();
        let mut a = applier(&env, false, true);
        let report = a
            .apply(1, &theme(Some(&env.image), "warn"), &context(), now())
            .unwrap();
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains(r#""themeId":"t""#));
        assert!(json.contains(r#""wallpaper":"applied""#));
    }
}
