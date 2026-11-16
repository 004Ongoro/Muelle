//! Logs which window is in front and the stacking order of the top app
//! windows (with their program) whenever either changes, plus minimize
//! start/end. Only listens. Usage: `spike_focuslog [seconds]` (default 90).

use std::time::{Duration, Instant};

use windows::{
    Win32::{
        Foundation::{CloseHandle, HWND, LPARAM},
        System::Threading::{OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW},
        UI::{
            Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
            WindowsAndMessaging::{
                DispatchMessageW, EnumWindows, GWL_EXSTYLE, GetForegroundWindow, GetWindowLongW,
                GetWindowThreadProcessId, IsIconic, IsWindowVisible, MSG, PM_REMOVE, PeekMessageW,
                WINEVENT_OUTOFCONTEXT, WS_EX_TOOLWINDOW,
            },
        },
    },
    core::{BOOL, PWSTR},
};

fn exe(hwnd: HWND) -> String {
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return format!("pid{pid}") };
        let mut buffer = [0u16; 512];
        let mut len = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(process);
        if !ok { return format!("pid{pid}") }
        let path = String::from_utf16_lossy(&buffer[..len as usize]);
        path.rsplit(char::from(92)).next().unwrap_or_default().to_owned()
    }
}

fn label(hwnd: HWND) -> String {
    format!("{}#{:x}{}", exe(hwnd), hwnd.0 as usize & 0xFFFF, if unsafe { IsIconic(hwnd).as_bool() } { "(min)" } else { "" })
}

unsafe extern "system" fn collect(hwnd: HWND, list: LPARAM) -> BOOL {
    unsafe {
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if IsWindowVisible(hwnd).as_bool() && ex & WS_EX_TOOLWINDOW.0 == 0 {
            let v = &mut *(list.0 as *mut Vec<HWND>);
            v.push(hwnd);
        }
    }
    true.into()
}

fn order() -> String {
    let mut all: Vec<HWND> = Vec::new();
    unsafe { let _ = EnumWindows(Some(collect), LPARAM(&raw mut all as isize)); }
    all.into_iter()
        .map(label)
        .filter(|l| !l.starts_with("pid") && !l.contains("TextInputHost") && !l.contains("ApplicationFrameHost"))
        .take(5)
        .collect::<Vec<_>>()
        .join(" > ")
}

static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

fn stamp() -> u128 { START.get().map_or(0, |s| s.elapsed().as_millis()) }

unsafe extern "system" fn on_event(_: HWINEVENTHOOK, event: u32, hwnd: HWND, object: i32, _: i32, _: u32, _: u32) {
    if object != 0 { return; }
    let name = match event { 3 => "FOREGROUND", 0x16 => "MINIMIZESTART", 0x17 => "MINIMIZEEND", _ => return };
    println!("{:>7} {name:<14} {}   | z: {}", stamp(), label(hwnd), order());
}

fn main() {
    let seconds: u64 = std::env::args().nth(1).and_then(|v| v.parse().ok()).unwrap_or(90);
    let _ = START.set(Instant::now());
    unsafe {
        let hook = SetWinEventHook(3, 0x17, None, Some(on_event), 0, 0, WINEVENT_OUTOFCONTEXT);
        let end = Instant::now() + Duration::from_secs(seconds);
        let mut last = String::new();
        let mut msg = MSG::default();
        while Instant::now() < end {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() { DispatchMessageW(&msg); }
            let now = format!("front {} | z: {}", label(GetForegroundWindow()), order());
            if now != last { println!("{:>7} poll           {now}", stamp()); last = now; }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = UnhookWinEvent(hook);
    }
}
