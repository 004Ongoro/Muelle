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
//! 3. the strips cut from it go up, still, under the stand-in, which stays for
//!    two more frames: composition commits asynchronously while DWM drops a
//!    thumbnail on the very next frame, and removing it at once left a frame
//!    with neither, a visible flash;
//! 4. the snapshot is one composition surface shared by thin horizontal
//!    strips (plain composition visuals) that follow the genie curves
//!    (`muelle_core::genie`).
//!
//! The stand-in goes up before the dock even knows where the window will land
//! (`Flight::start`, then `Flight::aim`): the window has already vanished, and
//! every millisecond until the stand-in shows is a flash of the desktop.
//!
//! The dock does not move the strips itself. Each strip gets two expression
//! animations (offset and scale) that read two shared values, `bend` and
//! `shift`, and those two are keyframe animations: the system compositor runs
//! the whole flight on its own thread at the display's refresh rate. Placing
//! ~300 strips from the UI thread every frame was measured at 8-10 ms a frame
//! and made the compositor skip every other refresh.
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
use muelle_core::genie::{Edges, Genie};
use muelle_shell::{icons::IconImage, screen};
use windows::{
    Foundation::TimeSpan,
    UI::Composition::{
        CompositionPropertySet, CompositionStretch, ContainerVisual, Desktop::DesktopWindowTarget,
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
    core::{HSTRING, Interface, h, w},
};
use windows_numerics::{Matrix3x2, Vector2};

use crate::gfx::Gfx;

const DURATION_SECONDS: f32 = 0.5;
/// Screen rows per strip: the edge of the funnel steps by at most a few pixels.
const STRIP_ROWS: u32 = 3;
const MAX_STRIPS: u32 = 400;
/// Samples of the bend and slide curves handed to the compositor, which
/// interpolates between them at the display's refresh rate.
const KEYFRAMES: u32 = 30;
/// Extra source rows each strip draws below itself. Strips sit at fractional
/// positions while they fly; overlapping the next one (same pixels) leaves no
/// seam between their antialiased edges.
const OVERLAP_ROWS: f32 = 1.0;
/// Frames the thumbnail stays up before the screen is copied, so DWM has
/// certainly presented it.
const FRAMES_BEFORE_SNAPSHOT: u32 = 2;
/// Frames the still strips and the stand-in overlap before the stand-in goes.
const HANDOVER_FRAMES: u32 = 2;
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
    /// First source row and row count, in snapshot pixels.
    top: f32,
    rows: f32,
}

enum Phase {
    /// The stand-in thumbnail is on screen; counting frames until the snapshot.
    Standing { thumbnail: isize, frames: u32 },
    /// The strips are up and still under the stand-in, until they are
    /// certainly on screen.
    Handover {
        thumbnail: isize,
        frames: u32,
        drive: CompositionPropertySet,
    },
    Flying {
        start: Instant,
        /// Kept alive: the strips' animations read it.
        _drive: CompositionPropertySet,
    },
    /// A restore has reached full size and waits for the real window.
    Settling { until: Instant },
}

pub struct Flight {
    /// The window being minimized or restored.
    pub hwnd: isize,
    overlay: HWND,
    /// Kept alive: it owns the overlay's visual tree.
    _target: DesktopWindowTarget,
    root: ContainerVisual,
    /// The monitor the overlay covers.
    screen: RECT,
    genie: Genie,
    /// The dock slot is known (a minimize learns it a little after it starts).
    aimed: bool,
    phase: Phase,
    /// Out of the dock instead of into it.
    restoring: bool,
    /// The real window still has to be restored (restore flights only).
    restore_pending: bool,
    /// The full-size screen copy of a minimize, reduced only once the flight
    /// is over so that work stays out of the take-off.
    captured: Option<(IconImage, RECT)>,
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
            let (top, rows) = (row(index), row(index + 1) - row(index));
            let brush = compositor.CreateSurfaceBrushWithSurface(&surface)?;
            brush.SetStretch(CompositionStretch::None)?;
            brush.SetHorizontalAlignmentRatio(0.0)?;
            brush.SetVerticalAlignmentRatio(0.0)?;
            // Bring this strip's first row to the top of the visual.
            brush.SetTransformMatrix(Matrix3x2::translation(0.0, -top))?;
            // The visual has the strip's natural size; its animated scale
            // squeezes it into place.
            let visual = compositor.CreateSpriteVisual()?;
            visual.SetSize(Vector2 {
                X: image.width as f32,
                Y: rows + OVERLAP_ROWS,
            })?;
            visual.SetBrush(&brush)?;
            children.InsertAtTop(&visual)?;
            Ok(Piece { visual, top, rows })
        })
        .collect()
}

/// Hands the flight to the compositor: every strip follows `genie` for
/// `DURATION_SECONDS`, into the slot or (`reverse`) out of it.
///
/// The expressions are `Genie::frame` written for the compositor: a strip's
/// top is its resting row plus `shift`, its bottom stops at the slot, and its
/// sides follow the cosine funnel scaled by `bend`.
/// Ties every strip to two shared values, `bend` and `shift`, set to the
/// first frame of the flight (the window, or the slot when `reverse`); the
/// strips stay still until [`launch`] animates those values.
fn bind(
    gfx: &Gfx,
    genie: &Genie,
    pieces: &[Piece],
    image: (f32, f32),
    reverse: bool,
) -> Result<CompositionPropertySet> {
    let compositor = &gfx.compositor;
    let (window, slot) = (genie.window, genie.slot);
    let travel = (slot.bottom - window.top).max(0.0);
    let at = |progress: f32| if reverse { 1.0 - progress } else { progress };

    let drive = compositor.CreatePropertySet()?;
    drive.InsertScalar(h!("bend"), Genie::bend(at(0.0)))?;
    drive.InsertScalar(h!("shift"), Genie::slide(at(0.0)) * travel)?;

    // How far along the funnel the middle of the strip is, eased by a cosine.
    let curve = format!(
        "((1 - Cos(3.14159265 * Clamp(((y0 + Min(y1, {bottom:.3} - d.shift)) * 0.5 + d.shift - ({top:.3})) / {span:.3}, 0, 1))) * 0.5 * d.bend)",
        bottom = slot.bottom,
        top = window.top,
        span = (slot.top - window.top).max(1.0),
    );
    let offset = format!(
        "Vector3(({left:.3}) + ({shrink:.3}) * {curve}, y0 + d.shift, 0)",
        left = window.left,
        shrink = slot.left - window.left,
    );
    let scale = format!(
        "Vector3((({width:.3}) + ({narrow:.3}) * {curve}) / {source:.3}, Max(Min(y1, {bottom:.3} - d.shift) - y0, 0) / rows, 1)",
        width = window.right - window.left,
        narrow = (slot.right - slot.left) - (window.right - window.left),
        source = image.0,
        bottom = slot.bottom,
    );
    let offset = compositor.CreateExpressionAnimationWithExpression(&HSTRING::from(offset))?;
    let scale = compositor.CreateExpressionAnimationWithExpression(&HSTRING::from(scale))?;
    offset.SetReferenceParameter(h!("d"), &drive)?;
    scale.SetReferenceParameter(h!("d"), &drive)?;

    // Screen rows per source row.
    let stretch = (window.bottom - window.top) / image.1;
    for piece in pieces {
        let y0 = window.top + piece.top * stretch;
        let y1 = y0 + piece.rows * stretch;
        // Each start takes a copy of the animation with these values.
        offset.SetScalarParameter(h!("y0"), y0)?;
        offset.SetScalarParameter(h!("y1"), y1)?;
        scale.SetScalarParameter(h!("y0"), y0)?;
        scale.SetScalarParameter(h!("y1"), y1)?;
        scale.SetScalarParameter(h!("rows"), piece.rows)?;
        piece.visual.StartAnimation(h!("Offset"), &offset)?;
        piece.visual.StartAnimation(h!("Scale"), &scale)?;
    }
    Ok(drive)
}

/// Starts the flight bound by [`bind`]: the compositor animates `bend` and
/// `shift` for `DURATION_SECONDS` at the display's refresh rate.
fn launch(gfx: &Gfx, genie: &Genie, drive: &CompositionPropertySet, reverse: bool) -> Result<()> {
    let compositor = &gfx.compositor;
    let travel = (genie.slot.bottom - genie.window.top).max(0.0);
    let at = |progress: f32| if reverse { 1.0 - progress } else { progress };
    let linear = compositor.CreateLinearEasingFunction()?;
    let duration = TimeSpan {
        Duration: (DURATION_SECONDS * 1e7) as i64,
    };
    let animate = |name: &HSTRING, curve: fn(f32) -> f32, amount: f32| -> Result<()> {
        let animation = compositor.CreateScalarKeyFrameAnimation()?;
        animation.SetDuration(duration)?;
        for step in 0..=KEYFRAMES {
            let progress = step as f32 / KEYFRAMES as f32;
            animation.InsertKeyFrameWithEasingFunction(
                progress,
                curve(at(progress)) * amount,
                &linear,
            )?;
        }
        drive.StartAnimation(name, &animation)?;
        Ok(())
    };
    animate(h!("bend"), Genie::bend, 1.0)?;
    animate(h!("shift"), Genie::slide, travel)?;
    Ok(())
}

/// Nothing is drawn below the slot: strips vanish as they enter it.
fn clip_at_slot(gfx: &Gfx, root: &ContainerVisual, genie: &Genie, screen: RECT) -> Result<()> {
    let below = (screen.bottom - screen.top) as f32 - genie.slot.bottom;
    let clip = gfx
        .compositor
        .CreateInsetClipWithInsets(0.0, 0.0, 0.0, below.max(0.0))?;
    root.SetClip(&clip)?;
    Ok(())
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
    /// Puts up the stand-in of a window that is being minimized, where it was
    /// (`from`, a screen rectangle; `screen` is the monitor the overlay
    /// covers). The flight takes off once [`Flight::aim`] gives it its slot.
    pub fn start(gfx: &Gfx, hwnd: isize, from: RECT, screen: RECT) -> Option<Self> {
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
        let window = local(visible, screen);
        Some(Self {
            hwnd,
            overlay,
            _target: target,
            root,
            screen,
            // Until `aim`, the slot is the window itself.
            genie: Genie {
                window,
                slot: window,
            },
            aimed: false,
            phase: Phase::Standing {
                thumbnail,
                frames: 0,
            },
            restoring: false,
            restore_pending: false,
            captured: None,
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
        let genie = Genie {
            window: local(window, screen),
            slot: local(slot, screen),
        };
        let image = &snapshot.image;
        let size = (image.width as f32, image.height as f32);
        let drive = clip_at_slot(gfx, &root, &genie, screen)
            .and_then(|()| cut(gfx, &root, image, height))
            .and_then(|pieces| bind(gfx, &genie, &pieces, size, true))
            .and_then(|drive| launch(gfx, &genie, &drive, true).map(|()| drive));
        let Ok(drive) = drive else {
            // SAFETY: Our own window, destroyed once.
            unsafe {
                let _ = DestroyWindow(overlay);
            }
            return None;
        };
        let flight = Self {
            hwnd,
            overlay,
            _target: target,
            root,
            screen,
            genie,
            aimed: true,
            phase: Phase::Flying {
                start: Instant::now(),
                _drive: drive,
            },
            restoring: true,
            restore_pending: true,
            captured: None,
        };
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
                // No slot to fly to: the window just stays minimized.
                if !self.aimed {
                    return false;
                }
                let thumbnail = *thumbnail;
                let Ok(drive) = self.take_off(gfx) else {
                    return false;
                };
                self.phase = Phase::Handover {
                    thumbnail,
                    frames: 0,
                    drive,
                };
                true
            }
            Phase::Handover {
                thumbnail,
                frames,
                drive,
            } => {
                *frames += 1;
                if *frames <= HANDOVER_FRAMES {
                    return true;
                }
                let (thumbnail, drive) = (*thumbnail, drive.clone());
                if launch(gfx, &self.genie, &drive, false).is_err() {
                    return false;
                }
                // The strips have been on screen for a while, showing the same
                // image: the stand-in can go. The clock starts now, so copying
                // the screen does not eat the animation.
                // SAFETY: `thumbnail` came from DwmRegisterThumbnail and is released once.
                unsafe {
                    let _ = DwmUnregisterThumbnail(thumbnail);
                }
                self.phase = Phase::Flying {
                    start: Instant::now(),
                    _drive: drive,
                };
                true
            }
            // The compositor is flying the strips; only the end is ours.
            Phase::Flying { start, .. } => {
                let t = now.saturating_duration_since(*start).as_secs_f32() / DURATION_SECONDS;
                if t < 1.0 {
                    return true;
                }
                if !self.restoring {
                    return false;
                }
                self.phase = Phase::Settling {
                    until: now + SETTLE,
                };
                true
            }
            Phase::Settling { until } => now < *until,
        }
    }

    /// Gives a minimize its dock slot (a screen rectangle).
    pub fn aim(&mut self, gfx: &Gfx, slot: RECT) {
        self.genie.slot = local(slot, self.screen);
        self.aimed = true;
        // Without the clip the flight still works; strips would only show a
        // little below the slot.
        let _ = clip_at_slot(gfx, &self.root, &self.genie, self.screen);
    }

    #[must_use]
    pub fn needs_aim(&self) -> bool {
        !self.aimed
    }

    /// Width of the flying window divided by its height.
    #[must_use]
    pub fn aspect(&self) -> f32 {
        let window = self.genie.window;
        (window.right - window.left) / (window.bottom - window.top).max(1.0)
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
        self.captured.take().map(|(image, window)| Snapshot {
            image: halved(&image),
            window,
        })
    }

    /// Opacity (0..=255) of the dock's own thumbnail for this window: it
    /// fades in as the genie lands, and out as it takes off.
    pub fn landing_opacity(&self, now: Instant) -> u8 {
        let Phase::Flying { start, .. } = &self.phase else {
            return 0;
        };
        let elapsed = now.saturating_duration_since(*start).as_secs_f32() / DURATION_SECONDS;
        let t = if self.restoring {
            1.0 - elapsed
        } else {
            elapsed
        };
        (((t - LANDING_START) / (1.0 - LANDING_START)).clamp(0.0, 1.0) * 255.0) as u8
    }

    /// Copies the window's rectangle from the screen, cuts it into strips and
    /// ties them to the genie, still at the window's place.
    fn take_off(&mut self, gfx: &Gfx) -> Result<CompositionPropertySet> {
        let edges = self.genie.window;
        let window = RECT {
            left: edges.left as i32 + self.screen.left,
            top: edges.top as i32 + self.screen.top,
            right: edges.right as i32 + self.screen.left,
            bottom: edges.bottom as i32 + self.screen.top,
        };
        let image =
            screen::capture(window).ok_or_else(|| anyhow::anyhow!("screen capture failed"))?;
        let pieces = cut(gfx, &self.root, &image, image.height)?;
        let size = (image.width as f32, image.height as f32);
        let drive = bind(gfx, &self.genie, &pieces, size, false)?;
        self.captured = Some((image, window));
        Ok(drive)
    }
}

impl Drop for Flight {
    fn drop(&mut self) {
        // SAFETY: The handles were created in `start` or `restore` and are
        // released once.
        unsafe {
            if let Phase::Standing { thumbnail, .. } | Phase::Handover { thumbnail, .. } =
                self.phase
            {
                let _ = DwmUnregisterThumbnail(thumbnail);
            }
            let _ = DestroyWindow(self.overlay);
        }
    }
}
