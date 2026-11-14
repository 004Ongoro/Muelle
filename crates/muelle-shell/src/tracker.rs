//! Open windows and the apps they belong to.
//!
//! An app identifier is a shell name that can be opened and resolved to an icon
//! and display name: the lowercase executable path or, for packaged (Store) apps,
//! `shell:AppsFolder\<AppUserModelID>`.

use std::collections::HashMap;

use windows::{
    Win32::{
        Foundation::{CloseHandle, HWND, LPARAM, RECT},
        Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute},
        Storage::{
            EnhancedStorage::PKEY_AppUserModel_ID, Packaging::Appx::GetApplicationUserModelId,
        },
        System::Threading::{
            GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
        },
        UI::{
            Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow},
            WindowsAndMessaging::{
                EnumWindows, GW_OWNER, GWL_EXSTYLE, GetClassNameW, GetForegroundWindow, GetWindow,
                GetWindowLongW, GetWindowPlacement, GetWindowRect, GetWindowTextLengthW,
                GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
                WINDOWPLACEMENT, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
            },
        },
    },
    core::{BOOL, PWSTR},
};

const APPS_FOLDER: &str = "shell:AppsFolder\\";
const FRAME_HOST_CLASS: &str = "ApplicationFrameWindow";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackedWindow {
    pub hwnd: isize,
    pub app_id: String,
    pub minimized: bool,
}

#[must_use]
pub fn normalize_id(id: &str) -> String {
    if id.starts_with(APPS_FOLDER) {
        id.to_owned()
    } else {
        id.to_lowercase()
    }
}

/// Lists app windows and tracks which app each window belongs to.
///
/// Identifying a window's app requires opening its process or querying shell
/// properties, which makes each lookup expensive. The cache limits this to once
/// per window, when it appears.
#[derive(Debug, Default)]
pub struct WindowTracker {
    app_ids: HashMap<isize, String>,
}

impl WindowTracker {
    pub fn list(&mut self) -> Vec<TrackedWindow> {
        unsafe extern "system" fn collect(hwnd: HWND, list: LPARAM) -> BOOL {
            // SAFETY: `list` points to the Vec passed by `list` and remains
            // alive for the entire synchronous enumeration.
            unsafe { (*(list.0 as *mut Vec<HWND>)).push(hwnd) };
            true.into()
        }

        let mut all: Vec<HWND> = Vec::new();
        // SAFETY: The Vec pointer remains valid until EnumWindows returns.
        unsafe {
            let _ = EnumWindows(Some(collect), LPARAM(&raw mut all as isize));
        }

        let windows: Vec<TrackedWindow> = all
            .into_iter()
            .filter(|&hwnd| is_app_window(hwnd))
            .filter_map(|hwnd| {
                let key = hwnd.0 as isize;
                let app_id = match self.app_ids.get(&key) {
                    Some(id) => id.clone(),
                    // The newly created UWP app has not declared an identifier
                    // yet; do not cache it so the lookup can be retried.
                    None => {
                        let id = app_id(hwnd)?;
                        self.app_ids.insert(key, id.clone());
                        id
                    }
                };
                Some(TrackedWindow {
                    hwnd: key,
                    app_id,
                    minimized: unsafe { IsIconic(hwnd).as_bool() },
                })
            })
            .collect();

        // Remove closed windows because Windows reuses their handles.
        self.app_ids
            .retain(|hwnd, _| windows.iter().any(|window| window.hwnd == *hwnd));
        windows
    }
}

#[must_use]
pub fn title(hwnd: isize) -> String {
    let mut buffer = [0u16; 256];
    // SAFETY: The buffer remains alive during the call, which returns the number of characters written.
    let len = unsafe { GetWindowTextW(HWND(hwnd as *mut _), &mut buffer) };
    String::from_utf16_lossy(&buffer[..len.max(0) as usize])
}

/// Restored-state rectangle of a window, also valid while it is minimized.
/// Workspace coordinates match screen coordinates here: the work area starts at
/// the screen's top-left corner because the dock reserves only the bottom edge.
#[must_use]
pub fn normal_rect(hwnd: isize) -> Option<RECT> {
    let mut placement = WINDOWPLACEMENT {
        length: size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    // SAFETY: The structure has its size initialized and lives for the call.
    unsafe { GetWindowPlacement(HWND(hwnd as *mut _), &mut placement) }.ok()?;
    // Placement uses workspace coordinates, whose origin is the top-left
    // corner of the work area: below the menu bar when it is shown.
    let (dx, dy) = crate::system::primary_monitor().map_or((0, 0), |monitor| {
        (
            monitor.work_left - monitor.screen.left,
            monitor.work_top - monitor.screen.top,
        )
    });
    let local = placement.rcNormalPosition;
    let rect = RECT {
        left: local.left + dx,
        top: local.top + dy,
        right: local.right + dx,
        bottom: local.bottom + dy,
    };
    (rect.right > rect.left && rect.bottom > rect.top).then_some(rect)
}

#[must_use]
pub fn foreground_window() -> isize {
    unsafe { GetForegroundWindow().0 as isize }
}

/// Returns `true` if the foreground window covers the entire `screen`
/// (for example, video, presentation, or game). The desktop does not count.
#[must_use]
pub fn foreground_is_fullscreen(screen: RECT) -> bool {
    // SAFETY: Read-only queries on the foreground window.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() || DESKTOP_CLASSES.contains(&class_name(hwnd).as_str()) {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == GetCurrentProcessId() {
            return false;
        }
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect).is_ok() && covers(rect, screen)
    }
}

const DESKTOP_CLASSES: [&str; 3] = ["Progman", "WorkerW", "Shell_TrayWnd"];

fn covers(window: RECT, screen: RECT) -> bool {
    window.left <= screen.left
        && window.top <= screen.top
        && window.right >= screen.right
        && window.bottom >= screen.bottom
}

fn is_app_window(hwnd: HWND) -> bool {
    // SAFETY: Read-only queries; calls on a destroyed `hwnd` have no effect.
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || GetWindowTextLengthW(hwnd) == 0 {
            return false;
        }
        let style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        let has =
            |flag: windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE| style & flag.0 != 0;
        if has(WS_EX_TOOLWINDOW) {
            return false;
        }
        let owned = GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.is_invalid());
        if (owned || has(WS_EX_NOACTIVATE)) && !has(WS_EX_APPWINDOW) {
            return false;
        }
        // Windows hidden by the system, such as on other virtual desktops or suspended apps.
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&raw mut cloaked).cast(),
            size_of::<u32>() as u32,
        );
        if cloaked != 0 {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        pid != GetCurrentProcessId()
    }
}

fn app_id(hwnd: HWND) -> Option<String> {
    if class_name(hwnd) == FRAME_HOST_CLASS {
        return window_aumid(hwnd).map(|aumid| format!("{APPS_FOLDER}{aumid}"));
    }

    // SAFETY: The process handle is closed before leaving the block, and the
    // buffers have the declared lengths.
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;

        let mut buffer = [0u16; 1024];
        let mut len = buffer.len() as u32;
        let id = if GetApplicationUserModelId(process, &mut len, Some(PWSTR(buffer.as_mut_ptr())))
            .is_ok()
        {
            // `len` includes the null terminator.
            let aumid = String::from_utf16_lossy(&buffer[..len.saturating_sub(1) as usize]);
            Some(format!("{APPS_FOLDER}{aumid}"))
        } else {
            len = buffer.len() as u32;
            QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut len,
            )
            .ok()
            .map(|()| normalize_id(&String::from_utf16_lossy(&buffer[..len as usize])))
        };
        let _ = CloseHandle(process);
        id
    }
}

fn class_name(hwnd: HWND) -> String {
    let mut buffer = [0u16; 128];
    // SAFETY: The buffer remains alive during the call, which returns the number of characters written.
    let len = unsafe { GetClassNameW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..len.max(0) as usize])
}

fn window_aumid(hwnd: HWND) -> Option<String> {
    // SAFETY: COM calls use pointers that remain valid for the duration of each call.
    let value = unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd).ok()?;
        store.GetValue(&PKEY_AppUserModel_ID).ok()?
    };
    let aumid = value.to_string();
    (!aumid.is_empty()).then_some(aumid)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: RECT = RECT {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    };

    #[test]
    fn covers_should_accept_window_exactly_the_size_of_the_screen() {
        assert!(covers(SCREEN, SCREEN));
    }

    #[test]
    fn covers_should_reject_maximized_window_above_the_dock() {
        // A maximized window extends a few pixels beyond the sides and top, but
        // ends above the strip reserved by the dock.
        let maximized = RECT {
            left: -8,
            top: -8,
            right: 1928,
            bottom: 985,
        };
        assert!(!covers(maximized, SCREEN));
    }

    #[test]
    fn normalize_id_should_lowercase_executable_paths() {
        assert_eq!(
            normalize_id("C:\\Windows\\Explorer.EXE"),
            "c:\\windows\\explorer.exe"
        );
    }

    #[test]
    fn normalize_id_should_keep_packaged_app_ids_untouched() {
        let id = "shell:AppsFolder\\Microsoft.WindowsTerminal_8wekyb3d8bbwe!App";
        assert_eq!(normalize_id(id), id);
    }
}
