//! A non-activating, on-demand window picker. DWM composites the cards'
//! thumbnails directly and releases them as soon as the picker closes.

use anyhow::Result;
use muelle_shell::{system::Monitor, tracker};
use windows::{
    UI::Composition::{ContainerVisual, Desktop::DesktopWindowTarget},
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn},
        System::{LibraryLoader::GetModuleHandleW, WinRT::Composition::ICompositorDesktopInterop},
        UI::WindowsAndMessaging::{
            CS_DROPSHADOW, CreateWindowExW, DefWindowProcW, DestroyWindow, GetForegroundWindow,
            HWND_TOPMOST, IDC_ARROW, IDC_HAND, LoadCursorW, RegisterClassW, SW_SHOWNA,
            SWP_NOACTIVATE, SetCursor, SetWindowPos, ShowWindow, WM_LBUTTONUP, WM_MOUSEMOVE,
            WM_MOUSEWHEEL, WM_SETCURSOR, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP,
            WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
        },
    },
    core::{Interface, w},
};
use windows_numerics::Vector2;

use crate::{
    app,
    gfx::{Gfx, Ink},
    menu::{apply_popup_frame, draw_popup_background},
    menubar::{add_rounded_box, add_visual, rgba},
    thumbnails::Thumbnails,
};

const PREVIEW_WIDTH: f32 = 176.0;
const PREVIEW_GAP: f32 = 12.0;
const PANEL_PADDING: f32 = 12.0;

#[derive(Clone, Copy)]
pub enum Choice {
    Window(isize),
    Page(usize),
}

struct Hit {
    bounds: RECT,
    choice: Choice,
    highlight: ContainerVisual,
}

struct Window(HWND);

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: Created and destroyed on the dock thread.
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

pub struct Picker {
    thumbnails: Thumbnails,
    target: DesktopWindowTarget,
    window: Window,
    pub app_id: String,
    pub windows: Vec<isize>,
    app_name: String,
    foreground: HWND,
    anchor: RECT,
    bounds: RECT,
    hits: Vec<Hit>,
    hot: Option<usize>,
    page: usize,
    per_page: usize,
    monitor: Monitor,
    light: bool,
    glass: bool,
    native_corners: bool,
}

impl Picker {
    pub fn open(
        gfx: &Gfx,
        app: (String, String),
        windows: Vec<isize>,
        anchor: RECT,
        monitor: Monitor,
        theme: (bool, bool),
    ) -> Result<Self> {
        let class = w!("MuelleWindowPreviews");
        // SAFETY: The class and window belong to the dock UI thread.
        let window = Window(unsafe {
            let module = GetModuleHandleW(None)?;
            RegisterClassW(&WNDCLASSW {
                style: CS_DROPSHADOW,
                lpfnWndProc: Some(proc),
                hInstance: module.into(),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                lpszClassName: class,
                ..Default::default()
            });
            CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                class,
                w!("Muelle"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(module.into()),
                None,
            )?
        });
        let hwnd = window.0;
        let native_corners = apply_popup_frame(hwnd, !theme.0);
        // SAFETY: hwnd is a live, top-level window owned by this process.
        let (target, foreground) = unsafe {
            (
                gfx.compositor
                    .cast::<ICompositorDesktopInterop>()?
                    .CreateDesktopWindowTarget(hwnd, true)?,
                GetForegroundWindow(),
            )
        };
        let available = (monitor.screen.right - monitor.screen.left) as f32 / monitor.scale;
        let per_page = ((available - 2.0 * PANEL_PADDING + PREVIEW_GAP)
            / (PREVIEW_WIDTH + PREVIEW_GAP))
            .floor()
            .clamp(1.0, 4.0) as usize;
        let mut picker = Self {
            thumbnails: Thumbnails::new(hwnd),
            target,
            window,
            app_id: app.0,
            app_name: app.1,
            windows,
            foreground,
            anchor,
            bounds: RECT::default(),
            hits: Vec::new(),
            hot: None,
            page: 0,
            per_page,
            monitor,
            light: theme.0,
            glass: theme.1,
            native_corners,
        };
        picker.draw(gfx, 0)?;
        Ok(picker)
    }

    pub fn draw(&mut self, gfx: &Gfx, page: usize) -> Result<()> {
        let px = |logical: f32| (logical * self.monitor.scale).round();
        let total = self.windows.len().div_ceil(self.per_page);
        let page = page.min(total - 1);
        let count = self.windows.len().min(self.per_page);
        let start = page * self.per_page;
        let visible = &self.windows[start..(start + count).min(self.windows.len())];
        let screen = self.monitor.screen;
        let gap = px(PREVIEW_GAP);
        let inset = px(PANEL_PADDING);
        let card = px(PREVIEW_WIDTH).min(
            (((screen.right - screen.left) as f32 - 2.0 * inset - gap * (count - 1) as f32)
                / count as f32)
                .floor()
                .max(1.0),
        );
        let width = inset * 2.0 + card * count as f32 + gap * (count - 1) as f32;
        let first_card = inset + (card + gap) * (count - visible.len()) as f32 / 2.0;
        let height = px(168.0);
        let top = (self.anchor.top - height as i32 - px(7.0) as i32).max(screen.top);
        let left = ((self.anchor.left + self.anchor.right - width as i32) / 2)
            .clamp(screen.left, (screen.right - width as i32).max(screen.left));
        let c = &gfx.compositor;
        let root = c.CreateContainerVisual()?;
        root.SetSize(Vector2::new(width, height))?;
        let layers = root.Children()?;
        draw_popup_background(&layers, !self.light, self.glass, width, height)?;
        let hwnd = self.window.0;
        // SAFETY: This window belongs to the UI thread. A region is used when
        // DWM doesn't accept the native corner preference and owned by Windows
        // only if SetWindowRgn succeeds.
        unsafe {
            if !self.native_corners {
                let diameter = px(16.0) as i32;
                let region = CreateRoundRectRgn(
                    0,
                    0,
                    width as i32 + 1,
                    height as i32 + 1,
                    diameter,
                    diameter,
                );
                if !region.is_invalid() && SetWindowRgn(hwnd, Some(region), true) == 0 {
                    let _ = DeleteObject(region.into());
                }
            }
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                left,
                top,
                width as i32,
                height as i32,
                SWP_NOACTIVATE,
            )?;
        }
        let ink = if self.light { Ink::Dark } else { Ink::Light };
        let label = gfx.text_brush_fitted(
            &self.app_name,
            self.monitor.scale,
            12.0,
            true,
            ink,
            width - px(if total > 1 { 110.0 } else { 30.0 }),
        )?;
        add_visual(
            c,
            &layers,
            &label.brush.cast()?,
            px(16.0),
            (px(35.0) - label.height) / 2.0,
            label.width,
            label.height,
        )?;

        let mut hits = Vec::with_capacity(visible.len() + 2);
        let outline = if self.light {
            rgba(29, 34, 42, 90)
        } else {
            rgba(249, 250, 252, 112)
        };
        self.thumbnails.sync(visible);
        for (index, &hwnd) in visible.iter().enumerate() {
            let x = (first_card + (card + gap) * index as f32).round();
            let slot = rect(x, px(32.0), card, px(106.0));
            let image = self.thumbnails.place_preview(hwnd, slot).unwrap_or(slot);
            let hot = c.CreateContainerVisual()?;
            hot.SetSize(Vector2::new(width, height))?;
            let halo = px(2.0);
            add_rounded_box(
                c,
                &hot.Children()?,
                image.left as f32 - halo,
                image.top as f32 - halo,
                (image.right - image.left) as f32 + halo * 2.0,
                (image.bottom - image.top) as f32 + halo * 2.0,
                px(4.0),
                rgba(0, 0, 0, 0),
                Some(outline),
            )?;
            hot.SetIsVisible(false)?;
            layers.InsertAtTop(&hot)?;
            let title = tracker::title(hwnd);
            let name = if title.is_empty() {
                &self.app_name
            } else {
                &title
            };
            let name = gfx.text_brush_fitted(name, self.monitor.scale, 11.5, false, ink, card)?;
            add_visual(
                c,
                &layers,
                &name.brush.cast()?,
                x,
                slot.bottom as f32 + px(7.0),
                name.width,
                name.height,
            )?;
            hits.push(Hit {
                bounds: rect(x - gap / 2.0, px(29.0), card + gap, height - px(34.0)),
                choice: Choice::Window(hwnd),
                highlight: hot,
            });
        }
        if total > 1 {
            let number = format!("{}/{}", page + 1, total);
            let label = gfx.text_brush_sized(&number, self.monitor.scale, 10.5, false, ink)?;
            add_visual(
                c,
                &layers,
                &label.brush.cast()?,
                width - px(40.0) - label.width / 2.0,
                (px(35.0) - label.height) / 2.0,
                label.width,
                label.height,
            )?
            .SetOpacity(0.65)?;
            for (glyph, x, destination) in [
                ('‹', width - px(76.0), page.checked_sub(1)),
                (
                    '›',
                    width - px(25.0),
                    (page + 1 < total).then_some(page + 1),
                ),
            ] {
                let label =
                    gfx.text_brush_sized(&glyph.to_string(), self.monitor.scale, 17.0, true, ink)?;
                let y = px(6.0);
                let side = px(21.0);
                let hot = c.CreateContainerVisual()?;
                hot.SetSize(Vector2::new(width, height))?;
                let fill = if self.light {
                    rgba(0, 0, 0, 21)
                } else {
                    rgba(255, 255, 255, 24)
                };
                add_rounded_box(c, &hot.Children()?, x, y, side, side, px(5.0), fill, None)?;
                hot.SetIsVisible(false)?;
                layers.InsertAtTop(&hot)?;
                let button = add_visual(
                    c,
                    &layers,
                    &label.brush.cast()?,
                    x + (side - label.width) / 2.0,
                    y + (side - label.height) / 2.0,
                    label.width,
                    label.height,
                )?;
                if let Some(destination) = destination {
                    hits.push(Hit {
                        bounds: rect(x, y, side, side),
                        choice: Choice::Page(destination),
                        highlight: hot,
                    });
                } else {
                    button.SetOpacity(0.27)?;
                }
            }
        }
        self.target.SetRoot(&root)?;
        // SAFETY: Show the picker without taking focus from the active app.
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNA);
        }
        self.bounds = rect(left as f32, top as f32, width, height);
        self.hits = hits;
        self.hot = None;
        self.page = page;
        Ok(())
    }

    pub fn contains(&self, point: POINT) -> bool {
        // SAFETY: Reading the foreground does not activate the picker.
        if unsafe { GetForegroundWindow() } != self.foreground {
            return false;
        }
        let near_anchor = point.y >= self.anchor.top
            && point.y < self.anchor.bottom
            && point.x >= self.anchor.left - 4
            && point.x < self.anchor.right + 4;
        let bridge = point.y >= self.bounds.bottom
            && point.y < self.anchor.top
            && point.x >= self.bounds.left.min(self.anchor.left)
            && point.x < self.bounds.right.max(self.anchor.right);
        inside(self.bounds, point) || near_anchor || bridge
    }

    pub fn choice_at(&self, point: POINT) -> Option<Choice> {
        self.hits
            .iter()
            .find(|hit| inside(hit.bounds, point))
            .map(|hit| hit.choice)
    }

    pub fn hover(&mut self, point: POINT) -> Result<()> {
        let hit = self.hits.iter().position(|hit| inside(hit.bounds, point));
        if hit != self.hot {
            if let Some(index) = self.hot {
                self.hits[index].highlight.SetIsVisible(false)?;
            }
            if let Some(index) = hit {
                self.hits[index].highlight.SetIsVisible(true)?;
            }
            self.hot = hit;
        }
        Ok(())
    }

    pub fn next_page(&self, delta: i16) -> Option<usize> {
        let next = if delta > 0 {
            self.page.checked_sub(1)?
        } else {
            self.page + 1
        };
        (next < self.windows.len().div_ceil(self.per_page)).then_some(next)
    }
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> RECT {
    RECT {
        left: x as i32,
        top: y as i32,
        right: (x + w) as i32,
        bottom: (y + h) as i32,
    }
}

fn inside(rect: RECT, point: POINT) -> bool {
    point.x >= rect.left && point.x < rect.right && point.y >= rect.top && point.y < rect.bottom
}

unsafe extern "system" fn proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let point = POINT {
        x: (l.0 as i16).into(),
        y: ((l.0 >> 16) as i16).into(),
    };
    match message {
        WM_MOUSEMOVE => app::preview_hover(point),
        WM_LBUTTONUP => app::preview_click(point),
        WM_MOUSEWHEEL => app::preview_scroll((w.0 >> 16) as i16),
        WM_SETCURSOR if app::preview_has_hit(hwnd) => {
            // SAFETY: The stock hand cursor is owned by Windows.
            if let Ok(hand) = unsafe { LoadCursorW(None, IDC_HAND) } {
                unsafe { SetCursor(Some(hand)) };
                return LRESULT(1);
            }
        }
        // SAFETY: Default handling for the preview's own window.
        _ => return unsafe { DefWindowProcW(hwnd, message, w, l) },
    }
    LRESULT(0)
}

#[cfg(test)]
mod native_probe {
    use super::*;
    use windows::Win32::System::WinRT::{
        CreateDispatcherQueueController, DQTAT_COM_STA, DQTYPE_THREAD_CURRENT,
        DispatcherQueueOptions,
    };

    #[test]
    #[ignore = "Requires an interactive Windows desktop with DWM"]
    fn picker_places_live_thumbnails_on_the_shared_caption_line() -> Result<()> {
        // SAFETY: The controller initializes and owns the test thread's queue.
        let _queue = unsafe {
            CreateDispatcherQueueController(DispatcherQueueOptions {
                dwSize: size_of::<DispatcherQueueOptions>() as u32,
                threadType: DQTYPE_THREAD_CURRENT,
                apartmentType: DQTAT_COM_STA,
            })?
        };
        let gfx = Gfx::new()?;
        // SAFETY: These hidden source windows belong to this thread and are
        // destroyed by their Window wrappers.
        let fixture = || unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("STATIC"),
                w!("Muelle preview test"),
                WS_POPUP,
                0,
                0,
                160,
                120,
                None,
                None,
                None,
                None,
            )
            .map(Window)
        };
        let sources = [fixture()?, fixture()?];
        let handles = sources.iter().map(|source| source.0.0 as isize).collect();
        let monitor =
            muelle_shell::system::primary_monitor().ok_or_else(|| anyhow::anyhow!("no display"))?;
        let center = (monitor.screen.right + monitor.screen.left) / 2;
        let picker = Picker::open(
            &gfx,
            ("probe".into(), "Muelle".into()),
            handles,
            RECT {
                left: center - 20,
                right: center + 20,
                top: monitor.screen.bottom - 130,
                bottom: monitor.screen.bottom - 90,
            },
            monitor,
            (false, false),
        )?;
        assert_eq!(picker.hits.len(), 2);
        for source in &sources {
            let image = picker
                .thumbnails
                .placed_bounds(source.0.0 as isize)
                .ok_or_else(|| anyhow::anyhow!("DWM did not place the preview"))?;
            let caption_line = (32.0 * monitor.scale).round() + (106.0 * monitor.scale).round();
            assert_eq!(image.bottom, caption_line as i32);
        }
        Ok(())
    }
}
