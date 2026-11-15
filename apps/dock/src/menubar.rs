//! Minimal macOS-style menu bar across the top of the screen.
//!
//! Hiding the Windows taskbar also hides its clock and status area; this bar
//! brings them back: the name of the app in front, the battery, a control
//! center button (opens the Windows quick settings: network, volume,
//! brightness) and the date and time (opens the notification center).
//!
//! Cost while idle: one timer that fires once a minute, on the minute, to
//! redraw the clock. Everything else is event-driven.
//!
//! With auto-hide on, the bar reserves no space and stays off screen; a thin
//! strip along the top edge reveals it while the pointer is there.
//!
//! Status items, right to left as on a Mac: clock, control center, search,
//! network, volume, battery, then Task View and Widgets. Search, Task View and
//! Widgets appear only if the user has them on the Windows taskbar. Network
//! and volume changes arrive as `STATUS_CHANGED`; nothing polls.

use anyhow::Result;
use muelle_shell::{
    appbar::{AppBar, Edge},
    launcher,
    status::{self, AudioWatch, Network, NetworkWatch, TaskbarButtons, Volume},
    system::{self, Battery, Monitor},
};
use windows::{
    UI::{
        Color,
        Composition::{ContainerVisual, Desktop::DesktopWindowTarget, SpriteVisual},
    },
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, SYSTEMTIME, WPARAM},
        Graphics::Dwm::{DWMWA_USE_HOSTBACKDROPBRUSH, DwmSetWindowAttribute},
        System::{
            LibraryLoader::GetModuleHandleW, SystemInformation::GetLocalTime,
            WinRT::Composition::ICompositorDesktopInterop,
        },
        UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent},
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, HWND_TOPMOST, IDC_ARROW, LoadCursorW,
            RegisterClassW, SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetTimer, SetWindowPos,
            WM_APP, WM_LBUTTONUP, WM_MOUSEMOVE, WM_SETTINGCHANGE, WM_TIMER, WNDCLASSW,
            WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
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

/// Height of the strip that reveals a hidden bar, in physical pixels.
const REVEAL_STRIP: i32 = 2;

const CLOCK_TIMER: usize = 1;
const WM_POWERBROADCAST: u32 = 0x0218;
const WM_MOUSELEAVE: u32 = 0x02A3;
/// AppBar notifications need a message id; the bar ignores them.
const APPBAR_CALLBACK: u32 = WM_APP + 20;
/// Posted by the network and audio watches.
const STATUS_CHANGED: u32 = WM_APP + 21;

/// Segoe Fluent Icons code points.
mod glyph {
    pub const WIFI: [char; 4] = ['\u{E872}', '\u{E873}', '\u{E874}', '\u{E701}'];
    pub const WIFI_FULL: char = '\u{E701}';
    pub const ETHERNET: char = '\u{E839}';
    pub const OFFLINE: char = '\u{F384}';
    pub const VOLUME: [char; 4] = ['\u{E992}', '\u{E993}', '\u{E994}', '\u{E995}'];
    pub const MUTE: char = '\u{E74F}';
    pub const SEARCH: char = '\u{E721}';
    pub const TASK_VIEW: char = '\u{E7C4}';
    pub const WIDGETS: char = '\u{F246}';
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    QuickSettings,
    Notifications,
    Sound,
    Search,
    TaskView,
    Widgets,
}

/// Network, volume and taskbar buttons, read only when Windows reports a change.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Status {
    network: Network,
    volume: Option<Volume>,
    buttons: TaskbarButtons,
}

/// What the bar was last drawn for.
#[derive(Clone, PartialEq)]
struct Look {
    screen: RECT,
    scale: f32,
    light: bool,
    /// See `system::transparency_enabled`.
    glass: bool,
    app_name: String,
    clock: String,
    battery: Option<Battery>,
    status: Status,
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
    auto_hide: bool,
    /// With auto-hide on: the pointer is at the top edge, so the bar shows.
    revealed: bool,
    /// Current visual tree and where the full bar goes on screen.
    root: Option<ContainerVisual>,
    bounds: RECT,
    status: Status,
    /// Held for their callbacks; dropping them unsubscribes.
    _network: Option<NetworkWatch>,
    audio: Option<AudioWatch>,
}

unsafe extern "system" fn bar_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONUP => {
            let x = (l.0 & 0xFFFF) as i16;
            app::menu_bar_clicked(f32::from(x));
        }
        WM_MOUSEMOVE => app::menu_bar_hover(true),
        WM_MOUSELEAVE => app::menu_bar_hover(false),
        // The minute changed, the charger was plugged or unplugged, the
        // network or volume changed, or a Windows setting (taskbar buttons).
        WM_TIMER | WM_POWERBROADCAST | STATUS_CHANGED | WM_SETTINGCHANGE => app::menu_bar_tick(),
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
            auto_hide: false,
            revealed: false,
            root: None,
            bounds: RECT::default(),
            status: Status {
                network: Network::Offline,
                volume: None,
                buttons: TaskbarButtons::default(),
            },
            _network: NetworkWatch::new(hwnd.0 as isize, STATUS_CHANGED),
            audio: AudioWatch::new(hwnd.0 as isize, STATUS_CHANGED),
        };
        bar.update_status();
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
    pub fn set_visible(&mut self, visible: bool) -> Result<()> {
        if self.visible == visible {
            return Ok(());
        }
        self.visible = visible;
        self.place()
    }

    pub fn set_auto_hide(&mut self, auto_hide: bool) -> Result<()> {
        if self.auto_hide == auto_hide {
            return Ok(());
        }
        self.auto_hide = auto_hide;
        self.revealed = false;
        self.place()
    }

    /// The pointer reached the top edge or left the bar.
    pub fn hover(&mut self, inside: bool) -> Result<()> {
        if !self.auto_hide || self.revealed == inside {
            return Ok(());
        }
        self.revealed = inside;
        self.place()?;
        if inside {
            let mut track = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: self.hwnd,
                dwHoverTime: 0,
            };
            // SAFETY: `track` is a valid structure with its size initialized.
            unsafe { TrackMouseEvent(&mut track)? };
        }
        Ok(())
    }

    /// Positions the window and reserves (or frees) its strip of the screen.
    fn place(&mut self) -> Result<()> {
        let bounds = self.bounds;
        let height = bounds.bottom - bounds.top;
        if height <= 0 {
            return Ok(());
        }
        let hidden = self.auto_hide && !self.revealed;
        if let Some(appbar) = &mut self.appbar {
            // An auto-hiding bar leaves the whole screen to the windows.
            let strip = if self.auto_hide { 0 } else { height };
            appbar.reserve(Edge::Top, bounds, strip);
        }
        if let Some(root) = &self.root {
            // Hidden: the content sits above the screen edge, and only the
            // reveal strip of the window remains to catch the pointer.
            let lift = if hidden { -(height as f32) } else { 0.0 };
            root.SetOffset(Vector3 {
                X: 0.0,
                Y: lift,
                Z: 0.0,
            })?;
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
                bounds.left,
                bounds.top,
                bounds.right - bounds.left,
                if hidden { REVEAL_STRIP } else { height },
                SWP_NOACTIVATE | show,
            )?;
        }
        Ok(())
    }

    /// The minute changed, or something the bar shows did: redraw what differs.
    pub fn tick(&mut self, gfx: &Gfx) -> Result<()> {
        self.update_status();
        self.schedule_next_minute();
        self.refresh(gfx)
    }

    fn update_status(&mut self) {
        self.status = Status {
            network: status::network(),
            volume: self.audio.as_mut().and_then(AudioWatch::volume),
            buttons: status::taskbar_buttons(),
        };
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
            Some(Action::Sound) => launcher::open_sound(),
            Some(Action::Search) => launcher::open_search(),
            Some(Action::TaskView) => launcher::open_task_view(),
            Some(Action::Widgets) => launcher::open_widgets(),
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
            glass: system::transparency_enabled(),
            app_name: self.app_name.clone(),
            clock: system::clock_text(),
            battery: system::battery(),
            status: self.status,
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
        root.SetSize(Vector2 {
            X: width,
            Y: height,
        })?;
        let layers = root.Children()?;
        // Without transparency the bar is solid, as macOS draws it with
        // "Reduce transparency".
        let tint = match (look.light, look.glass) {
            (true, true) => rgba(255, 255, 255, 110),
            (false, true) => rgba(24, 24, 26, 120),
            (true, false) => rgba(236, 236, 238, 255),
            (false, false) => rgba(36, 36, 38, 255),
        };
        let backdrop = if look.glass {
            Some(c.CreateHostBackdropBrush()?.cast()?)
        } else {
            None
        };
        let fills: [windows::UI::Composition::CompositionBrush; 1] =
            [c.CreateColorBrushWithColor(tint)?.cast()?];
        for brush in backdrop.iter().chain(&fills) {
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

        // Right side, from the edge inward.
        let slop = px(HIT_SLOP);
        let mut right = width - px(SIDE_MARGIN);
        let status = look.status;
        let symbol = |layers: &[(char, f32)]| gfx.glyph_brush(layers, scale, ink);
        let mut items: Vec<(LabelBrush, Action)> = vec![
            (
                gfx.text_brush(&look.clock, scale, false, ink)?,
                Action::Notifications,
            ),
            (gfx.control_center_brush(scale, ink)?, Action::QuickSettings),
        ];
        if status.buttons.search {
            items.push((symbol(&[(glyph::SEARCH, 1.0)])?, Action::Search));
        }
        let network = match status.network {
            // The full fan stays faint behind the bars received, as on a Mac.
            Network::Wifi { bars } => vec![
                (glyph::WIFI_FULL, 0.3),
                (glyph::WIFI[usize::from(bars.clamp(1, 4)) - 1], 1.0),
            ],
            Network::Ethernet => vec![(glyph::ETHERNET, 1.0)],
            Network::Offline => vec![(glyph::OFFLINE, 1.0)],
        };
        items.push((symbol(&network)?, Action::QuickSettings));
        if let Some(volume) = status.volume {
            let speaker = if volume.muted {
                glyph::MUTE
            } else {
                glyph::VOLUME[usize::from(volume.level.min(3))]
            };
            items.push((symbol(&[(speaker, 1.0)])?, Action::Sound));
        }
        if let Some(battery) = look.battery {
            items.push((
                gfx.battery_brush(battery.percent, battery.charging, scale, ink)?,
                Action::QuickSettings,
            ));
        }
        if status.buttons.task_view {
            items.push((symbol(&[(glyph::TASK_VIEW, 1.0)])?, Action::TaskView));
        }
        if status.buttons.widgets {
            items.push((symbol(&[(glyph::WIDGETS, 1.0)])?, Action::Widgets));
        }
        for (item, action) in &items {
            let left = right - item.width;
            place(&layers, item, left)?;
            self.zones.push((left - slop, right + slop, *action));
            right = left - px(ITEM_GAP);
        }

        self.target.SetRoot(&root)?;
        self.root = Some(root);
        self.bounds = RECT {
            bottom: screen.top + height as i32,
            ..screen
        };
        self.place()
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
