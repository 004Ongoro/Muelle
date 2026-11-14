//! Genie minimize animation.
//!
//! Windows cannot deform another app's window, Windows 11 ignores the shell's
//! minimize-target hint, and a minimized window can no longer be captured. So:
//!
//! 1. one live DWM thumbnail is shown exactly where the window was (nothing
//!    changes on screen);
//! 2. a frame later that rectangle is copied from the screen: a snapshot, as
//!    macOS also uses for its genie;
//! 3. the snapshot becomes one composition surface shared by thin horizontal
//!    strips (plain composition visuals), placed every frame along the genie
//!    curves (`muelle_core::genie`).
//!
//! Many DWM thumbnails instead of a snapshot were measured and rejected: DWM
//! drops frames beyond about 16 of them (plan section 8).
//!
//! Everything lives on a transparent overlay window that exists only while
//! the animation runs. The system's own minimize animation must already be
//! off for the window (see `App::calm_windows`): it starts before the dock
//! hears about the minimize and cannot be cancelled afterwards.

use std::{sync::OnceLock, time::Instant};

use anyhow::Result;
use muelle_core::genie::{Edges, Genie, Strip};
use muelle_shell::screen;
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
/// Source rows per strip: the edge of the funnel steps by at most a few pixels.
const STRIP_ROWS: u32 = 3;
const MAX_STRIPS: u32 = 400;
/// Frames the thumbnail stays up before the screen is copied, so DWM has
/// certainly presented it.
const FRAMES_BEFORE_SNAPSHOT: u32 = 2;
/// The dock's own thumbnail fades in over the last part of the animation.
const LANDING_START: f32 = 0.8;

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
}

pub struct Flight {
    /// The window being minimized.
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

impl Flight {
    /// `from` and `to` are screen rectangles; `screen` is the monitor the
    /// overlay covers.
    pub fn start(gfx: &Gfx, hwnd: isize, from: RECT, to: RECT, screen: RECT) -> Option<Self> {
        // Only what is on screen can be copied from it.
        let visible = intersect(from, screen);
        if visible.right <= visible.left || visible.bottom <= visible.top {
            return None;
        }

        static CLASS: OnceLock<bool> = OnceLock::new();
        let class = w!("MuelleFlight");
        // SAFETY: Standard class registration and window creation on the UI
        // thread; the handles are released in `Drop` or on the early returns below.
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
            let (Ok((target, root)), Ok(thumbnail)) =
                (tree(), DwmRegisterThumbnail(overlay, HWND(hwnd as *mut _)))
            else {
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

            let local = |rect: RECT| Edges {
                left: (rect.left - screen.left) as f32,
                top: (rect.top - screen.top) as f32,
                right: (rect.right - screen.left) as f32,
                bottom: (rect.bottom - screen.top) as f32,
            };
            Some(Self {
                hwnd,
                overlay,
                _target: target,
                root,
                origin: (screen.left, screen.top),
                genie: Genie {
                    window: local(visible),
                    slot: local(to),
                },
                phase: Phase::Standing {
                    thumbnail,
                    frames: 0,
                },
                strips: Vec::new(),
            })
        }
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
                self.strips = vec![
                    Strip {
                        source_top: 0.0,
                        source_bottom: 0.0,
                        target: None,
                    };
                    pieces.len()
                ];
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
                t < 1.0 && self.place(t).is_ok()
            }
        }
    }

    /// Opacity (0..=255) of the dock's own thumbnail for this window, which
    /// fades in as the genie lands.
    pub fn landing_opacity(&self, now: Instant) -> u8 {
        let Phase::Flying { start, .. } = &self.phase else {
            return 0;
        };
        let t = now.duration_since(*start).as_secs_f32() / DURATION_SECONDS;
        (((t - LANDING_START) / (1.0 - LANDING_START)).clamp(0.0, 1.0) * 255.0) as u8
    }

    /// Copies the window's rectangle from the screen and cuts it into strips.
    fn take_snapshot(&self, gfx: &Gfx) -> Result<Vec<Piece>> {
        let window = self.genie.window;
        let image = screen::capture(RECT {
            left: window.left as i32 + self.origin.0,
            top: window.top as i32 + self.origin.1,
            right: window.right as i32 + self.origin.0,
            bottom: window.bottom as i32 + self.origin.1,
        })
        .ok_or_else(|| anyhow::anyhow!("screen capture failed"))?;
        let surface = gfx.image_surface(&image)?;

        let count = (image.height / STRIP_ROWS).clamp(1, MAX_STRIPS);
        let row =
            |index: u32| (u64::from(index) * u64::from(image.height) / u64::from(count)) as f32;
        let compositor = &gfx.compositor;
        let children = self.root.Children()?;
        (0..count)
            .map(|index| {
                let brush = compositor.CreateSurfaceBrushWithSurface(&surface)?;
                brush.SetStretch(CompositionStretch::None)?;
                brush.SetHorizontalAlignmentRatio(0.0)?;
                brush.SetVerticalAlignmentRatio(0.0)?;
                let visual = compositor.CreateSpriteVisual()?;
                visual.SetBrush(&brush)?;
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

    fn place(&mut self, t: f32) -> Result<()> {
        let Phase::Flying { pieces, .. } = &mut self.phase else {
            return Ok(());
        };
        let source_width = self.genie.window.right - self.genie.window.left;
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
            if piece.shown == target && piece.shown.is_some() {
                continue;
            }
            let Some([left, top, right, bottom]) = target else {
                if piece.shown.is_some() {
                    piece.visual.SetIsVisible(false)?;
                    piece.shown = None;
                }
                continue;
            };
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
        // SAFETY: The handles were created in `create` and are released once.
        unsafe {
            if let Phase::Standing { thumbnail, .. } = self.phase {
                let _ = DwmUnregisterThumbnail(thumbnail);
            }
            let _ = DestroyWindow(self.overlay);
        }
    }
}
