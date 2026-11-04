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
