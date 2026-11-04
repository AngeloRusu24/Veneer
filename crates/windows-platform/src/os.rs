//! Operating system version and local time.

use veneer_theme_schema::TimeOfDay;
use windows::Wdk::System::SystemServices::RtlGetVersion;
use windows::Win32::System::SystemInformation::{GetLocalTime, OSVERSIONINFOW};

/// The current local time of day, with minute precision.
pub fn local_time_of_day() -> Option<TimeOfDay> {
    // SAFETY: GetLocalTime has no preconditions and returns a plain struct.
    let now = unsafe { GetLocalTime() };
    TimeOfDay::from_minutes(now.wHour * 60 + now.wMinute)
}

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
