//! Per-user autostart through the documented `HKCU\...\Run` registry key.
//!
//! No scheduled tasks, services or admin rights are involved, and the entry is
//! visible to the user in Windows' own Startup apps settings page.

use windows::{
    Win32::{
        Foundation::ERROR_SUCCESS,
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE,
            REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegQueryValueExW,
            RegSetValueExW,
        },
    },
    core::{HSTRING, PCWSTR},
};

pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

#[derive(Debug, Clone)]
pub struct Autostart {
    subkey: String,
    value_name: String,
}

impl Autostart {
    /// Autostart entry in the real per-user Run key.
    pub fn new(value_name: &str) -> Self {
        Self::in_key(RUN_KEY, value_name)
    }

    /// Entry in an arbitrary HKCU subkey (used by tests to stay isolated).
    pub fn in_key(subkey: &str, value_name: &str) -> Self {
        Self {
            subkey: subkey.to_owned(),
            value_name: value_name.to_owned(),
        }
    }

    pub fn enable(&self, command: &str) -> Result<(), windows::core::Error> {
        let key = self.open(KEY_SET_VALUE)?;
        let wide: Vec<u8> = command
            .encode_utf16()
            .chain(std::iter::once(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        // SAFETY: `key` is a valid open handle and `wide` outlives the call.
        let status = unsafe {
            RegSetValueExW(
                key.0,
                &HSTRING::from(self.value_name.as_str()),
                None,
                REG_SZ,
                Some(&wide),
            )
        };
        status.ok()
    }

    /// Removing an entry that does not exist is not an error.
    pub fn disable(&self) -> Result<(), windows::core::Error> {
        let key = self.open(KEY_SET_VALUE)?;
        // SAFETY: `key` is a valid open handle.
        let status = unsafe { RegDeleteValueW(key.0, &HSTRING::from(self.value_name.as_str())) };
        if status.0 == 2 {
            return Ok(());
        }
        status.ok()
    }

    pub fn is_enabled(&self) -> bool {
        let Ok(key) = self.open(KEY_QUERY_VALUE) else {
            return false;
        };
        // SAFETY: `key` is a valid open handle; all optional outputs are null.
        let status = unsafe {
            RegQueryValueExW(
                key.0,
                &HSTRING::from(self.value_name.as_str()),
                None,
                None,
                None,
                None,
            )
        };
        status == ERROR_SUCCESS
    }

    fn open(
        &self,
        access: windows::Win32::System::Registry::REG_SAM_FLAGS,
    ) -> Result<OwnedKey, windows::core::Error> {
        let mut key = HKEY::default();
        // SAFETY: the subkey string and output handle are valid for the call.
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                &HSTRING::from(self.subkey.as_str()),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                access,
                None,
                &mut key,
                None,
            )
        };
        status.ok()?;
        Ok(OwnedKey(key))
    }
}

struct OwnedKey(HKEY);

impl Drop for OwnedKey {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by us and is closed exactly once.
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated() -> Autostart {
        Autostart::in_key(
            &format!(r"Software\VeneerTests\autostart-{}", std::process::id()),
            "Veneer",
        )
    }

    #[test]
    fn enable_disable_roundtrip() {
        let entry = isolated();
        assert!(!entry.is_enabled());
        entry
            .enable(r#""C:\Veneer\veneer.exe" --minimized"#)
            .unwrap();
        assert!(entry.is_enabled());
        entry.disable().unwrap();
        assert!(!entry.is_enabled());
    }

    #[test]
    fn disabling_a_missing_entry_is_ok() {
        isolated().disable().unwrap();
    }
}
