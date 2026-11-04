//! Operating system version detection.

use windows::Wdk::System::SystemServices::RtlGetVersion;
use windows::Win32::System::SystemInformation::OSVERSIONINFOW;

/// The Windows build number (e.g. 22631), or `None` if it cannot be read.
///
/// Uses `RtlGetVersion`, which unlike `GetVersionEx` is not subject to
/// application-manifest version lies.
pub fn build_number() -> Option<u32> {
    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is a properly sized, initialised OSVERSIONINFOW.
    let status = unsafe { RtlGetVersion(&mut info) };
    status.is_ok().then_some(info.dwBuildNumber)
}
