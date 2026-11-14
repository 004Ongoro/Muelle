//! Context menu implemented with the native Windows menu API. It supports
//! screen readers and keyboard navigation, and exists only while open.

use std::sync::OnceLock;

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, POINT, WPARAM},
        System::LibraryLoader::{GetProcAddress, LoadLibraryW},
        UI::WindowsAndMessaging::{
            AppendMenuW, CreatePopupMenu, DestroyMenu, GetForegroundWindow, MF_CHECKED, MF_GRAYED,
            MF_SEPARATOR, MF_STRING, PostMessageW, SetForegroundWindow, TPM_BOTTOMALIGN,
            TPM_CENTERALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenuEx,
            WM_NULL,
        },
    },
    core::{HSTRING, PCSTR, PCWSTR, w},
};

pub enum MenuItem {
    Action {
        command: u32,
        label: String,
        checked: bool,
        enabled: bool,
    },
    Separator,
}

impl MenuItem {
    pub fn action(command: u32, label: impl Into<String>) -> Self {
        Self::Action {
            command,
            label: label.into(),
            checked: false,
            enabled: true,
        }
    }
}

/// Shows the menu with its bottom edge centered at `anchor` and returns the
/// selected command (`None` if dismissed).
///
/// Runs a modal loop that continues dispatching messages to dock windows.
/// The caller must not hold the app state while this function runs.
pub fn show(owner: HWND, items: &[MenuItem], anchor: POINT, dark: bool) -> Option<u32> {
    apply_theme(dark);
    // SAFETY: The menu is destroyed before returning, and strings live through each call.
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        for item in items {
            let _ = match item {
                MenuItem::Separator => AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()),
                MenuItem::Action {
                    command,
                    label,
                    checked,
                    enabled,
                } => {
                    let mut flags = MF_STRING;
                    if *checked {
                        flags |= MF_CHECKED;
                    }
                    if !*enabled {
                        flags |= MF_GRAYED;
                    }
                    AppendMenuW(
                        menu,
                        flags,
                        *command as usize,
                        &HSTRING::from(label.as_str()),
                    )
                }
            };
        }

        // Without making the window foreground, TrackPopupMenu does not dismiss
        // the menu when the user clicks outside it.
        let previous = GetForegroundWindow();
        let _ = SetForegroundWindow(owner);
        let chosen = TrackPopupMenuEx(
            menu,
            (TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_CENTERALIGN).0,
            anchor.x,
            anchor.y,
            owner,
            None,
        );
        let _ = PostMessageW(Some(owner), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);

        let chosen = u32::try_from(chosen.0).ok().filter(|&command| command != 0);
        if chosen.is_none() {
            // Dismissing without a selection restores the previous focus, as in macOS.
            let _ = SetForegroundWindow(previous);
        }
        chosen
    }
}

/// Windows has no public API for dark Win32 menus, so this uses `uxtheme.dll`
/// functions by ordinal, as Windows Terminal and Notepad++ do. If unavailable
/// on another Windows version, the menu remains light.
fn apply_theme(dark: bool) {
    type SetPreferredAppMode = unsafe extern "system" fn(i32) -> i32;
    type FlushMenuThemes = unsafe extern "system" fn();
    const FORCE_DARK: i32 = 2;
    const FORCE_LIGHT: i32 = 3;

    static FUNCTIONS: OnceLock<Option<(SetPreferredAppMode, FlushMenuThemes)>> = OnceLock::new();
    let functions = FUNCTIONS.get_or_init(|| {
        // SAFETY: Functions exported at these ordinals have these signatures
        // since Windows 10 1903, and the library is never unloaded.
        unsafe {
            let module = LoadLibraryW(w!("uxtheme.dll")).ok()?;
            let set = GetProcAddress(module, PCSTR(135 as *const u8))?;
            let flush = GetProcAddress(module, PCSTR(136 as *const u8))?;
            Some((
                std::mem::transmute::<unsafe extern "system" fn() -> isize, SetPreferredAppMode>(
                    set,
                ),
                std::mem::transmute::<unsafe extern "system" fn() -> isize, FlushMenuThemes>(flush),
            ))
        }
    });
    if let Some((set, flush)) = functions {
        // SAFETY: The function is resolved from the loaded module using its documented ordinal.
        unsafe {
            set(if dark { FORCE_DARK } else { FORCE_LIGHT });
            flush();
        }
    }
}
