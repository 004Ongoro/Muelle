//! Watchdog process: a minimal second `muelle.exe` that waits for the dock to
//! exit (normally, after a failure, or via End Task) and then restores the
//! Windows taskbar.

use std::{env, os::windows::process::CommandExt, process::Command};

use anyhow::{Context, Result};
use muelle_shell::{power, system::set_window_transitions, taskbar, tracker::WindowTracker};
use windows::Win32::{
    Foundation::CloseHandle,
    System::Threading::{INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
};

pub const FLAG: &str = "--guard";

/// Starts without a console in its own process group so Ctrl+C in the dock's
/// terminal does not also terminate the watchdog.
const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

pub fn spawn() -> Result<()> {
    let exe = env::current_exe().context("no se pudo localizar muelle.exe")?;
    Command::new(exe)
        .args([FLAG, &std::process::id().to_string()])
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .context("no se pudo lanzar el proceso guardián")?;
    Ok(())
}

/// Watchdog body: waits for dock process `pid` and restores the taskbar.
pub fn run(pid: u32) -> Result<()> {
    // The watchdog may wait for weeks, so it should not compete for CPU or memory.
    power::run_as_background_process();
    // SAFETY: The handle is closed after the wait.
    unsafe {
        let process = OpenProcess(PROCESS_SYNCHRONIZE, false, pid)
            .context("el dock a vigilar ya no existe")?;
        WaitForSingleObject(process, INFINITE);
        let _ = CloseHandle(process);
    }
    // If the dock exited cleanly, it already restored the taskbar; repeating this is harmless.
    taskbar::give_back_on_exit();
    // The dock turns off window transitions while it runs (genie effect); if
    // it died without restoring them, do it here.
    for window in WindowTracker::default().list() {
        set_window_transitions(window.hwnd, true);
    }
    Ok(())
}
