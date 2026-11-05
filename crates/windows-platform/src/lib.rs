//! Windows integration layer.
//!
//! Everything that touches Win32 lives here, behind small functions, so the
//! rest of Veneer stays pure and testable. The decision of *what* to apply is
//! in [`plan`] (portable); the calls that *do* it are in `dwm` (Windows only).

pub mod backend;
pub mod plan;

#[cfg(windows)]
pub mod appearance;
#[cfg(windows)]
pub mod autostart;
#[cfg(windows)]
pub mod cursors;
#[cfg(windows)]
pub mod dwm;
#[cfg(windows)]
pub mod os;
#[cfg(windows)]
pub mod wallpaper;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityStatus {
    Supported,
    Unsupported,
    NotDetected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    WindowFrameStyling,
    GlobalDisplayFilter,
    AnimatedDesktopWallpaper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityReport {
    pub capability: Capability,
    pub status: CapabilityStatus,
}

/// Capability report for a given Windows build (`None` = could not detect).
///
/// Window-frame styling counts as supported from the first build that has any
/// DWM attribute we use (dark title bars); individual properties are further
/// filtered by [`plan::plan_window_frame`].
pub fn capabilities_for_build(build: Option<u32>) -> [CapabilityReport; 3] {
    let window_frame = match build {
        None => CapabilityStatus::NotDetected,
        Some(b) if b >= plan::BUILD_DARK_MODE => CapabilityStatus::Supported,
        Some(_) => CapabilityStatus::Unsupported,
    };
    [
        CapabilityReport {
            capability: Capability::WindowFrameStyling,
            status: window_frame,
        },
        // No documented API lets a third party filter the whole desktop output.
        CapabilityReport {
            capability: Capability::GlobalDisplayFilter,
            status: CapabilityStatus::Unsupported,
        },
        // IDesktopWallpaper only supports static images and slideshows.
        CapabilityReport {
            capability: Capability::AnimatedDesktopWallpaper,
            status: CapabilityStatus::Unsupported,
        },
    ]
}

/// Capability report for the machine we are running on.
pub fn current_capabilities() -> [CapabilityReport; 3] {
    #[cfg(windows)]
    let build = os::build_number();
    #[cfg(not(windows))]
    let build = None;
    capabilities_for_build(build)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window_frame(build: Option<u32>) -> CapabilityStatus {
        capabilities_for_build(build)[0].status
    }

    #[test]
    fn window_frame_support_depends_on_build() {
        assert_eq!(window_frame(None), CapabilityStatus::NotDetected);
        assert_eq!(window_frame(Some(18363)), CapabilityStatus::Unsupported);
        assert_eq!(window_frame(Some(19041)), CapabilityStatus::Supported);
        assert_eq!(window_frame(Some(26100)), CapabilityStatus::Supported);
    }

    #[test]
    fn global_features_are_never_claimed() {
        for build in [None, Some(19045), Some(26100)] {
            let report = capabilities_for_build(build);
            assert_eq!(report[1].status, CapabilityStatus::Unsupported);
            assert_eq!(report[2].status, CapabilityStatus::Unsupported);
        }
    }
}
