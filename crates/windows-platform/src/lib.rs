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

pub fn current_capabilities() -> [CapabilityReport; 3] {
    [
        CapabilityReport {
            capability: Capability::WindowFrameStyling,
            status: CapabilityStatus::NotDetected,
        },
        CapabilityReport {
            capability: Capability::GlobalDisplayFilter,
            status: CapabilityStatus::Unsupported,
        },
        CapabilityReport {
            capability: Capability::AnimatedDesktopWallpaper,
            status: CapabilityStatus::Unsupported,
        },
    ]
}
