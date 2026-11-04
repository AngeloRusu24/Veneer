//! One call to apply a whole theme (window frame + wallpaper), one to undo it.

use std::path::Path;

use serde::Serialize;
use veneer_rule_engine::WindowContext;
use veneer_theme_schema::{TimeOfDay, UnsupportedPolicy, parse_theme};
use veneer_windows_platform::backend::{FrameBackend, WallpaperBackend};

use crate::{
    ApplyError, ApplyReport, Controller, RevertReport,
    wallpaper::{WallpaperController, WallpaperRevert},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WallpaperStatus {
    /// The theme has no wallpaper and none was active.
    NotRequested,
    Applied,
    /// The theme has no wallpaper, so the previous theme's one was undone.
    Reverted,
    /// Could not be set; the rest of the theme is applied (policy `warn`).
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeReport {
    #[serde(flatten)]
    pub frame: ApplyReport,
    pub wallpaper: WallpaperStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeRevert {
    pub frame: RevertReport,
    pub wallpaper: WallpaperRevert,
}

#[derive(Debug)]
pub struct ThemeApplier<F, W> {
    frames: Controller<F>,
    /// `None` when the journal could not be opened: wallpaper changes are then
    /// refused rather than made without a way back.
    wallpaper: Option<WallpaperController<W>>,
}

impl<F: FrameBackend, W: WallpaperBackend> ThemeApplier<F, W> {
    pub fn new(frames: Controller<F>, wallpaper: Option<WallpaperController<W>>) -> Self {
        Self { frames, wallpaper }
    }

    /// Applies the window-frame part, then the wallpaper. Applying a theme
    /// replaces the previous one completely, wallpaper included.
    ///
    /// With policy `error`, a wallpaper failure rolls the frame back and
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

        let wallpaper = match &theme.wallpaper {
            Some(wallpaper) => match self.set_wallpaper(Path::new(&wallpaper.path)) {
                Ok(()) => WallpaperStatus::Applied,
                Err(reason) if theme.unsupported == UnsupportedPolicy::Error => {
                    self.frames.revert(window);
                    return Err(ApplyError::Wallpaper(reason));
                }
                Err(reason) => {
                    frame.warnings.push(reason);
                    WallpaperStatus::Failed
                }
            },
            None => match self.wallpaper.as_mut().map(WallpaperController::revert) {
                Some(result) if !result.failed.is_empty() => {
                    frame.warnings.push(format!(
                        "sfondo precedente non ripristinato: {}",
                        result.failed.join("; ")
                    ));
                    WallpaperStatus::Failed
                }
                Some(result) if result.was_changed => WallpaperStatus::Reverted,
                _ => WallpaperStatus::NotRequested,
            },
        };
        Ok(ThemeReport { frame, wallpaper })
    }

    fn set_wallpaper(&mut self, path: &Path) -> Result<(), String> {
        self.wallpaper
            .as_mut()
            .ok_or_else(|| "sfondo non disponibile: journal non aperto".to_owned())?
            .set(path)
            .map_err(|error| error.to_string())
    }

    pub fn revert(&mut self, window: isize) -> ThemeRevert {
        ThemeRevert {
            frame: self.frames.revert(window),
            wallpaper: self.revert_wallpaper(),
        }
    }

    /// Undoes every style and the wallpaper; used when the app exits.
    pub fn revert_all(&mut self) -> (Vec<RevertReport>, WallpaperRevert) {
        (self.frames.revert_all(), self.revert_wallpaper())
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
        backend::{ApplyOutcome, MonitorWallpaper},
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
        assert_eq!(report.wallpaper, WallpaperStatus::Applied);
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
        assert_eq!(report.wallpaper, WallpaperStatus::Reverted);
        assert_eq!(current(&mut a), r"C:\orig.png");

        let again = a.apply(1, &theme(None, "warn"), &context(), now()).unwrap();
        assert_eq!(again.wallpaper, WallpaperStatus::NotRequested);
    }

    #[test]
    fn warn_policy_keeps_the_frame_when_the_wallpaper_fails() {
        let env = env();
        let mut a = applier(&env, true, true);
        let report = a
            .apply(1, &theme(Some(&env.image), "warn"), &context(), now())
            .unwrap();
        assert_eq!(report.wallpaper, WallpaperStatus::Failed);
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
    fn without_a_journal_the_wallpaper_is_refused_but_the_frame_applies() {
        let env = env();
        let mut a = applier(&env, false, false);
        let report = a
            .apply(1, &theme(Some(&env.image), "warn"), &context(), now())
            .unwrap();
        assert_eq!(report.wallpaper, WallpaperStatus::Failed);
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
        let (frames, wallpaper) = a.revert_all();
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
