//! Minimal macOS-style menu bar across the top of the screen.
//!
//! Hiding the Windows taskbar also hides its clock and status area; this bar
//! brings them back: the name of the app in front, the battery, a control
//! center button (opens the Windows quick settings: network, volume,
//! brightness) and the date and time (opens the notification center).
//!
//! Cost while idle: one timer that fires once a minute, on the minute, to
//! redraw the clock. Everything else is event-driven.

use anyhow::Result;
use muelle_shell::{
    appbar::{AppBar, Edge},
    launcher,
    system::{self, Battery, Monitor},
};
use windows::{
    UI::{
        Color,
        Composition::{Desktop::DesktopWindowTarget, SpriteVisual},
    },
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, SYSTEMTIME, WPARAM},
        Graphics::Dwm::{DWMWA_USE_HOSTBACKDROPBRUSH, DwmSetWindowAttribute},
        System::{
            LibraryLoader::GetModuleHandleW, SystemInformation::GetLocalTime,
            WinRT::Composition::ICompositorDesktopInterop,
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, HWND_TOPMOST, IDC_ARROW, LoadCursorW,
            RegisterClassW, SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetTimer, SetWindowPos,
            WM_APP, WM_LBUTTONUP, WM_TIMER, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP,
            WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
        },
    },
    core::{BOOL, Interface, w},
};
use windows_numerics::{Vector2, Vector3};

use crate::{
    app,
    gfx::{Gfx, Ink, LabelBrush},
};

// Logical pixels.
const HEIGHT: f32 = 28.0;
const SIDE_MARGIN: f32 = 14.0;
const ITEM_GAP: f32 = 16.0;
/// Extra clickable room around an item.
const HIT_SLOP: f32 = 7.0;

const CLOCK_TIMER: usize = 1;
const WM_POWERBROADCAST: u32 = 0x0218;
/// AppBar notifications need a message id; the bar ignores them.
const APPBAR_CALLBACK: u32 = WM_APP + 20;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    QuickSettings,
    Notifications,
}

/// What the bar was last drawn for.
#[derive(Clone, PartialEq)]
struct Look {
    screen: RECT,
    scale: f32,
    light: bool,
    app_name: String,
    clock: String,
    battery: Option<Battery>,
}

pub struct MenuBar {
    hwnd: HWND,
    target: DesktopWindowTarget,
    appbar: Option<AppBar>,
    app_name: String,
    look: Option<Look>,
    /// Clickable spans, in window pixels.
    zones: Vec<(f32, f32, Action)>,
    visible: bool,
}

unsafe extern "system" fn bar_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONUP => {
            let x = (l.0 & 0xFFFF) as i16;
            app::menu_bar_clicked(f32::from(x));
        }
        // The minute changed, or the charger was plugged or unplugged.
        WM_TIMER | WM_POWERBROADCAST => app::menu_bar_tick(),
        // SAFETY: Standard forwarding of unhandled messages.
        _ => return unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
    LRESULT(0)
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color {
        A: a,
        R: r,
        G: g,
        B: b,
    }
}

impl MenuBar {
    pub fn create(gfx: &Gfx) -> Result<Self> {
        let class = w!("MuelleMenuBar");
        // SAFETY: Standard class registration and window creation on the UI
        // thread; the window is destroyed in `Drop`.
        let hwnd = unsafe {
            let instance = GetModuleHandleW(None)?;
            // Registering an existing class fails harmlessly.
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(bar_proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                lpszClassName: class,
                ..Default::default()
            });
            let hwnd = CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class,
                w!("Muelle"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            // Without this, the system backdrop brush appears black.
            let enable = BOOL::from(true);
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_HOSTBACKDROPBRUSH,
                (&raw const enable).cast(),
                size_of::<BOOL>() as u32,
            );
            hwnd
        };
        let target = unsafe {
            // SAFETY: `hwnd` is the live window created above.
            gfx.compositor
                .cast::<ICompositorDesktopInterop>()?
                .CreateDesktopWindowTarget(hwnd, true)?
        };
        let mut bar = Self {
            hwnd,
            target,
            appbar: AppBar::register(hwnd, APPBAR_CALLBACK),
            app_name: String::new(),
            look: None,
            zones: Vec::new(),
            visible: true,
        };
        bar.refresh(gfx)?;
        bar.schedule_next_minute();
        Ok(bar)
    }

    pub fn set_app_name(&mut self, gfx: &Gfx, name: &str) -> Result<()> {
        if self.app_name == name {
            return Ok(());
        }
        name.clone_into(&mut self.app_name);
        self.refresh(gfx)
    }

    /// Hidden while an app is full screen.
    pub fn set_visible(&mut self, visible: bool) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        let show = if visible {
            SWP_SHOWWINDOW
        } else {
            SWP_HIDEWINDOW
        };
        // SAFETY: Our own live window; only its visibility changes.
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE
                    | show
                    | windows::Win32::UI::WindowsAndMessaging::SWP_NOMOVE
                    | windows::Win32::UI::WindowsAndMessaging::SWP_NOSIZE,
            );
        }
    }

    /// The minute changed or the power state did: redraw what differs.
    pub fn tick(&mut self, gfx: &Gfx) -> Result<()> {
        self.schedule_next_minute();
        self.refresh(gfx)
    }

    /// One wake-up per minute, aligned to the minute, instead of polling.
    fn schedule_next_minute(&self) {
        // SAFETY: Read-only query, then a timer on our own window (setting an
        // existing timer id replaces it).
        unsafe {
            let now: SYSTEMTIME = GetLocalTime();
            let elapsed = u32::from(now.wSecond) * 1000 + u32::from(now.wMilliseconds);
            SetTimer(
                Some(self.hwnd),
                CLOCK_TIMER,
                60_000 - elapsed.min(59_900) + 50,
                None,
            );
        }
    }

    pub fn click(&self, x: f32) {
        let hit = self
            .zones
            .iter()
            .find(|(left, right, _)| x >= *left && x < *right);
        match hit.map(|(_, _, action)| *action) {
            Some(Action::QuickSettings) => launcher::open_quick_settings(),
            Some(Action::Notifications) => launcher::open_notifications(),
            None => {}
        }
    }

    /// Redraws the bar if anything it shows has changed.
    pub fn refresh(&mut self, gfx: &Gfx) -> Result<()> {
        let Some(monitor) = system::primary_monitor() else {
            return Ok(());
        };
        let look = Look {
            screen: monitor.screen,
            scale: monitor.scale,
            light: system::uses_light_theme(),
            app_name: self.app_name.clone(),
            clock: system::clock_text(),
            battery: system::battery(),
        };
        if self.look.as_ref() == Some(&look) {
            return Ok(());
        }
        self.build(gfx, &monitor, &look)?;
        self.look = Some(look);
        Ok(())
    }

    fn build(&mut self, gfx: &Gfx, monitor: &Monitor, look: &Look) -> Result<()> {
        let c = &gfx.compositor;
        let scale = look.scale;
        let px = |logical: f32| (logical * scale).round();
        let screen = monitor.screen;
        let (width, height) = ((screen.right - screen.left) as f32, px(HEIGHT));
        let ink = if look.light { Ink::Dark } else { Ink::Light };

        let root = c.CreateContainerVisual()?;
        root.SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })?;
        let layers = root.Children()?;
        let tint = if look.light {
            rgba(255, 255, 255, 110)
        } else {
            rgba(24, 24, 26, 120)
        };
        let fills = [
            c.CreateHostBackdropBrush()?
                .cast::<windows::UI::Composition::CompositionBrush>()?,
            c.CreateColorBrushWithColor(tint)?.cast()?,
        ];
        for brush in &fills {
            let fill = c.CreateSpriteVisual()?;
            fill.SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })?;
            fill.SetBrush(brush)?;
            layers.InsertAtTop(&fill)?;
        }

        let place = |layers: &windows::UI::Composition::VisualCollection,
                     item: &LabelBrush,
                     left: f32|
         -> Result<SpriteVisual> {
            let visual = c.CreateSpriteVisual()?;
            visual.SetSize(Vector2 {
                X: item.width,
                Y: item.height,
            })?;
            visual.SetOffset(Vector3 {
                X: left.round(),
                Y: ((height - item.height) / 2.0).round(),
                Z: 0.0,
            })?;
            visual.SetBrush(&item.brush)?;
            layers.InsertAtTop(&visual)?;
            Ok(visual)
        };

        self.zones.clear();
        if !look.app_name.is_empty() {
            let name = gfx.text_brush(&look.app_name, scale, true, ink)?;
            place(&layers, &name, px(SIDE_MARGIN))?;
        }

        // Right side, from the edge inward: clock, control center, battery.
        let slop = px(HIT_SLOP);
        let mut right = width - px(SIDE_MARGIN);
        let mut items: Vec<(LabelBrush, Action)> = vec![
            (
                gfx.text_brush(&look.clock, scale, false, ink)?,
                Action::Notifications,
            ),
            (gfx.control_center_brush(scale, ink)?, Action::QuickSettings),
        ];
        if let Some(battery) = look.battery {
            items.push((
                gfx.battery_brush(battery.percent, battery.charging, scale, ink)?,
                Action::QuickSettings,
            ));
        }
        for (item, action) in &items {
            let left = right - item.width;
            place(&layers, item, left)?;
            self.zones.push((left - slop, right + slop, *action));
            right = left - px(ITEM_GAP);
        }

        self.target.SetRoot(&root)?;

        if let Some(appbar) = &mut self.appbar {
            appbar.reserve(Edge::Top, screen, height as i32);
        }
        let show = if self.visible {
            SWP_SHOWWINDOW
        } else {
            SWP_HIDEWINDOW
        };
        // SAFETY: Our own live window.
        unsafe {
            SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                screen.left,
                screen.top,
                width as i32,
                height as i32,
                SWP_NOACTIVATE | show,
            )?;
        }
        Ok(())
    }
}

impl Drop for MenuBar {
    fn drop(&mut self) {
        // Give the reserved strip back before the window goes away.
        self.appbar = None;
        // SAFETY: Our own window, destroyed once.
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
