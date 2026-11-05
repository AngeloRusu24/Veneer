//! System mouse cursors through documented user32 calls.
//!
//! `SetSystemCursor` swaps a cursor in memory only. The way back is also
//! documented: `SystemParametersInfo(SPI_SETCURSORS)` reloads the user's
//! configured scheme from the registry, which Veneer never touches. So no
//! snapshot of the original is needed, and a crash is undone by the same call
//! (or by signing out).

use windows::Win32::UI::WindowsAndMessaging::{
    LoadCursorFromFileW, OCR_APPSTARTING, OCR_CROSS, OCR_HAND, OCR_HELP, OCR_IBEAM, OCR_NO,
    OCR_NORMAL, OCR_SIZEALL, OCR_SIZENESW, OCR_SIZENS, OCR_SIZENWSE, OCR_SIZEWE, OCR_UP, OCR_WAIT,
    SPI_SETCURSORS, SYSTEM_CURSOR_ID, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetSystemCursor,
    SystemParametersInfoW,
};
use windows::core::HSTRING;

use crate::backend::{SettingBackend, Snapshot};

/// Schema role name -> `OCR_*` identifier.
fn cursor_id(role: &str) -> Option<SYSTEM_CURSOR_ID> {
    let id = match role {
        "arrow" => OCR_NORMAL,
        "ibeam" => OCR_IBEAM,
        "wait" => OCR_WAIT,
        "cross" => OCR_CROSS,
        "upArrow" => OCR_UP,
        "sizeNwse" => OCR_SIZENWSE,
        "sizeNesw" => OCR_SIZENESW,
        "sizeWe" => OCR_SIZEWE,
        "sizeNs" => OCR_SIZENS,
        "sizeAll" => OCR_SIZEALL,
        "no" => OCR_NO,
        "hand" => OCR_HAND,
        "appStarting" => OCR_APPSTARTING,
        "help" => OCR_HELP,
        _ => return None,
    };
    Some(id)
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CursorBackend;

impl SettingBackend for CursorBackend {
    fn capture(&mut self) -> Result<Snapshot, String> {
        Ok(Vec::new())
    }

    fn apply(&mut self, desired: &[(String, String)]) -> Result<(), String> {
        // Resolve everything first so a bad role fails before any change.
        let roles = desired
            .iter()
            .map(|(role, path)| {
                cursor_id(role)
                    .map(|id| (id, path))
                    .ok_or_else(|| format!("unknown cursor `{role}`"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        for (id, path) in roles {
            // SAFETY: the path is a valid wide string for the call.
            let cursor = unsafe { LoadCursorFromFileW(&HSTRING::from(path.as_str())) }
                .map_err(|e| format!("{path}: {e}"))?;
            // SAFETY: on success the system takes ownership of the handle.
            unsafe { SetSystemCursor(cursor, id) }.map_err(|e| format!("{path}: {e}"))?;
        }
        Ok(())
    }

    fn restore(&mut self, _snapshot: &Snapshot) -> Vec<String> {
        // SAFETY: SPI_SETCURSORS takes no parameters.
        match unsafe {
            SystemParametersInfoW(
                SPI_SETCURSORS,
                0,
                None,
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        } {
            Ok(()) => Vec::new(),
            Err(error) => vec![error.to_string()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_schema_role_maps_to_a_system_cursor() {
        for role in [
            "arrow",
            "ibeam",
            "wait",
            "appStarting",
            "hand",
            "help",
            "cross",
            "no",
            "sizeAll",
            "sizeNs",
            "sizeWe",
            "sizeNesw",
            "sizeNwse",
            "upArrow",
        ] {
            assert!(cursor_id(role).is_some(), "{role}");
        }
        assert!(cursor_id("nope").is_none());
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        let mut backend = CursorBackend;
        let result = backend.apply(&[("arrow".into(), r"C:\definitely\missing.cur".into())]);
        assert!(result.is_err());
    }

    #[test]
    fn unknown_roles_fail_before_any_change() {
        let mut backend = CursorBackend;
        assert!(backend.apply(&[("nope".into(), "x.cur".into())]).is_err());
    }
}
