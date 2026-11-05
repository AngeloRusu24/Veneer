//! Light/dark mode of apps and of the system UI.
//!
//! Windows exposes no function for this; the per-user values
//! `AppsUseLightTheme` / `SystemUsesLightTheme` under `HKCU\...\Personalize`
//! are what the Settings app itself writes, followed by a documented
//! `WM_SETTINGCHANGE` broadcast. It is a convention rather than a contract, so
//! a future Windows version may ignore it (see ADR 0005).

use windows::{
    Win32::{
        Foundation::{LPARAM, WPARAM},
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_DWORD,
            REG_OPTION_NON_VOLATILE, REG_VALUE_TYPE, RegCloseKey, RegCreateKeyExW, RegDeleteValueW,
            RegQueryValueExW, RegSetValueExW,
        },
        UI::WindowsAndMessaging::{
            HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
        },
    },
    core::{HSTRING, PCWSTR},
};

use crate::backend::{SettingBackend, Snapshot};

pub const PERSONALIZE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

const APPS: &str = "AppsUseLightTheme";
const SYSTEM: &str = "SystemUsesLightTheme";

#[derive(Debug, Clone)]
pub struct AppearanceBackend {
    subkey: String,
    broadcast: bool,
}

impl Default for AppearanceBackend {
    fn default() -> Self {
        Self {
            subkey: PERSONALIZE_KEY.to_owned(),
            broadcast: true,
        }
    }
}

impl AppearanceBackend {
    /// Uses another HKCU subkey and does not broadcast (tests stay isolated).
    pub fn in_key(subkey: &str) -> Self {
        Self {
            subkey: subkey.to_owned(),
            broadcast: false,
        }
    }

    fn open(
        &self,
        access: windows::Win32::System::Registry::REG_SAM_FLAGS,
    ) -> Result<HKEY, String> {
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
        status.ok().map_err(|e| e.to_string())?;
        Ok(key)
    }

    fn read(&self, name: &str) -> Result<Option<u32>, String> {
        let key = self.open(KEY_QUERY_VALUE)?;
        let mut kind = REG_VALUE_TYPE::default();
        let mut data = [0u8; 4];
        let mut size = 4u32;
        // SAFETY: `key` is open; the buffers are valid and `size` matches `data`.
        let status = unsafe {
            RegQueryValueExW(
                key,
                &HSTRING::from(name),
                None,
                Some(&mut kind),
                Some(data.as_mut_ptr()),
                Some(&mut size),
            )
        };
        // SAFETY: closing the handle opened above, once.
        unsafe {
            let _ = RegCloseKey(key);
        }
        match status.0 {
            0 if kind == REG_DWORD && size == 4 => Ok(Some(u32::from_le_bytes(data))),
            0 => Err(format!("{name} has an unexpected type")),
            2 => Ok(None),
            code => Err(format!("registry error {code} reading {name}")),
        }
    }

    fn write(&self, name: &str, value: Option<u32>) -> Result<(), String> {
        let key = self.open(KEY_SET_VALUE)?;
        let name = HSTRING::from(name);
        // SAFETY: `key` is open and the data slice outlives the call.
        let status = unsafe {
            match value {
                Some(v) => RegSetValueExW(key, &name, None, REG_DWORD, Some(&v.to_le_bytes())),
                None => RegDeleteValueW(key, &name),
            }
        };
        // SAFETY: closing the handle opened above, once.
        unsafe {
            let _ = RegCloseKey(key);
        }
        // Deleting a value that is already absent is fine.
        if value.is_none() && status.0 == 2 {
            return Ok(());
        }
        status.ok().map_err(|e| e.to_string())
    }

    fn notify(&self) {
        if !self.broadcast {
            return;
        }
        let area = HSTRING::from("ImmersiveColorSet");
        // SAFETY: `area` outlives the call; the timeout keeps a hung window
        // from blocking us.
        unsafe {
            let _ = SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(area.as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                200,
                None,
            );
        }
    }
}

fn value_name(key: &str) -> Option<&'static str> {
    match key {
        "apps" => Some(APPS),
        "system" => Some(SYSTEM),
        _ => None,
    }
}

impl SettingBackend for AppearanceBackend {
    fn capture(&mut self) -> Result<Snapshot, String> {
        [APPS, SYSTEM]
            .into_iter()
            .map(|name| Ok((name.to_owned(), self.read(name)?.map(|v| v.to_string()))))
            .collect()
    }

    fn apply(&mut self, desired: &[(String, String)]) -> Result<(), String> {
        for (key, mode) in desired {
            let name = value_name(key).ok_or_else(|| format!("unknown setting `{key}`"))?;
            let light = match mode.as_str() {
                "light" => 1,
                "dark" => 0,
                other => return Err(format!("unknown mode `{other}`")),
            };
            self.write(name, Some(light))?;
        }
        self.notify();
        Ok(())
    }

    fn restore(&mut self, snapshot: &Snapshot) -> Vec<String> {
        let mut failed = Vec::new();
        for (name, original) in snapshot {
            let value = original.as_deref().and_then(|v| v.parse::<u32>().ok());
            if let Err(error) = self.write(name, value) {
                failed.push(format!("{name}: {error}"));
            }
        }
        self.notify();
        failed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: &str = r"Software\VeneerTests\Appearance";

    #[test]
    fn apply_then_restore_returns_the_original_values() {
        let mut backend = AppearanceBackend::in_key(TEST_KEY);
        backend.write(APPS, Some(1)).unwrap();
        backend.write(SYSTEM, None).unwrap();

        let snapshot = backend.capture().unwrap();
        assert_eq!(snapshot[0].1.as_deref(), Some("1"));
        assert_eq!(snapshot[1].1, None);

        backend
            .apply(&[
                ("apps".into(), "dark".into()),
                ("system".into(), "dark".into()),
            ])
            .unwrap();
        assert_eq!(backend.read(APPS).unwrap(), Some(0));
        assert_eq!(backend.read(SYSTEM).unwrap(), Some(0));

        assert!(backend.restore(&snapshot).is_empty());
        assert_eq!(backend.read(APPS).unwrap(), Some(1));
        assert_eq!(backend.read(SYSTEM).unwrap(), None);
    }

    #[test]
    fn rejects_unknown_keys_and_modes() {
        let mut backend = AppearanceBackend::in_key(TEST_KEY);
        assert!(backend.apply(&[("x".into(), "dark".into())]).is_err());
        assert!(backend.apply(&[("apps".into(), "blue".into())]).is_err());
    }
}
