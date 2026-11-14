use windows::{
    Win32::{
        Foundation::WIN32_ERROR,
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegDeleteValueW,
            RegOpenKeyExW, RegSetValueExW,
        },
    },
    core::{HSTRING, w},
};

const VALUE_NAME: &str = "Muelle";

pub fn set(exe: Option<&str>) -> windows::core::Result<()> {
    let mut key = HKEY::default();
    // SAFETY: The key is closed before returning, and strings live through each call.
    unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
            None,
            KEY_SET_VALUE,
            &mut key,
        )
        .ok()?;
        let result = match exe {
            Some(path) => {
                let command: Vec<u16> = format!("\"{path}\"")
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                let bytes = std::slice::from_raw_parts(command.as_ptr().cast(), command.len() * 2);
                RegSetValueExW(key, &HSTRING::from(VALUE_NAME), None, REG_SZ, Some(bytes))
            }
            None => match RegDeleteValueW(key, &HSTRING::from(VALUE_NAME)) {
                // Removing a value that does not exist is not an error.
                WIN32_ERROR(2) => WIN32_ERROR(0),
                other => other,
            },
        };
        let _ = RegCloseKey(key);
        result.ok()
    }
}

/// `true` if Muelle is registered to start when the user signs in.
#[must_use]
pub fn is_enabled() -> bool {
    use windows::Win32::System::Registry::{RRF_RT_REG_SZ, RegGetValueW};
    // SAFETY: Only asks whether the value exists; no buffers are passed.
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Run"),
            &HSTRING::from(VALUE_NAME),
            RRF_RT_REG_SZ,
            None,
            None,
            None,
        )
        .is_ok()
    }
}
