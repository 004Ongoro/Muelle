//! Hover-only window picker. DWM owns the live previews; the dock draws cards
//! and titles without capturing or retaining the contents of other windows.

use std::time::{Duration, Instant};

use anyhow::Result;
use muelle_shell::{system::animations_enabled, tracker};
use windows::{
    UI::{
        Color,
        Composition::{ContainerVisual, Desktop::DesktopWindowTarget, SpriteVisual},
    },
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Dwm::{DWMWA_USE_HOSTBACKDROPBRUSH, DwmSetWindowAttribute},
        System::{LibraryLoader::GetModuleHandleW, WinRT::Composition::ICompositorDesktopInterop},
        UI::{
            Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos, HWND_TOPMOST,
                IDC_ARROW, KillTimer, LoadCursorW, MA_NOACTIVATE, RegisterClassW, SWP_NOACTIVATE,
                SWP_SHOWWINDOW, SetTimer, SetWindowPos, WM_LBUTTONDOWN, WM_LBUTTONUP,
                WM_MOUSEACTIVATE, WM_MOUSEWHEEL, WM_RBUTTONUP, WM_TIMER, WNDCLASSW,
                WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
                WS_POPUP,
            },
        },
    },
    core::{BOOL, Interface, w},
};
use windows_numerics::{Vector2, Vector3};

use crate::{
    app,
    gfx::{Gfx, Ink},
    scene::Theme,
    strings::text,
    thumbnails::Thumbnails,
};

const TIMER: usize = 1;
const PADDING: f32 = 18.0;
const GAP: f32 = 12.0;
const CARD_WIDTH: f32 = 216.0;
const CARD_HEIGHT: f32 = 164.0;
const HEADER: f32 = 24.0;
const ARC: f32 = 16.0;
const OPEN_SECONDS: f32 = 0.2;
const LEAVE_DELAY: Duration = Duration::from_millis(220);

/// Pure layout shared by rendering and hit testing, including negative-origin
/// monitors. Overflow is paged instead of shrinking previews to unreadable sizes.
struct MultiWindowLayout {
    bounds: RECT,
    cards: Vec<RECT>,
    per_page: usize,
}

impl MultiWindowLayout {
    fn new(screen: RECT, anchor: RECT, scale: f32, count: usize) -> Self {
        let available = ((screen.right - screen.left) as f32 - 32.0 * scale).max(1.0);
        let per_page = (((available - 2.0 * PADDING * scale) / ((CARD_WIDTH + GAP) * scale))
            as usize)
            .clamp(1, 5);
        let shown = count.min(per_page);
        let header = if count > per_page { HEADER } else { 0.0 };
        let card_width = (CARD_WIDTH * scale).min((available - 2.0 * PADDING * scale).max(1.0));
        let width = (2.0 * PADDING * scale
            + shown as f32 * card_width
            + shown.saturating_sub(1) as f32 * GAP * scale)
            .ceil() as i32;
        let height = ((2.0 * PADDING + header + CARD_HEIGHT + ARC) * scale).ceil() as i32;
        let left = ((anchor.left + (anchor.right - anchor.left) / 2) - width / 2)
            .clamp(screen.left, (screen.right - width).max(screen.left));
        let top = (anchor.top - height - (10.0 * scale) as i32).max(screen.top);
        let cards = (0..shown)
            .map(|index| {
                let across = if shown > 1 {
                    index as f32 / (shown - 1) as f32 * 2.0 - 1.0
                } else {
                    0.0
                };
                let x = PADDING * scale + index as f32 * (card_width + GAP * scale);
                let y = (PADDING + header + ARC * across * across) * scale;
                RECT {
                    left: x.round() as i32,
                    top: y.round() as i32,
                    right: (x + card_width).round() as i32,
                    bottom: (y + CARD_HEIGHT * scale).round() as i32,
                }
            })
            .collect();
        Self {
            bounds: RECT {
                left,
                top,
                right: left + width,
                bottom: top + height,
            },
            cards,
            per_page,
        }
    }

    fn hit(&self, point: POINT) -> Option<usize> {
        self.cards.iter().position(|rect| contains(*rect, point))
    }

    fn keeps_open(&self, anchor: RECT, point: POINT) -> bool {
        if contains(self.bounds, point) || contains(anchor, point) {
            return true;
        }
        // A bridge just above the source icon lets the pointer reach the view
        // without keeping the popup open over unrelated applications.
        contains(
            RECT {
                left: anchor.left,
                right: anchor.right,
                top: self.bounds.bottom.min(anchor.top),
                bottom: anchor.bottom,
            },
            point,
        )
    }
}

fn contains(rect: RECT, point: POINT) -> bool {
    point.x >= rect.left && point.x < rect.right && point.y >= rect.top && point.y < rect.bottom
}

struct Card {
    root: ContainerVisual,
    highlight: SpriteVisual,
}

pub struct MultiWindowView {
    pub app_id: String,
    hwnd: HWND,
    target: Option<DesktopWindowTarget>,
    thumbnails: Thumbnails,
    windows: Vec<isize>,
    layout: MultiWindowLayout,
    anchor: RECT,
    scale: f32,
    theme: Theme,
    page: usize,
    cards: Vec<Card>,
    opened: Instant,
    animated: bool,
    left_at: Option<Instant>,
    pressed: Option<usize>,
    mouse_was_down: bool,
    foreground: isize,
}

impl MultiWindowView {
    pub fn show(
        gfx: &Gfx,
        app_id: String,
        windows: Vec<isize>,
        anchor: RECT,
        screen: RECT,
        scale: f32,
        theme: Theme,
    ) -> Result<Self> {
        // SAFETY: Class and window belong to this UI thread. Once created,
        // `view` owns the HWND even if subsequent graphics setup fails.
        let hwnd = unsafe {
            let instance = GetModuleHandleW(None)?;
            let class = w!("MuelleMultiWindowView");
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(multi_window_view_proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                lpszClassName: class,
                ..Default::default()
            });
            CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class,
                w!("Muelle window previews"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance.into()),
                None,
            )?
        };
        let mut view = Self {
            app_id,
            hwnd,
            target: None,
            thumbnails: Thumbnails::new(hwnd),
            layout: MultiWindowLayout::new(screen, anchor, scale, windows.len()),
            windows,
            anchor,
            scale,
            theme,
            page: 0,
            cards: Vec::new(),
            opened: Instant::now(),
            animated: animations_enabled(),
            left_at: None,
            pressed: None,
            mouse_was_down: false,
            foreground: tracker::foreground_window(),
        };
        // Match the dock's native backdrop setup. Older Windows versions can
        // reject the attribute; their theme already supplies an opaque tint.
        let enable = BOOL::from(true);
        unsafe {
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_HOSTBACKDROPBRUSH,
                (&raw const enable).cast(),
                size_of::<BOOL>() as u32,
            );
        }
        // SAFETY: This process owns the top-level destination window.
        view.target = Some(unsafe {
            gfx.compositor
                .cast::<ICompositorDesktopInterop>()?
                .CreateDesktopWindowTarget(hwnd, true)?
        });
        view.rebuild(gfx)?;
        let bounds = view.layout.bounds;
        // SAFETY: Show our window without stealing focus from the active app.
        unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                bounds.left,
                bounds.top,
                bounds.right - bounds.left,
                bounds.bottom - bounds.top,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )?;
            SetTimer(Some(hwnd), TIMER, 16, None);
        }
        view.update()?;
        Ok(view)
    }

    pub fn matches(&self, windows: &[isize]) -> bool {
        self.windows.len() == windows.len()
            && self.windows.iter().all(|hwnd| windows.contains(hwnd))
    }

    fn local_cursor(&self) -> Result<POINT> {
        let mut point = POINT::default();
        // SAFETY: Valid output buffer for a read-only query.
        unsafe {
            GetCursorPos(&mut point)?;
        }
        point.x -= self.layout.bounds.left;
        point.y -= self.layout.bounds.top;
        Ok(point)
    }

    fn rebuild(&mut self, gfx: &Gfx) -> Result<()> {
        let c = &gfx.compositor;
        let root = c.CreateContainerVisual()?;
        let layers = root.Children()?;
        let width = (self.layout.bounds.right - self.layout.bounds.left) as f32;
        let height = (self.layout.bounds.bottom - self.layout.bounds.top) as f32;
        let panel = self.theme.panel(
            gfx,
            Vector2 {
                X: width,
                Y: height,
            },
            18.0 * self.scale,
            self.scale,
        )?;
        layers.InsertAtTop(&panel)?;
        let ink = if self.theme.light {
            Ink::Dark
        } else {
            Ink::Light
        };
        if self.windows.len() > self.layout.per_page {
            let heading = format!(
                "{}–{} / {}",
                self.page + 1,
                (self.page + self.layout.per_page).min(self.windows.len()),
                self.windows.len()
            );
            let label = gfx.text_brush(&heading, self.scale, true, ink)?;
            let header = c.CreateSpriteVisual()?;
            header.SetBrush(&label.brush)?;
            header.SetSize(Vector2 {
                X: label.width.min(width - 2.0 * PADDING * self.scale),
                Y: label.height,
            })?;
            header.SetOffset(Vector3 {
                X: PADDING * self.scale,
                Y: 12.0 * self.scale,
                Z: 0.0,
            })?;
            layers.InsertAtTop(&header)?;
        }
        self.cards.clear();
        let shown =
            &self.windows[self.page..(self.page + self.layout.per_page).min(self.windows.len())];
        self.thumbnails.sync(shown);
        for (&hwnd, rect) in shown.iter().zip(&self.layout.cards) {
            let card = c.CreateContainerVisual()?;
            let size = Vector2 {
                X: (rect.right - rect.left) as f32,
                Y: (rect.bottom - rect.top) as f32,
            };
            card.SetSize(size)?;
            let shape = c.CreateRoundedRectangleGeometry()?;
            shape.SetSize(size)?;
            shape.SetCornerRadius(Vector2 {
                X: 12.0 * self.scale,
                Y: 12.0 * self.scale,
            })?;
            card.SetClip(&c.CreateGeometricClipWithGeometry(&shape)?)?;
            let background = c.CreateSpriteVisual()?;
            background.SetSize(size)?;
            background.SetBrush(&c.CreateColorBrushWithColor(if self.theme.light {
                Color {
                    A: 56,
                    R: 241,
                    G: 241,
                    B: 244,
                }
            } else {
                Color {
                    A: 40,
                    R: 38,
                    G: 38,
                    B: 43,
                }
            })?)?;
            card.Children()?.InsertAtTop(&background)?;
            let highlight = c.CreateSpriteVisual()?;
            highlight.SetSize(size)?;
            highlight.SetBrush(&c.CreateColorBrushWithColor(Color {
                A: 75,
                R: 80,
                G: 150,
                B: 255,
            })?)?;
            highlight.SetOpacity(0.0)?;
            card.Children()?.InsertAtTop(&highlight)?;
            let title = tracker::title(hwnd);
            let mut title = if title.is_empty() {
                text().new_window.replace('&', "")
            } else {
                title
            };
            if title.chars().count() > 27 {
                title = title.chars().take(26).chain(['…']).collect();
            }
            let label = gfx.text_brush(&title, self.scale, false, ink)?;
            let caption = c.CreateSpriteVisual()?;
            caption.SetBrush(&label.brush)?;
            caption.SetSize(Vector2 {
                X: label.width,
                Y: label.height,
            })?;
            caption.SetOffset(Vector3 {
                X: 10.0 * self.scale,
                Y: size.Y - 29.0 * self.scale,
                Z: 0.0,
            })?;
            card.Children()?.InsertAtTop(&caption)?;
            layers.InsertAtTop(&card)?;
            self.cards.push(Card {
                root: card,
                highlight,
            });
        }
        if let Some(target) = &self.target {
            target.SetRoot(&root)?;
        }
        self.opened = Instant::now();
        Ok(())
    }

    fn update(&mut self) -> Result<()> {
        let elapsed = self.opened.elapsed().as_secs_f32();
        let progress = if self.animated {
            (elapsed / OPEN_SECONDS).min(1.0)
        } else {
            1.0
        };
        let eased = 1.0 - (1.0 - progress).powi(3);
        let hovered = self.layout.hit(self.local_cursor()?);
        let center = (self.anchor.left + (self.anchor.right - self.anchor.left) / 2
            - self.layout.bounds.left) as f32;
        for (index, (card, rect)) in self.cards.iter().zip(&self.layout.cards).enumerate() {
            let width = (rect.right - rect.left) as f32;
            let start_x = center - width / 2.0;
            let x = start_x + (rect.left as f32 - start_x) * eased;
            let y = rect.top as f32 + (1.0 - eased) * 36.0 * self.scale;
            card.root.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            card.root.SetOpacity(eased)?;
            card.highlight
                .SetOpacity(if hovered == Some(index) { 1.0 } else { 0.0 })?;
            if let Some(&hwnd) = self.windows.get(self.page + index) {
                self.thumbnails.place_preview(
                    hwnd,
                    RECT {
                        left: (x + 4.0 * self.scale).round() as i32,
                        top: (y + 4.0 * self.scale).round() as i32,
                        right: (x + width - 4.0 * self.scale).round() as i32,
                        bottom: (y + (CARD_HEIGHT - 31.0) * self.scale).round() as i32,
                    },
                    (255.0 * eased) as u8,
                );
            }
        }
        // Run quickly only during the opening view animation. Nothing is polled
        // when the popup is closed, and thumbnail pixels are maintained by DWM.
        unsafe {
            SetTimer(
                Some(self.hwnd),
                TIMER,
                if progress < 1.0 { 16 } else { 60 },
                None,
            );
        }
        Ok(())
    }

    /// Returns false after Escape, an outside click, or leaving the view/icon bridge.
    pub fn tick(&mut self) -> Result<bool> {
        if tracker::foreground_window() != self.foreground {
            return Ok(false);
        }
        let mut point = POINT::default();
        // SAFETY: Read-only pointer/key state queries; no global input hook.
        let (escape, mouse_down) = unsafe {
            GetCursorPos(&mut point)?;
            (
                GetAsyncKeyState(i32::from(VK_ESCAPE.0)) < 0,
                GetAsyncKeyState(i32::from(VK_LBUTTON.0)) < 0,
            )
        };
        let inside = self.layout.keeps_open(self.anchor, point);
        if escape || (mouse_down && !self.mouse_was_down && !inside) {
            return Ok(false);
        }
        self.mouse_was_down = mouse_down;
        if inside {
            self.left_at = None;
        } else if self.left_at.get_or_insert_with(Instant::now).elapsed() >= LEAVE_DELAY {
            return Ok(false);
        }
        self.update()?;
        Ok(true)
    }

    pub fn press(&mut self) -> Result<()> {
        self.press_at(self.local_cursor()?);
        Ok(())
    }

    fn press_at(&mut self, point: POINT) {
        // Do not select a card while it is still moving past another card.
        self.pressed = if !self.animated || self.opened.elapsed().as_secs_f32() >= OPEN_SECONDS {
            self.layout.hit(point)
        } else {
            None
        };
    }

    pub fn release(&mut self) -> Result<Option<isize>> {
        Ok(self.release_at(self.local_cursor()?))
    }

    fn release_at(&mut self, point: POINT) -> Option<isize> {
        let hit = self.layout.hit(point);
        self.pressed
            .take()
            .filter(|pressed| Some(*pressed) == hit)
            .and_then(|index| self.windows.get(self.page + index).copied())
    }

    pub fn scroll(&mut self, gfx: &Gfx, previous: bool) -> Result<()> {
        let page = if previous {
            self.page.saturating_sub(self.layout.per_page)
        } else {
            (self.page + self.layout.per_page).min(
                self.windows.len().saturating_sub(1) / self.layout.per_page * self.layout.per_page,
            )
        };
        if page != self.page {
            self.page = page;
            self.pressed = None;
            self.rebuild(gfx)?;
            self.update()?;
        }
        Ok(())
    }
}

impl Drop for MultiWindowView {
    fn drop(&mut self) {
        // Release DWM relationships and composition target before the HWND.
        self.thumbnails.sync(&[]);
        self.target = None;
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

unsafe extern "system" fn multi_window_view_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        WM_TIMER if w.0 == TIMER => app::multi_window_view_tick(),
        WM_LBUTTONDOWN => app::multi_window_view_press(),
        WM_LBUTTONUP => app::multi_window_view_release(),
        WM_RBUTTONUP => app::multi_window_view_dismiss(),
        WM_MOUSEWHEEL => app::multi_window_view_scroll(((w.0 >> 16) as i16) > 0),
        _ => return unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
    LRESULT(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an interactive Windows desktop and DWM"]
    fn native_multi_window_view_should_page_select_without_focus_and_release_its_window()
    -> Result<()> {
        use windows::Win32::{
            System::WinRT::{
                CreateDispatcherQueueController, DQTAT_COM_STA, DQTYPE_THREAD_CURRENT,
                DispatcherQueueOptions,
            },
            UI::WindowsAndMessaging::{
                GetForegroundWindow, IsWindow, IsWindowVisible, SW_SHOWNOACTIVATE, ShowWindow,
                WS_OVERLAPPEDWINDOW,
            },
        };
        struct Source(HWND);
        impl Drop for Source {
            fn drop(&mut self) {
                unsafe {
                    let _ = DestroyWindow(self.0);
                }
            }
        }
        // These blank windows belong to the test; no existing user window is
        // activated, moved, closed or used as a preview source.
        let _queue = unsafe {
            CreateDispatcherQueueController(DispatcherQueueOptions {
                dwSize: size_of::<DispatcherQueueOptions>() as u32,
                threadType: DQTYPE_THREAD_CURRENT,
                apartmentType: DQTAT_COM_STA,
            })?
        };
        let foreground = unsafe { GetForegroundWindow() };
        let mut sources = Vec::new();
        for _ in 0..6 {
            let hwnd = unsafe {
                CreateWindowExW(
                    WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    w!("STATIC"),
                    w!("Muelle preview test"),
                    WS_OVERLAPPEDWINDOW,
                    10,
                    10,
                    300,
                    180,
                    None,
                    None,
                    None,
                    None,
                )?
            };
            let source = Source(hwnd);
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            sources.push(source);
        }
        let windows: Vec<_> = sources.iter().map(|source| source.0.0 as isize).collect();
        let gfx = Gfx::new()?;
        for theme in [
            Theme::for_system(false, false),
            Theme::for_system(false, true),
            Theme::for_system(true, false),
            Theme::for_system(true, true),
        ] {
            let mut view = MultiWindowView::show(
                &gfx,
                "test".into(),
                windows.clone(),
                RECT {
                    left: 280,
                    top: 600,
                    right: 340,
                    bottom: 670,
                },
                RECT {
                    left: 0,
                    top: 0,
                    right: 640,
                    bottom: 720,
                },
                1.0,
                theme,
            )?;
            view.animated = false;
            view.update()?;
            for (&source, rect) in windows.iter().zip(&view.layout.cards) {
                let image = view
                    .thumbnails
                    .placed_bounds(source)
                    .ok_or_else(|| anyhow::anyhow!("DWM did not place the preview"))?;
                assert_eq!(
                    image.bottom,
                    (rect.top as f32 + (CARD_HEIGHT - 31.0) * view.scale).round() as i32,
                    "each preview must align with its card's caption baseline"
                );
            }
            let hwnd = view.hwnd;
            assert!(unsafe { IsWindowVisible(hwnd) }.as_bool());
            assert_eq!(
                unsafe { GetForegroundWindow() },
                foreground,
                "hover must not steal focus"
            );
            view.scroll(&gfx, false)?;
            let rect = view.layout.cards[0];
            let point = POINT {
                x: (rect.left + rect.right) / 2,
                y: (rect.top + rect.bottom) / 2,
            };
            view.press_at(point);
            assert_eq!(view.release_at(point), Some(windows[view.layout.per_page]));
            view.press_at(point);
            assert_eq!(
                view.release_at(POINT { x: 0, y: 0 }),
                None,
                "dragging off a card must cancel"
            );
            view.scroll(&gfx, true)?;
            view.press_at(point);
            assert_eq!(view.release_at(point), Some(windows[0]));
            drop(view);
            assert!(
                !unsafe { IsWindow(Some(hwnd)) }.as_bool(),
                "popup must release its HWND"
            );
        }
        Ok(())
    }

    #[test]
    fn multi_window_view_should_stay_on_screen_at_both_edges_and_high_dpi() {
        for scale in [1.0, 1.5, 2.0] {
            for left in [-1920, -960, -60] {
                let screen = RECT {
                    left: -1920,
                    top: -200,
                    right: 0,
                    bottom: 1080,
                };
                let anchor = RECT {
                    left,
                    top: 950,
                    right: left + 60,
                    bottom: 1020,
                };
                let layout = MultiWindowLayout::new(screen, anchor, scale, 12);
                assert!(layout.bounds.left >= screen.left && layout.bounds.right <= screen.right);
                for rect in &layout.cards {
                    assert!(
                        rect.left >= 0 && rect.right <= layout.bounds.right - layout.bounds.left
                    );
                    assert!(
                        rect.top >= 0 && rect.bottom <= layout.bounds.bottom - layout.bounds.top
                    );
                }
            }
        }
    }

    #[test]
    fn pointer_should_cross_gap_from_icon_without_dismissing() {
        let anchor = RECT {
            left: 900,
            top: 950,
            right: 960,
            bottom: 1020,
        };
        let layout = MultiWindowLayout::new(
            RECT {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            anchor,
            1.0,
            2,
        );
        assert!(layout.keeps_open(
            anchor,
            POINT {
                x: 930,
                y: anchor.top - 5
            }
        ));
        assert!(!layout.keeps_open(
            anchor,
            POINT {
                x: 20,
                y: anchor.top - 5
            }
        ));
    }

    #[test]
    fn every_visible_card_should_have_a_distinct_click_target() {
        let layout = MultiWindowLayout::new(
            RECT {
                left: 0,
                top: 0,
                right: 1366,
                bottom: 768,
            },
            RECT {
                left: 600,
                top: 660,
                right: 660,
                bottom: 730,
            },
            1.0,
            20,
        );
        for (index, rect) in layout.cards.iter().enumerate() {
            assert_eq!(
                layout.hit(POINT {
                    x: (rect.left + rect.right) / 2,
                    y: (rect.top + rect.bottom) / 2
                }),
                Some(index)
            );
        }
        assert!(layout.per_page < 20);
    }
}
