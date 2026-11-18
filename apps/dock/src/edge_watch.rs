//! One low-level mouse hook for the top bar and the dock. The hook only compares
//! cached rectangles and posts changes. Dwell and window work run on the UI thread.

use std::{
    sync::{Mutex, MutexGuard, PoisonError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use anyhow::Result;
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    System::{
        LibraryLoader::GetModuleHandleW,
        Threading::{
            GetCurrentThread, GetCurrentThreadId, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
        },
    },
    UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, PeekMessageW, PostMessageW,
        PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_MOUSE_LL, WM_APP,
        WM_MOUSEMOVE, WM_QUIT,
    },
};

pub const CHANGED: u32 = WM_APP + 26;
const DWELL: Duration = Duration::from_millis(250);
const EDGE_DEPTH: i32 = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    MenuBar,
    Dock(RECT),
}

pub enum Reveal {
    Idle,
    Wait(Duration),
    Ready(Target),
}

#[derive(Default)]
struct Edges {
    window: isize,
    top: Option<RECT>,
    bottoms: Vec<RECT>,
    target: Option<Target>,
    entered: Option<Instant>,
}

static EDGES: Mutex<Edges> = Mutex::new(Edges {
    window: 0,
    top: None,
    bottoms: Vec::new(),
    target: None,
    entered: None,
});

impl Edges {
    fn at(&self, point: POINT) -> Option<Target> {
        let within_x = |rect: RECT| point.x >= rect.left && point.x < rect.right;
        if self.top.is_some_and(|screen| {
            within_x(screen) && (screen.top..screen.top + EDGE_DEPTH).contains(&point.y)
        }) {
            return Some(Target::MenuBar);
        }
        self.bottoms
            .iter()
            .copied()
            .find(|&screen| {
                within_x(screen) && (screen.bottom - EDGE_DEPTH..screen.bottom).contains(&point.y)
            })
            .map(Target::Dock)
    }

    fn observe(&mut self, point: POINT, now: Instant) -> bool {
        let target = self.at(point);
        if target == self.target {
            return false;
        }
        self.target = target;
        self.entered = target.map(|_| now);
        true
    }

    fn set_top(&mut self, top: Option<RECT>) {
        if top != self.top {
            self.top = top;
            if self.target == Some(Target::MenuBar) {
                self.target = None;
                self.entered = None;
            }
        }
    }

    fn set_bottoms(&mut self, bottoms: Vec<RECT>) {
        if let Some(Target::Dock(screen)) = self.target
            && !bottoms.contains(&screen)
        {
            self.target = None;
            self.entered = None;
        }
        self.bottoms = bottoms;
    }

    fn dwell(&mut self, point: POINT, now: Instant) -> Reveal {
        self.observe(point, now);
        let (Some(entered), Some(target)) = (self.entered, self.target) else {
            return Reveal::Idle;
        };
        let elapsed = now.saturating_duration_since(entered);
        if elapsed < DWELL {
            return Reveal::Wait(DWELL - elapsed);
        }
        self.entered = None;
        Reveal::Ready(target)
    }
}

fn lock() -> MutexGuard<'static, Edges> {
    EDGES.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn set_menu_bar(top: Option<RECT>) {
    lock().set_top(top);
}

pub fn set_dock_monitors(bottoms: Vec<RECT>) {
    lock().set_bottoms(bottoms);
}

pub fn poll(point: POINT) -> Reveal {
    lock().dwell(point, Instant::now())
}

/// The hook can post before Windows applies the pointer move. Only the timer
/// samples the live cursor; this notification uses the hook's position.
pub fn pending() -> (Option<Target>, Option<Duration>) {
    let edges = lock();
    (
        edges.target,
        edges
            .entered
            .map(|entered| DWELL.saturating_sub(entered.elapsed())),
    )
}

pub fn retry_on_move(target: Target) {
    let mut edges = lock();
    if edges.target == Some(target) {
        edges.target = None;
        edges.entered = None;
    }
}

pub struct EdgeWatch {
    thread: u32,
    join: Option<JoinHandle<()>>,
}

impl EdgeWatch {
    pub fn start(window: HWND) -> Result<Self> {
        let (sender, receiver) = std::sync::mpsc::channel();
        let join = thread::Builder::new()
            .name("muelle-edge".into())
            .spawn(move || {
                // SAFETY: The hook belongs to the message thread created here.
                unsafe {
                    let mut msg = MSG::default();
                    let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
                    let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
                    let hook = GetModuleHandleW(None).and_then(|module| {
                        SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), Some(module.into()), 0)
                    });
                    let _ = sender.send(
                        hook.as_ref()
                            .map(|_| GetCurrentThreadId())
                            .map_err(Clone::clone),
                    );
                    if let Ok(hook) = hook {
                        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {}
                        let _ = UnhookWindowsHookEx(hook);
                    }
                }
            })?;
        match receiver.recv() {
            Ok(Ok(thread)) => {
                lock().window = window.0 as isize;
                Ok(Self {
                    thread,
                    join: Some(join),
                })
            }
            result => {
                let _ = join.join();
                Err(anyhow::anyhow!("no se pudo observar el borde: {result:?}"))
            }
        }
    }
}

impl Drop for EdgeWatch {
    fn drop(&mut self) {
        {
            let mut edges = lock();
            edges.window = 0;
            edges.target = None;
            edges.entered = None;
        }
        // SAFETY: Posting to the queue created before the thread reported ready.
        unsafe {
            let _ = PostThreadMessageW(self.thread, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

unsafe extern "system" fn mouse_hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code >= 0 && w.0 as u32 == WM_MOUSEMOVE {
        // SAFETY: A low-level mouse notification has a live MSLLHOOKSTRUCT.
        let point = unsafe { (*(l.0 as *const MSLLHOOKSTRUCT)).pt };
        let window = EDGES.try_lock().ok().and_then(|mut edges| {
            (edges.window != 0 && edges.observe(point, Instant::now())).then_some(edges.window)
        });
        if let Some(window) = window {
            // SAFETY: The UI owns the receiving window; posting never waits.
            unsafe {
                let _ = PostMessageW(Some(HWND(window as *mut _)), CHANGED, WPARAM(0), LPARAM(0));
            }
        }
    }
    // SAFETY: Forward the original event without consuming it.
    unsafe { CallNextHookEx(None, code, w, l) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unchanged_bar_layout_does_not_cancel_hover_dwell() {
        let mut edges = Edges::default();
        let screen = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let point = POINT { x: 700, y: 1 };
        let start = Instant::now();
        edges.set_top(Some(screen));
        assert!(edges.observe(point, start));
        edges.set_top(Some(screen));
        assert!(matches!(
            edges.dwell(point, start + DWELL),
            Reveal::Ready(Target::MenuBar)
        ));
        assert!(matches!(edges.dwell(point, start + DWELL), Reveal::Idle));
    }

    #[test]
    fn dwell_follows_the_bottom_of_the_display_with_the_pointer() {
        let primary = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let secondary = RECT {
            left: -1280,
            top: -200,
            right: 0,
            bottom: 824,
        };
        let mut edges = Edges::default();
        edges.set_bottoms(vec![primary, secondary]);
        let start = Instant::now();
        let left = POINT { x: -700, y: 823 };
        let right = POINT { x: 700, y: 1079 };
        assert!(edges.observe(right, start));
        assert!(edges.observe(left, start + Duration::from_millis(100)));
        assert!(matches!(edges.dwell(left, start + DWELL), Reveal::Wait(_)));
        assert!(
            matches!(edges.dwell(left, start + DWELL * 2), Reveal::Ready(Target::Dock(screen)) if screen == secondary)
        );
        edges.set_bottoms(vec![primary]);
        assert!(matches!(edges.dwell(left, start + DWELL * 3), Reveal::Idle));
    }

    #[test]
    fn pending_reveal_cancels_when_the_pointer_crosses_the_edge() {
        let mut edges = Edges::default();
        edges.set_bottoms(vec![RECT {
            left: 0,
            top: -900,
            right: 1600,
            bottom: 0,
        }]);
        let start = Instant::now();
        assert!(edges.observe(POINT { x: 800, y: -1 }, start));
        assert!(matches!(
            edges.dwell(POINT { x: 800, y: 0 }, start + DWELL),
            Reveal::Idle
        ));
    }
}
