//! Starting Muelle when the user signs in, with the `Run` registry key: what
//! most desktop apps use, and what security software expects from them.
//!
//! Versions 0.1.7 and 0.1.8 used a logon scheduled task instead, created by
//! running `schtasks.exe` hidden. That looks like a trojan setting itself up
//! to persist, and some antivirus programs deleted Muelle for it. Those tasks
//! are moved back to `Run` once (`migrate_legacy`), deleting the task through
//! the Task Scheduler API rather than another hidden process.
//!
//! Whether autostart is on is also written to `HKCU\Software\Muelle`, so the
//! watchdog knows it instantly while Windows shuts down.

use std::env;

use windows::{
    Win32::{
        Foundation::WIN32_ERROR,
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ,
            RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteValueW,
            RegGetValueW, RegOpenKeyExW, RegSetValueExW,
        },
    },
    core::{PCWSTR, w},
};

const RUN_VALUE: PCWSTR = w!("Muelle");
const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const MUELLE_KEY: PCWSTR = w!(r"Software\Muelle");
const MARKER_VALUE: PCWSTR = w!("Autostart");

/// Starts `exe` at every sign-in, or stops doing so (`None`).
pub fn set(exe: Option<&str>) -> windows::core::Result<()> {
    match exe {
        Some(exe) => set_run_value(exe)?,
        None => {
            delete_run_value()?;
            // A task left by 0.1.7 or 0.1.8 must not keep starting Muelle.
            delete_legacy_task();
        }
    }
    set_marker(exe.is_some())
}

/// `true` if Muelle is set to start when the user signs in. Reads only the
/// registry, so it is instant and works while Windows shuts down.
#[must_use]
pub fn is_enabled() -> bool {
    marker() || run_value_exists()
}

/// Moves the logon task of Muelle 0.1.7 and 0.1.8 back to the `Run` key:
/// autostart is on (the marker) but there is no `Run` value. Updates keep
/// the user's choice and never re-register it, so the dock does it once.
pub fn migrate_legacy() {
    if !marker() || run_value_exists() {
        return;
    }
    let Ok(exe) = env::current_exe() else {
        return;
    };
    if set_run_value(&exe.to_string_lossy()).is_ok() {
        delete_legacy_task();
    }
}

/// Removes the logon task older versions created, through the Task
/// Scheduler's own API. Fails harmlessly if there is none.
fn delete_legacy_task() {
    use windows::{
        Win32::System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
            },
            TaskScheduler::{ITaskService, TaskScheduler},
            Variant::VARIANT,
        },
        core::BSTR,
    };
    // One task per user, since task names are shared by the whole PC.
    let name = format!("Muelle ({})", env::var("USERNAME").unwrap_or_default());
    // SAFETY: COM is initialized for this call and released after; the
    // service and folder are released before.
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let deleted = (|| -> windows::core::Result<()> {
            let service: ITaskService =
                CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)?;
            let none = VARIANT::default();
            service.Connect(&none, &none, &none, &none)?;
            service
                .GetFolder(&BSTR::from("\\"))?
                .DeleteTask(&BSTR::from(name), 0)
        })();
        let _ = deleted;
        if initialized {
            CoUninitialize();
        }
    }
}

fn marker() -> bool {
    let mut value = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: `value` and `size` describe a valid 4-byte buffer.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            MUELLE_KEY,
            MARKER_VALUE,
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut value).cast()),
            Some(&mut size),
        )
    };
    status.is_ok() && value != 0
}

fn set_marker(on: bool) -> windows::core::Result<()> {
    let mut key = HKEY::default();
    let value = u32::from(on).to_le_bytes();
    // SAFETY: The key is closed before returning; `value` lives through the call.
    unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            MUELLE_KEY,
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
        .ok()?;
        let result = RegSetValueExW(key, MARKER_VALUE, None, REG_DWORD, Some(&value));
        let _ = RegCloseKey(key);
        result.ok()
    }
}

fn run_value_exists() -> bool {
    // SAFETY: Only asks whether the value exists; no buffers are passed.
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            RRF_RT_REG_SZ,
            None,
            None,
            None,
        )
        .is_ok()
    }
}

fn set_run_value(exe: &str) -> windows::core::Result<()> {
    with_run_key(|key| {
        let command: Vec<u16> = format!("\"{exe}\"").encode_utf16().chain(Some(0)).collect();
        // SAFETY: `command` lives through the call.
        unsafe {
            let bytes = std::slice::from_raw_parts(command.as_ptr().cast(), command.len() * 2);
            RegSetValueExW(key, RUN_VALUE, None, REG_SZ, Some(bytes))
        }
    })
}

fn delete_run_value() -> windows::core::Result<()> {
    // SAFETY: Deletes a value of the open key.
    with_run_key(|key| match unsafe { RegDeleteValueW(key, RUN_VALUE) } {
        // Removing a value that does not exist is not an error.
        WIN32_ERROR(2) => WIN32_ERROR(0),
        other => other,
    })
}

fn with_run_key(action: impl FnOnce(HKEY) -> WIN32_ERROR) -> windows::core::Result<()> {
    let mut key = HKEY::default();
    // SAFETY: The key is closed before returning.
    unsafe {
        RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, KEY_SET_VALUE, &mut key).ok()?;
        let result = action(key);
        let _ = RegCloseKey(key);
        result.ok()
    }
}
