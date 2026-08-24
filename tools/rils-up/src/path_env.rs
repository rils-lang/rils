use std::path::Path;

pub(crate) fn ensure_on_user_path(directory: &Path) -> Result<bool, String> {
    imp::ensure_on_user_path(directory)
}

fn append_path_entry(current: &str, directory: &str) -> Option<String> {
    let wanted = normalize_path_entry(directory);
    if current
        .split(';')
        .any(|entry| normalize_path_entry(entry) == wanted)
    {
        return None;
    }
    let current = current.trim_end_matches(';');
    Some(if current.is_empty() {
        directory.to_owned()
    } else {
        format!("{current};{directory}")
    })
}

fn normalize_path_entry(entry: &str) -> String {
    entry
        .trim()
        .trim_matches('"')
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

#[cfg(windows)]
mod imp {
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt, path::Path, ptr};

    use windows_sys::Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_EXPAND_SZ, REG_SZ,
            RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
        },
        UI::WindowsAndMessaging::{
            HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
        },
    };

    use super::append_path_entry;

    const ENVIRONMENT_KEY: &str = "Environment";
    const PATH_VALUE: &str = "Path";

    pub(super) fn ensure_on_user_path(directory: &Path) -> Result<bool, String> {
        let directory = directory.to_str().ok_or_else(|| {
            format!(
                "Rils bin path is not valid Unicode: {}",
                directory.display()
            )
        })?;
        let key = RegistryKey::open_environment()?;
        let (current, value_type) = key.read_path()?;
        let Some(updated) = append_path_entry(&current, directory) else {
            return Ok(false);
        };
        key.write_path(&updated, value_type)?;
        broadcast_environment_change();
        Ok(true)
    }

    struct RegistryKey(HKEY);

    impl RegistryKey {
        fn open_environment() -> Result<Self, String> {
            let subkey = wide(ENVIRONMENT_KEY);
            let mut key = ptr::null_mut();
            let status = unsafe {
                RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    subkey.as_ptr(),
                    0,
                    KEY_QUERY_VALUE | KEY_SET_VALUE,
                    &mut key,
                )
            };
            if status != ERROR_SUCCESS {
                return Err(format!(
                    "Could not open the user Environment registry key: {}",
                    std::io::Error::from_raw_os_error(status as i32)
                ));
            }
            Ok(Self(key))
        }

        fn read_path(&self) -> Result<(String, u32), String> {
            let name = wide(PATH_VALUE);
            let mut value_type = REG_EXPAND_SZ;
            let mut byte_count = 0_u32;
            let status = unsafe {
                RegQueryValueExW(
                    self.0,
                    name.as_ptr(),
                    ptr::null(),
                    &mut value_type,
                    ptr::null_mut(),
                    &mut byte_count,
                )
            };
            if status == ERROR_FILE_NOT_FOUND {
                return Ok((String::new(), REG_EXPAND_SZ));
            }
            if status != ERROR_SUCCESS {
                return Err(registry_error("read the user PATH", status));
            }
            if value_type != REG_SZ && value_type != REG_EXPAND_SZ {
                return Err("The user PATH registry value is not a string".to_owned());
            }
            let mut bytes = vec![0_u8; byte_count as usize];
            let status = unsafe {
                RegQueryValueExW(
                    self.0,
                    name.as_ptr(),
                    ptr::null(),
                    &mut value_type,
                    bytes.as_mut_ptr(),
                    &mut byte_count,
                )
            };
            if status != ERROR_SUCCESS {
                return Err(registry_error("read the user PATH", status));
            }
            let units = bytes[..byte_count as usize]
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .take_while(|unit| *unit != 0)
                .collect::<Vec<_>>();
            Ok((String::from_utf16_lossy(&units), value_type))
        }

        fn write_path(&self, value: &str, value_type: u32) -> Result<(), String> {
            let name = wide(PATH_VALUE);
            let value = wide(value);
            let status = unsafe {
                RegSetValueExW(
                    self.0,
                    name.as_ptr(),
                    0,
                    value_type,
                    value.as_ptr().cast(),
                    (value.len() * size_of::<u16>()) as u32,
                )
            };
            if status != ERROR_SUCCESS {
                return Err(registry_error("update the user PATH", status));
            }
            Ok(())
        }
    }

    impl Drop for RegistryKey {
        fn drop(&mut self) {
            unsafe { RegCloseKey(self.0) };
        }
    }

    fn broadcast_environment_change() {
        let environment = wide("Environment");
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                environment.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                5_000,
                ptr::null_mut(),
            )
        };
    }

    fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
        value.as_ref().encode_wide().chain(Some(0)).collect()
    }

    fn registry_error(action: &str, status: u32) -> String {
        format!(
            "Could not {action}: {}",
            std::io::Error::from_raw_os_error(status as i32)
        )
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::Path;

    pub(super) fn ensure_on_user_path(_directory: &Path) -> Result<bool, String> {
        Err("Automatic PATH configuration is currently supported only on Windows; rerun with --no-path".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::append_path_entry;

    #[test]
    fn appends_a_missing_path_without_duplicate_separators() {
        assert_eq!(
            append_path_entry(r"C:\Tools;", r"C:\Users\me\.rils\bin"),
            Some(r"C:\Tools;C:\Users\me\.rils\bin".to_owned())
        );
    }

    #[test]
    fn recognizes_equivalent_windows_path_entries() {
        assert_eq!(
            append_path_entry(
                r#"C:\Tools;"C:\Users\ME\.rils\bin\""#,
                r"C:/Users/me/.rils/bin"
            ),
            None
        );
    }
}
