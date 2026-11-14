//! Genie animation: a window flies into the dock when minimized and back out
//! when restored from the dock.
//!
//! Windows cannot deform another app's window, Windows 11 ignores the shell's
//! minimize-target hint, and a minimized window can no longer be captured. So,
//! to minimize:
//!
//! 1. one live DWM thumbnail is shown exactly where the window was (nothing
//!    changes on screen);
//! 2. a frame later that rectangle is copied from the screen: a snapshot, as
//!    macOS also uses for its genie;
//! 3. the snapshot becomes one composition surface shared by thin horizontal
//!    strips (plain composition visuals), placed every frame along the genie
//!    curves (`muelle_core::genie`).
//!
//! To restore, the same strips play backwards from a quarter-size copy of that
//! snapshot (a [`Snapshot`], kept while the window is minimized), and the real
//! window is restored underneath once they reach full size. A window that was
//! not minimized with the genie has no snapshot and restores without it.
//!
//! Many DWM thumbnails instead of a snapshot were measured and rejected: DWM
//! drops frames beyond about 16 of them (plan section 8).
//!
//! Everything lives on a transparent overlay window that exists only while
//! the animation runs. The system's own minimize animation must already be
//! off for the window (see `App::calm_windows`): it starts before the dock
//! hears about the minimize and cannot be cancelled afterwards.

use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};

use anyhow::Result;
use muelle_core::genie::{Edges, Genie, Strip};
use muelle_shell::{icons::IconImage, screen};
use windows::{
    UI::Composition::{
        CompositionStretch, CompositionSurfaceBrush, ContainerVisual, Desktop::DesktopWindowTarget,
        SpriteVisual,
    },
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Dwm::{
            DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION,
            DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE, DwmRegisterThumbnail,
            DwmUnregisterThumbnail, DwmUpdateThumbnailProperties,
        },
        System::{LibraryLoader::GetModuleHandleW, WinRT::Composition::ICompositorDesktopInterop},
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, SW_SHOWNOACTIVATE,
            ShowWindow, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP,
            WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
        },
    },
    core::{Interface, w},
};
use windows_numerics::{Matrix3x2, Vector2, Vector3};

use crate::gfx::Gfx;

const DURATION_SECONDS: f32 = 0.5;
/// Screen rows per strip: the edge of the funnel steps by at most a few pixels.
const STRIP_ROWS: u32 = 3;
const MAX_STRIPS: u32 = 400;
/// Frames the thumbnail stays up before the screen is copied, so DWM has
/// certainly presented it.
const FRAMES_BEFORE_SNAPSHOT: u32 = 2;
/// The dock's own thumbnail fades in over the last part of the animation.
const LANDING_START: f32 = 0.8;
/// How long the last frame of a restore stays up, so the real window has
/// certainly appeared underneath it.
const SETTLE: Duration = Duration::from_millis(120);

/// What a minimized window looked like, to fly it back out of the dock.
/// Half the resolution in each direction: a quarter of the memory, and the
/// softness does not show in a half-second animation.
pub struct Snapshot {
    image: IconImage,
    /// Where the window was, in screen coordinates.
    window: RECT,
}

impl Snapshot {
    /// Width of the window divided by its height.
    pub fn aspect(&self) -> f32 {
        let window = self.window;
        (window.right - window.left) as f32 / (window.bottom - window.top).max(1) as f32
    }
}

struct Piece {
    visual: SpriteVisual,
    brush: CompositionSurfaceBrush,
    /// First source row and row count, in snapshot pixels.
    top: f32,
    rows: f32,
    /// Last placement applied (`None` = hidden), to skip redundant calls.
    shown: Option<[i32; 4]>,
}

enum Phase {
    /// The stand-in thumbnail is on screen; counting frames until the snapshot.
    Standing {
        thumbnail: isize,
        frames: u32,
    },
    Flying {
        start: Instant,
        pieces: Vec<Piece>,
    },
    /// A restore has reached full size and waits for the real window.
    Settling {
        until: Instant,
    },
}

pub struct Flight {
    /// The window being minimized or restored.
    pub hwnd: isize,
    overlay: HWND,
    /// Kept alive: it owns the overlay's visual tree.
    _target: DesktopWindowTarget,
    root: ContainerVisual,
    /// Overlay origin on screen.
    origin: (i32, i32),
    genie: Genie,
    phase: Phase,
    /// Scratch buffer reused every frame.
    strips: Vec<Strip>,
    /// Width of the surface the strips sample, in its own pixels.
    source_width: f32,
    /// Out of the dock instead of into it.
    restoring: bool,
    /// The real window still has to be restored (restore flights only).
    restore_pending: bool,
    snapshot: Option<Snapshot>,
}

unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SAFETY: Standard forwarding; the overlay handles no messages itself.
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}

fn intersect(a: RECT, b: RECT) -> RECT {
    RECT {
        left: a.left.max(b.left),
        top: a.top.max(b.top),
        right: a.right.min(b.right),
        bottom: a.bottom.min(b.bottom),
    }
}

fn is_empty(rect: RECT) -> bool {
    rect.right <= rect.left || rect.bottom <= rect.top
}

/// Averages each 2×2 block of pixels into one.
fn halved(image: &IconImage) -> IconImage {
    let (width, height) = ((image.width / 2).max(1), (image.height / 2).max(1));
    let stride = image.width as usize * 4;
    let mut bgra = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height as usize {
        // The last row or column of a one-pixel-wide image pairs with itself.
        let upper = (y * 2).min(image.height as usize - 1) * stride;
        let lower = (y * 2 + 1).min(image.height as usize - 1) * stride;
        for x in 0..width as usize {
            let left = (x * 2).min(image.width as usize - 1) * 4;
            let right = (x * 2 + 1).min(image.width as usize - 1) * 4;
            for channel in 0..4 {
                let sum = u16::from(image.bgra[upper + left + channel])
                    + u16::from(image.bgra[upper + right + channel])
                    + u16::from(image.bgra[lower + left + channel])
                    + u16::from(image.bgra[lower + right + channel]);
                bgra.push((sum / 4) as u8);
            }
        }
    }
    IconImage {
        width,
        height,
        bgra,
    }
}

/// The transparent, click-through window that covers `screen` during a flight.
fn create_overlay(gfx: &Gfx, screen: RECT) -> Option<(HWND, DesktopWindowTarget, ContainerVisual)> {
    static CLASS: OnceLock<bool> = OnceLock::new();
    let class = w!("MuelleFlight");
    // SAFETY: Standard class registration and window creation on the UI
    // thread; the window is destroyed here on failure, or by `Flight::drop`.
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        CLASS.get_or_init(|| {
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(overlay_proc),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            }) != 0
        });
        let overlay = CreateWindowExW(
            WS_EX_LAYERED
                | WS_EX_TRANSPARENT
                | WS_EX_NOREDIRECTIONBITMAP
                | WS_EX_TOPMOST
                | WS_EX_TOOLWINDOW
                | WS_EX_NOACTIVATE,
            class,
            w!(""),
            WS_POPUP,
            screen.left,
            screen.top,
            screen.right - screen.left,
            screen.bottom - screen.top,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .ok()?;

        let tree = || -> Result<(DesktopWindowTarget, ContainerVisual)> {
            let target = gfx
                .compositor
                .cast::<ICompositorDesktopInterop>()?
                .CreateDesktopWindowTarget(overlay, true)?;
            let root = gfx.compositor.CreateContainerVisual()?;
            root.SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })?;
            target.SetRoot(&root)?;
            Ok((target, root))
        };
        match tree() {
            Ok((target, root)) => Some((overlay, target, root)),
            Err(_) => {
                let _ = DestroyWindow(overlay);
                None
            }
        }
    }
}

/// Cuts `image` into the strips of a window `window_height` screen pixels tall.
fn cut(
    gfx: &Gfx,
    root: &ContainerVisual,
    image: &IconImage,
    window_height: u32,
) -> Result<Vec<Piece>> {
    let surface = gfx.image_surface(image)?;
    let count = (window_height / STRIP_ROWS).clamp(1, MAX_STRIPS);
    // A full-size image is cut on whole rows, so the first frame matches the
    // screen exactly; a reduced one on fractions, so every strip is stretched
    // by the same amount.
    let exact = image.height == window_height;
    let row = |index: u32| {
        let row = index as f32 * image.height as f32 / count as f32;
        if exact { row.floor() } else { row }
    };
    let compositor = &gfx.compositor;
    let children = root.Children()?;
    (0..count)
        .map(|index| {
            let brush = compositor.CreateSurfaceBrushWithSurface(&surface)?;
            brush.SetStretch(CompositionStretch::None)?;
            brush.SetHorizontalAlignmentRatio(0.0)?;
            brush.SetVerticalAlignmentRatio(0.0)?;
            let visual = compositor.CreateSpriteVisual()?;
            visual.SetBrush(&brush)?;
            visual.SetIsVisible(false)?;
            children.InsertAtTop(&visual)?;
            Ok(Piece {
                visual,
                brush,
                top: row(index),
                rows: row(index + 1) - row(index),
                shown: None,
            })
        })
        .collect()
}

fn blank_strips(count: usize) -> Vec<Strip> {
    vec![
        Strip {
            source_top: 0.0,
            source_bottom: 0.0,
            target: None,
        };
        count
    ]
}

fn local(rect: RECT, screen: RECT) -> Edges {
    Edges {
        left: (rect.left - screen.left) as f32,
        top: (rect.top - screen.top) as f32,
        right: (rect.right - screen.left) as f32,
        bottom: (rect.bottom - screen.top) as f32,
    }
}

impl Flight {
    /// Flies a window that is being minimized into the dock. `from` and `to`
    /// are screen rectangles; `screen` is the monitor the overlay covers.
    pub fn start(gfx: &Gfx, hwnd: isize, from: RECT, to: RECT, screen: RECT) -> Option<Self> {
        // Only what is on screen can be copied from it.
        let visible = intersect(from, screen);
        if is_empty(visible) {
            return None;
        }
        let (overlay, target, root) = create_overlay(gfx, screen)?;
        // SAFETY: `overlay` is the live window created above; it is destroyed
        // here on failure, or by `Drop`.
        let thumbnail = unsafe {
            let Ok(thumbnail) = DwmRegisterThumbnail(overlay, HWND(hwnd as *mut _)) else {
                let _ = DestroyWindow(overlay);
                return None;
            };
            // The stand-in: the whole window, exactly where it was.
            let properties = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_RECTDESTINATION
                    | DWM_TNP_VISIBLE
                    | DWM_TNP_OPACITY
                    | DWM_TNP_SOURCECLIENTAREAONLY,
                rcDestination: RECT {
                    left: from.left - screen.left,
                    top: from.top - screen.top,
                    right: from.right - screen.left,
                    bottom: from.bottom - screen.top,
                },
                opacity: 255,
                fVisible: true.into(),
                fSourceClientAreaOnly: false.into(),
                ..Default::default()
            };
            let _ = DwmUpdateThumbnailProperties(thumbnail, &properties);
            let _ = ShowWindow(overlay, SW_SHOWNOACTIVATE);
            thumbnail
        };
        Some(Self {
            hwnd,
            overlay,
            _target: target,
            root,
            origin: (screen.left, screen.top),
            genie: Genie {
                window: local(visible, screen),
                slot: local(to, screen),
            },
            phase: Phase::Standing {
                thumbnail,
                frames: 0,
            },
            strips: Vec::new(),
            source_width: (visible.right - visible.left) as f32,
            restoring: false,
            restore_pending: false,
            snapshot: None,
        })
    }

    /// Flies a minimized window out of its dock slot (`slot`, a screen
    /// rectangle) back to where `snapshot` was taken. The caller restores the
    /// real window when [`Flight::take_restore`] says so.
    pub fn restore(
        gfx: &Gfx,
        hwnd: isize,
        snapshot: &Snapshot,
        slot: RECT,
        screen: RECT,
    ) -> Option<Self> {
        let window = intersect(snapshot.window, screen);
        if is_empty(window) {
            return None;
        }
        let (overlay, target, root) = create_overlay(gfx, screen)?;
        let height = (window.bottom - window.top) as u32;
        let Ok(pieces) = cut(gfx, &root, &snapshot.image, height) else {
            // SAFETY: Our own window, destroyed once.
            unsafe {
                let _ = DestroyWindow(overlay);
            }
            return None;
        };
        let mut flight = Self {
            hwnd,
            overlay,
            _target: target,
            root,
            origin: (screen.left, screen.top),
            genie: Genie {
                window: local(window, screen),
                slot: local(slot, screen),
            },
            strips: blank_strips(pieces.len()),
            phase: Phase::Flying {
                start: Instant::now(),
                pieces,
            },
            source_width: snapshot.image.width as f32,
            restoring: true,
            restore_pending: true,
            snapshot: None,
        };
        let _ = flight.place(1.0);
        // SAFETY: Our own live window.
        unsafe {
            let _ = ShowWindow(flight.overlay, SW_SHOWNOACTIVATE);
        }
        Some(flight)
    }

    /// Advances the animation; returns `false` once it has finished (or cannot go on).
    pub fn step(&mut self, gfx: &Gfx, now: Instant) -> bool {
        match &mut self.phase {
            Phase::Standing { thumbnail, frames } => {
                *frames += 1;
                if *frames <= FRAMES_BEFORE_SNAPSHOT {
                    return true;
                }
                let thumbnail = *thumbnail;
                let Ok(pieces) = self.take_snapshot(gfx) else {
                    return false;
                };
                self.strips = blank_strips(pieces.len());
                self.phase = Phase::Flying { start: now, pieces };
                let _ = self.place(0.0);
                // The strips now show the same image: the stand-in can go.
                // SAFETY: `thumbnail` came from DwmRegisterThumbnail and is released once.
                unsafe {
                    let _ = DwmUnregisterThumbnail(thumbnail);
                }
                true
            }
            Phase::Flying { start, .. } => {
                let t = now.duration_since(*start).as_secs_f32() / DURATION_SECONDS;
                if !self.restoring {
                    return t < 1.0 && self.place(t).is_ok();
                }
                if t < 1.0 {
                    return self.place(1.0 - t).is_ok();
                }
                let arrived = self.place(0.0).is_ok();
                self.phase = Phase::Settling {
                    until: now + SETTLE,
                };
                arrived
            }
            Phase::Settling { until } => now < *until,
        }
    }

    /// `true`, once, when the real window has to be restored: the restore
    /// animation has reached full size, or has ended early.
    pub fn take_restore(&mut self, finished: bool) -> bool {
        let due = finished || matches!(self.phase, Phase::Settling { .. });
        due && std::mem::take(&mut self.restore_pending)
    }

    /// The snapshot of a window that has just been minimized, to restore it
    /// with the genie later.
    pub fn take_snapshot_for_restore(&mut self) -> Option<Snapshot> {
        self.snapshot.take()
    }

    /// Opacity (0..=255) of the dock's own thumbnail for this window: it
    /// fades in as the genie lands, and out as it takes off.
    pub fn landing_opacity(&self, now: Instant) -> u8 {
        let Phase::Flying { start, .. } = &self.phase else {
            return 0;
        };
        let elapsed = now.duration_since(*start).as_secs_f32() / DURATION_SECONDS;
        let t = if self.restoring {
            1.0 - elapsed
        } else {
            elapsed
        };
        (((t - LANDING_START) / (1.0 - LANDING_START)).clamp(0.0, 1.0) * 255.0) as u8
    }

    /// Copies the window's rectangle from the screen and cuts it into strips.
    fn take_snapshot(&mut self, gfx: &Gfx) -> Result<Vec<Piece>> {
        let edges = self.genie.window;
        let window = RECT {
            left: edges.left as i32 + self.origin.0,
            top: edges.top as i32 + self.origin.1,
            right: edges.right as i32 + self.origin.0,
            bottom: edges.bottom as i32 + self.origin.1,
        };
        let image =
            screen::capture(window).ok_or_else(|| anyhow::anyhow!("screen capture failed"))?;
        let pieces = cut(gfx, &self.root, &image, image.height)?;
        self.snapshot = Some(Snapshot {
            image: halved(&image),
            window,
        });
        Ok(pieces)
    }

    fn place(&mut self, t: f32) -> Result<()> {
        let Phase::Flying { pieces, .. } = &mut self.phase else {
            return Ok(());
        };
        let source_width = self.source_width;
        self.genie.frame(t, &mut self.strips);
        for (piece, strip) in pieces.iter_mut().zip(&self.strips) {
            // Whole pixels, so neighbouring strips meet without seams.
            let target = strip
                .target
                .map(|edges| {
                    [
                        edges.left.round() as i32,
                        edges.top.round() as i32,
                        edges.right.round() as i32,
                        edges.bottom.round() as i32,
                    ]
                })
                .filter(|[left, top, right, bottom]| right > left && bottom > top);
            if piece.shown == target {
                continue;
            }
            let Some([left, top, right, bottom]) = target else {
                piece.visual.SetIsVisible(false)?;
                piece.shown = None;
                continue;
            };
            if piece.shown.is_none() {
                piece.visual.SetIsVisible(true)?;
            }
            piece.shown = target;

            let (width, height) = ((right - left) as f32, (bottom - top) as f32);
            piece.visual.SetOffset(Vector3 {
                X: left as f32,
                Y: top as f32,
                Z: 0.0,
            })?;
            piece.visual.SetSize(Vector2 {
                X: width,
                Y: height,
            })?;
            // Squeeze the full snapshot width into the strip and bring this
            // strip's first row to the top of the visual.
            let (scale_x, scale_y) = (width / source_width, height / piece.rows);
            piece.brush.SetTransformMatrix(Matrix3x2 {
                M11: scale_x,
                M12: 0.0,
                M21: 0.0,
                M22: scale_y,
                M31: 0.0,
                M32: -piece.top * scale_y,
            })?;
        }
        Ok(())
    }
}

impl Drop for Flight {
    fn drop(&mut self) {
        // SAFETY: The handles were created in `start` or `restore` and are
        // released once.
        unsafe {
            if let Phase::Standing { thumbnail, .. } = self.phase {
                let _ = DwmUnregisterThumbnail(thumbnail);
            }
            let _ = DestroyWindow(self.overlay);
        }
    }
}
