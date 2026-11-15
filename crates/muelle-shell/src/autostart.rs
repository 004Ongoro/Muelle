//! Starting Muelle when the user signs in.
//!
//! A scheduled task with a logon trigger, not the `Run` registry key: Windows
//! launches `Run` entries several seconds after the desktop appears, at low
//! priority, so the full Windows taskbar showed for a long while at every
//! sign-in. A logon task starts with the session. Creating one for yourself
//! needs no administrator rights.
//!
//! Whether autostart is on is also written to `HKCU\Software\Muelle`, because
//! task registrations cannot be read without administrator rights and the
//! watchdog has to know it instantly while Windows shuts down.

use std::{env, fs, os::windows::process::CommandExt, path::PathBuf, process::Command};

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

/// The value Muelle used to add under `Run`, before the logon task.
const LEGACY_RUN_VALUE: PCWSTR = w!("Muelle");
const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const MUELLE_KEY: PCWSTR = w!(r"Software\Muelle");
const MARKER_VALUE: PCWSTR = w!("Autostart");
/// `schtasks.exe` runs without a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Starts `exe` at every sign-in, or stops doing so (`None`).
pub fn set(exe: Option<&str>) -> windows::core::Result<()> {
    match exe {
        Some(exe) => {
            if create_task(exe) {
                delete_legacy_run_value()?;
            } else {
                // Without the Task Scheduler service, the slower `Run` entry
                // still starts the dock.
                set_legacy_run_value(exe)?;
            }
            set_marker(true)
        }
        None => {
            delete_task();
            delete_legacy_run_value()?;
            set_marker(false)
        }
    }
}

/// `true` if Muelle is set to start when the user signs in. Reads only the
/// registry, so it is instant and works while Windows shuts down.
#[must_use]
pub fn is_enabled() -> bool {
    marker() || legacy_run_value_exists()
}

/// Moves an autostart registered by an older Muelle (the `Run` key) to the
/// logon task. Updates keep the user's choice and never re-register it, so
/// the dock does it once on its own.
pub fn migrate_legacy() {
    if !legacy_run_value_exists() {
        return;
    }
    let Ok(exe) = env::current_exe() else {
        return;
    };
    let _ = set(Some(&exe.to_string_lossy()));
}

/// The task name: one per user, since task names are shared by the whole PC.
fn task_name() -> String {
    format!("Muelle ({})", env::var("USERNAME").unwrap_or_default())
}

fn schtasks(args: &[&str]) -> bool {
    let system =
        env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    Command::new(system.join(r"System32\schtasks.exe"))
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .is_ok_and(|output| output.status.success())
}

fn create_task(exe: &str) -> bool {
    let user = match (env::var("USERDOMAIN"), env::var("USERNAME")) {
        (Ok(domain), Ok(name)) => format!("{domain}\\{name}"),
        (_, Ok(name)) => name,
        _ => return false,
    };
    let path = env::temp_dir().join(format!("muelle-autostart-{}.xml", std::process::id()));
    // schtasks wants the definition as UTF-16 with a byte order mark.
    let bytes: Vec<u8> = std::iter::once(0xFEFF_u16)
        .chain(task_xml(&user, exe).encode_utf16())
        .flat_map(u16::to_le_bytes)
        .collect();
    if fs::write(&path, bytes).is_err() {
        return false;
    }
    let created = schtasks(&[
        "/Create",
        "/TN",
        &task_name(),
        "/XML",
        &path.to_string_lossy(),
        "/F",
    ]);
    let _ = fs::remove_file(&path);
    created
}

fn delete_task() {
    // Deleting a task that does not exist fails harmlessly.
    schtasks(&["/Delete", "/TN", &task_name(), "/F"]);
}

/// The task: at this user's sign-in, run `exe` as them, without admin rights,
/// at normal priority (tasks default to below normal), on battery too, with
/// no time limit.
fn task_xml(user: &str, exe: &str) -> String {
    let user = escape(user);
    let exe = escape(exe);
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Starts the Muelle dock when you sign in. Turn it off in Muelle Settings.</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>{user}</UserId>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>4</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{exe}</Command>
    </Exec>
  </Actions>
</Task>
"#
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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

fn legacy_run_value_exists() -> bool {
    // SAFETY: Only asks whether the value exists; no buffers are passed.
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            LEGACY_RUN_VALUE,
            RRF_RT_REG_SZ,
            None,
            None,
            None,
        )
        .is_ok()
    }
}

fn set_legacy_run_value(exe: &str) -> windows::core::Result<()> {
    with_run_key(|key| {
        let command: Vec<u16> = format!("\"{exe}\"").encode_utf16().chain(Some(0)).collect();
        // SAFETY: `command` lives through the call.
        unsafe {
            let bytes = std::slice::from_raw_parts(command.as_ptr().cast(), command.len() * 2);
            RegSetValueExW(key, LEGACY_RUN_VALUE, None, REG_SZ, Some(bytes))
        }
    })
}

fn delete_legacy_run_value() -> windows::core::Result<()> {
    // SAFETY: Deletes a value of the open key.
    with_run_key(
        |key| match unsafe { RegDeleteValueW(key, LEGACY_RUN_VALUE) } {
            // Removing a value that does not exist is not an error.
            WIN32_ERROR(2) => WIN32_ERROR(0),
            other => other,
        },
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_xml_should_escape_the_path_and_keep_the_user() {
        let xml = task_xml(r"PC\ana", r"C:\Apps & Tools\muelle.exe");
        assert!(xml.contains(r"<Command>C:\Apps &amp; Tools\muelle.exe</Command>"));
        assert!(xml.contains(r"<UserId>PC\ana</UserId>"));
        assert!(xml.contains("<Priority>4</Priority>"));
    }
}
