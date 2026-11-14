use std::mem::size_of;

use windows::{
    Win32::{
        Foundation::{POINT, RECT},
        Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint},
        System::Com::CoTaskMemFree,
        System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
        UI::{
            HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
            Shell::{FOLDERID_Downloads, KF_FLAG_DEFAULT, SHGetKnownFolderPath},
            WindowsAndMessaging::{SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW},
        },
    },
    core::{BOOL, w},
};

/// Returns `true` if Windows uses the light theme for the taskbar and shell.
///
/// Assumes the dark theme on read failure (the Windows 11 shell default).
#[must_use]
pub fn uses_light_theme() -> bool {
    let mut value = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: `value` and `size` describe a valid 4-byte buffer.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut value).cast()),
            Some(&mut size),
        )
    };
    status.is_ok() && value != 0
}

/// Monitor bounds, work area, and scale in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Monitor {
    /// Full-screen bounds.
    pub screen: RECT,
    pub work_left: i32,
    pub work_top: i32,
    pub work_right: i32,
    pub work_bottom: i32,
    /// 1.0 = 96 DPI (100%).
    pub scale: f32,
}

/// Primary monitor. `None` only if Windows cannot describe it.
#[must_use]
pub fn primary_monitor() -> Option<Monitor> {
    // SAFETY: Read-only queries using correctly sized structures.
    unsafe {
        let handle = MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(handle, &mut info).as_bool() {
            return None;
        }
        let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
        // Fall back to 96 DPI.
        let _ = GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        Some(Monitor {
            screen: info.rcMonitor,
            work_left: info.rcWork.left,
            work_top: info.rcWork.top,
            work_right: info.rcWork.right,
            work_bottom: info.rcWork.bottom,
            scale: dpi_x as f32 / 96.0,
        })
    }
}

/// Path to the user's Downloads folder, which may be on another drive.
#[must_use]
pub fn downloads_folder() -> Option<String> {
    // SAFETY: The returned string is copied and freed with the allocator that created it.
    unsafe {
        let raw = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None).ok()?;
        let path = raw.to_string().ok();
        CoTaskMemFree(Some(raw.as_ptr().cast()));
        path
    }
}

#[must_use]
pub fn ui_language_is_spanish() -> bool {
    const LANG_SPANISH: u16 = 0x0A;
    // SAFETY: Read-only query.
    let language = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    language & 0x3FF == LANG_SPANISH
}

/// `false` if the user turned off "Animation effects" in Windows settings.
#[must_use]
pub fn animations_enabled() -> bool {
    let mut enabled = BOOL::from(true);
    // SAFETY: `enabled` is a valid BOOL the call writes to.
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&raw mut enabled).cast()),
            Default::default(),
        );
    }
    enabled.as_bool()
}

/// Enables or disables the DWM transitions (minimize, maximize, close
/// animations) of one window. It works across processes.
pub fn set_window_transitions(hwnd: isize, enabled: bool) {
    let disabled = BOOL::from(!enabled);
    // SAFETY: The value outlives the call; a window that has closed makes it fail harmlessly.
    unsafe {
        let _ = windows::Win32::Graphics::Dwm::DwmSetWindowAttribute(
            windows::Win32::Foundation::HWND(hwnd as *mut _),
            windows::Win32::Graphics::Dwm::DWMWA_TRANSITIONS_FORCEDISABLED,
            (&raw const disabled).cast(),
            size_of::<BOOL>() as u32,
        );
    }
}
