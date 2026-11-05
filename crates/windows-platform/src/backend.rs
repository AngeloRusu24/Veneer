//! The seam between portable logic and the operating system.
//!
//! Application code talks to a [`FrameBackend`]; on Windows the implementation
//! is `dwm::DwmBackend`, in tests it is a fake that records calls.

use crate::plan::{Failure, Operation, Plan};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApplyOutcome {
    pub applied: Vec<Operation>,
    pub failed: Vec<Failure>,
}

pub trait FrameBackend {
    /// Applies `plan` to `window`, remembering the original values.
    /// Calling it again for the same window must keep the *first* originals.
    fn apply(&mut self, window: isize, plan: &Plan) -> ApplyOutcome;

    /// Puts the originals back and forgets them. Returns what could not be restored.
    fn restore(&mut self, window: isize) -> Vec<Failure>;
}

/// Wallpaper of one monitor. `path` is empty when the monitor has no image
/// (solid color or slideshow), which cannot be restored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorWallpaper {
    pub monitor_id: String,
    pub path: String,
}

pub trait WallpaperBackend {
    fn capture(&mut self) -> Result<Vec<MonitorWallpaper>, String>;
    fn set_all(&mut self, image: &std::path::Path) -> Result<(), String>;
    /// Returns a description of each monitor that could not be restored.
    fn restore(&mut self, snapshot: &[MonitorWallpaper]) -> Vec<String>;
}

/// `(name, value)` pairs. Used both for what a theme asks for and for the
/// original state captured before a change, so one journaled controller can
/// drive several simple global settings.
pub type Settings = Vec<(String, String)>;
pub type Snapshot = Vec<(String, Option<String>)>;

/// A global, user-level setting changed through a documented API.
pub trait SettingBackend {
    /// Reads the current state so it can be put back.
    fn capture(&mut self) -> Result<Snapshot, String>;
    /// Applies the requested values; may leave partial changes on error,
    /// in which case the caller calls [`SettingBackend::restore`].
    fn apply(&mut self, desired: &[(String, String)]) -> Result<(), String>;
    /// Returns a description of everything that could not be restored.
    fn restore(&mut self, snapshot: &Snapshot) -> Vec<String>;
}

impl<T: SettingBackend + ?Sized> SettingBackend for Box<T> {
    fn capture(&mut self) -> Result<Snapshot, String> {
        (**self).capture()
    }
    fn apply(&mut self, desired: &[(String, String)]) -> Result<(), String> {
        (**self).apply(desired)
    }
    fn restore(&mut self, snapshot: &Snapshot) -> Vec<String> {
        (**self).restore(snapshot)
    }
}
