use std::{env, fs, path::PathBuf};

use windows::{
    Win32::{
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ},
        UI::Shell::{IShellLinkW, ShellLink},
    },
    core::{HSTRING, Interface},
};

use crate::tracker::normalize_id;

/// Limitations: results are alphabetized by shortcut (the actual taskbar order
/// uses an undocumented binary format), and Store apps are omitted because
/// Windows stores them elsewhere.
///
/// Requires COM to be initialized on the calling thread.
#[must_use]
pub fn taskbar_pins() -> Vec<String> {
    let Ok(appdata) = env::var("APPDATA") else {
        return Vec::new();
    };
    let folder = PathBuf::from(appdata)
        .join("Microsoft\\Internet Explorer\\Quick Launch\\User Pinned\\TaskBar");
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };

    let mut links: Vec<PathBuf> = entries
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"))
        })
        .collect();
    links.sort();

    let mut pins: Vec<String> = Vec::new();
    for target in links.iter().filter_map(|link| link_target(link)) {
        let id = normalize_id(&target);
        if id.ends_with(".exe") && PathBuf::from(&id).exists() && !pins.contains(&id) {
            pins.push(id);
        }
    }
    pins
}

pub fn link_target(link: &std::path::Path) -> Option<String> {
    // SAFETY: COM calls use buffers and strings that remain alive for the duration of each call.
    unsafe {
        let shell_link: IShellLinkW =
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        shell_link
            .cast::<IPersistFile>()
            .ok()?
            .Load(&HSTRING::from(link.as_os_str()), STGM_READ)
            .ok()?;
        let mut buffer = [0u16; 1024];
        shell_link
            .GetPath(&mut buffer, std::ptr::null_mut(), 0)
            .ok()?;
        let len = buffer.iter().position(|&unit| unit == 0)?;
        (len > 0).then(|| String::from_utf16_lossy(&buffer[..len]))
    }
}
