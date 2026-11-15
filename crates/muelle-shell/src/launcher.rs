use thiserror::Error;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        UI::{
            Input::KeyboardAndMouse::{
                INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT,
                KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_A, VK_LWIN, VK_N,
            },
            Shell::ShellExecuteW,
            WindowsAndMessaging::{
                ASFW_ANY, AllowSetForegroundWindow, GetForegroundWindow, HWND_TOP, IsIconic,
                IsWindowVisible, PostMessageW, SW_MINIMIZE, SW_RESTORE, SW_SHOWNORMAL,
                SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetForegroundWindow, SetWindowPos,
                ShowWindow, ShowWindowAsync, SwitchToThisWindow, WM_CLOSE,
            },
        },
    },
    core::{HSTRING, w},
};

#[derive(Debug, Error)]
#[error("no se pudo abrir `{id}` (código {code} del shell)")]
pub struct LaunchError {
    id: String,
    code: isize,
}

/// The dock never takes the focus, so Windows may refuse the foreground to the
/// app it starts and open it behind other windows. The click that asked for it
/// was the dock's, which lets the dock hand that right on.
fn allow_foreground() {
    // SAFETY: No pointers; failure only means Windows keeps its default.
    unsafe {
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
}

pub fn open(id: &str) -> Result<(), LaunchError> {
    allow_foreground();
    // SAFETY: The strings live for the duration of the call.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(id),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    // ShellExecute returns a value greater than 32 on success.
    let code = result.0 as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(LaunchError {
            id: id.to_owned(),
            code,
        })
    }
}

/// Opens `files` with the executable `app`.
pub fn open_with(app: &str, files: &[String]) -> Result<(), LaunchError> {
    let arguments: Vec<String> = files.iter().map(|file| format!("\"{file}\"")).collect();
    allow_foreground();
    // SAFETY: The strings live for the duration of the call.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(app),
            &HSTRING::from(arguments.join(" ")),
            None,
            SW_SHOWNORMAL,
        )
    };
    let code = result.0 as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(LaunchError {
            id: app.to_owned(),
            code,
        })
    }
}

/// Presses the given keys together and releases them.
fn tap(keys: &[VIRTUAL_KEY]) {
    let key = |vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    let inputs: Vec<INPUT> = keys
        .iter()
        .map(|&vk| key(vk, KEYBD_EVENT_FLAGS(0)))
        .chain(keys.iter().rev().map(|&vk| key(vk, KEYEVENTF_KEYUP)))
        .collect();
    // SAFETY: The array lives for the duration of the call.
    unsafe {
        SendInput(&inputs, size_of::<INPUT>() as i32);
    }
}

/// Opens the Windows Start menu, exactly as the Windows key does.
pub fn open_start_menu() {
    tap(&[VK_LWIN]);
}

/// Opens the Windows 11 quick settings panel (network, volume, battery): Win+A.
pub fn open_quick_settings() {
    tap(&[VK_LWIN, VK_A]);
}

/// Opens the Windows 11 notification center and calendar: Win+N.
pub fn open_notifications() {
    tap(&[VK_LWIN, VK_N]);
}

pub fn reveal(path: &str) -> Result<(), LaunchError> {
    let arguments = HSTRING::from(format!("/select,\"{path}\""));
    allow_foreground();
    // SAFETY: The strings live for the duration of the call.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            w!("explorer.exe"),
            &arguments,
            None,
            SW_SHOWNORMAL,
        )
    };
    let code = result.0 as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(LaunchError {
            id: path.to_owned(),
            code,
        })
    }
}

/// Asks the windows to close, as the close button does; each app can prompt
/// the user to save changes.
pub fn close(windows: &[isize]) {
    for &hwnd in windows {
        // SAFETY: Only a message is sent; if the window no longer exists, the call has no effect.
        unsafe {
            let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

/// Brings an app's windows to the foreground. `windows` is ordered from front
/// to back; the first window receives focus.
///
/// If all windows are minimized, restores only the first one, as macOS does.
pub fn activate(windows: &[isize]) {
    let handles = || windows.iter().map(|&raw| HWND(raw as *mut _));
    // SAFETY: These operations target other windows; calls on a window that no
    // longer exists fail without further effect.
    unsafe {
        // Process from back to front to preserve their relative order.
        for hwnd in handles().rev().filter(|&hwnd| !IsIconic(hwnd).as_bool()) {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOP),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        let Some(front) = handles()
            .find(|&hwnd| !IsIconic(hwnd).as_bool())
            .or_else(|| handles().next())
        else {
            return;
        };
        if IsIconic(front).as_bool() {
            let _ = ShowWindow(front, SW_RESTORE);
            // Windows refuses ShowWindow on an app running as administrator
            // (Task Manager, for one). SwitchToThisWindow, what Alt+Tab uses,
            // restores it anyway and also brings it to the front.
            if IsIconic(front).as_bool() {
                SwitchToThisWindow(front, true);
            }
        }
        bring_to_front(front);
    }
}

/// Gives `hwnd` the focus. Windows only lets the process that received the
/// last input do this; when it refuses (the click was some time ago, or
/// another app grabbed the input since), an empty mouse input makes the dock
/// that process again, and the second attempt succeeds.
unsafe fn bring_to_front(hwnd: HWND) {
    // SAFETY: Calls on another window; they fail without effect if it is gone.
    unsafe {
        if SetForegroundWindow(hwnd).as_bool() && GetForegroundWindow() == hwnd {
            return;
        }
        let nudge = INPUT {
            r#type: INPUT_MOUSE,
            ..Default::default()
        };
        SendInput(&[nudge], size_of::<INPUT>() as i32);
        let _ = SetForegroundWindow(hwnd);
    }
}

/// Minimizes an app's windows, as clicking the active app's taskbar button
/// does. Asynchronous, so an app that is not responding cannot block the dock.
pub fn minimize(windows: &[isize]) {
    for &raw in windows {
        let hwnd = HWND(raw as *mut _);
        // SAFETY: Calls on another window; they fail without effect if it is gone.
        unsafe {
            if IsWindowVisible(hwnd).as_bool() && !IsIconic(hwnd).as_bool() {
                let _ = ShowWindowAsync(hwnd, SW_MINIMIZE);
            }
        }
    }
}
