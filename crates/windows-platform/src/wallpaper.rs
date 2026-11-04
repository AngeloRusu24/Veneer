//! Static desktop wallpaper through the documented `IDesktopWallpaper` COM
//! interface (per monitor). Animated wallpapers are not offered by this API.

use std::path::Path;

use crate::backend::{MonitorWallpaper, WallpaperBackend};

use windows::{
    Win32::{
        System::Com::{
            CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
        },
        UI::Shell::{DesktopWallpaper, IDesktopWallpaper},
    },
    core::{HSTRING, PCWSTR, PWSTR},
};

pub type Error = windows::core::Error;

fn api() -> Result<IDesktopWallpaper, Error> {
    // S_FALSE / RPC_E_CHANGED_MODE only mean COM is already initialised on this
    // thread, which is all we need.
    // SAFETY: plain COM initialisation for the calling thread.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    // SAFETY: the class and interface are statically matched.
    unsafe { CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL) }
}

fn take_string(value: PWSTR) -> String {
    if value.is_null() {
        return String::new();
    }
    // SAFETY: COM returned a valid NUL-terminated string that we free once.
    unsafe {
        let text = value.to_string().unwrap_or_default();
        CoTaskMemFree(Some(value.0.cast()));
        text
    }
}

/// Reads the current wallpaper of every attached monitor.
pub fn capture() -> Result<Vec<MonitorWallpaper>, Error> {
    let api = api()?;
    // SAFETY: `api` is a valid interface; returned strings are freed in `take_string`.
    unsafe {
        let count = api.GetMonitorDevicePathCount()?;
        let mut monitors = Vec::with_capacity(count as usize);
        for index in 0..count {
            let id = api.GetMonitorDevicePathAt(index)?;
            let id = take_string(id);
            let path = api
                .GetWallpaper(&HSTRING::from(id.as_str()))
                .map(take_string)
                .unwrap_or_default();
            monitors.push(MonitorWallpaper {
                monitor_id: id,
                path,
            });
        }
        Ok(monitors)
    }
}

/// Sets one monitor's wallpaper, or all monitors when `monitor_id` is `None`.
pub fn set(monitor_id: Option<&str>, image: &Path) -> Result<(), Error> {
    let api = api()?;
    let id = monitor_id.map(HSTRING::from);
    let monitor = id.as_ref().map_or(PCWSTR::null(), |id| PCWSTR(id.as_ptr()));
    let image = HSTRING::from(image.as_os_str());
    // SAFETY: both strings outlive the call.
    unsafe { api.SetWallpaper(monitor, &image) }
}

/// Puts back what [`capture`] returned. Monitors that were captured without an
/// image are skipped; the first error does not stop the others.
pub fn restore(snapshot: &[MonitorWallpaper]) -> Vec<(String, Error)> {
    snapshot
        .iter()
        .filter(|entry| !entry.path.is_empty())
        .filter_map(|entry| {
            set(Some(&entry.monitor_id), Path::new(&entry.path))
                .err()
                .map(|error| (entry.monitor_id.clone(), error))
        })
        .collect()
}

/// [`WallpaperBackend`] backed by `IDesktopWallpaper`.
#[derive(Debug, Default)]
pub struct DesktopWallpaperBackend;

impl WallpaperBackend for DesktopWallpaperBackend {
    fn capture(&mut self) -> Result<Vec<MonitorWallpaper>, String> {
        capture().map_err(|error| error.to_string())
    }

    fn set_all(&mut self, image: &Path) -> Result<(), String> {
        set(None, image).map_err(|error| error.to_string())
    }

    fn restore(&mut self, snapshot: &[MonitorWallpaper]) -> Vec<String> {
        restore(snapshot)
            .into_iter()
            .map(|(monitor, error)| format!("{monitor}: {error}"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_then_restore_leaves_the_wallpaper_unchanged() {
        let Ok(before) = capture() else {
            // No desktop session (e.g. a service account): nothing to verify.
            return;
        };
        let failures = restore(&before);
        assert!(failures.is_empty(), "{failures:?}");
        assert_eq!(capture().unwrap(), before);
    }
}
