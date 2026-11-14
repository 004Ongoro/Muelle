/**
 * Genie minimize effect: the window is drawn once into an offscreen canvas, sliced into
 * two-pixel rows, and each row is flown towards the dock along a smooth curve.
 */

import { gsap } from "@/lib/motion";

const SLICE = 1;
const MAX_DPR = 2;
const FLIGHT_SECONDS = 0.9;
const SCROLL_SCRUB_SECONDS = 0.6;
const THUMB_SIZE = 52;
/** Resting width of a dock item, needed to measure the slot without the magnification. */
const DOCK_REST = 52;
/** Rows near the bottom of the window start moving first. */
const STAGGER = 0.42;
const TRAVEL = 0.58;

const clamp01 = (value: number) => Math.min(1, Math.max(0, value));
/** Where the window must sit in the viewport for the flight to start. */
const START_RATIO = 0.55;
const smoothstep = (value: number) => value * value * (3 - 2 * value);

interface Scene {
  x: number;
  y: number;
  width: number;
  height: number;
  targetX: number;
  targetY: number;
  targetWidth: number;
  source: HTMLCanvasElement;
  slice: number;
}

export function initGenie(root: HTMLElement, reduceMotion: boolean) {
  const canvas = root.querySelector<HTMLCanvasElement>("[data-genie]");
  const note = root.querySelector<HTMLElement>("[data-note]");
  const thumb = document.querySelector<HTMLCanvasElement>("[data-dock-thumb] canvas");
  const slot = document.querySelector<HTMLElement>("[data-dock-thumb]");
  const restore = slot?.querySelector<HTMLButtonElement>("[data-dock-restore]");
  const context = canvas?.getContext("2d");
  if (!canvas || !note || !thumb || !slot || !context) return;

  let dpr = Math.min(window.devicePixelRatio || 1, MAX_DPR);
  const state = { progress: 0 };
  let scene: Scene | null = null;
  let scrollProgress = 0;
  let scrollTarget: number | null = null;
  let manualTarget: number | null = null;
  let manualDirection = 0;
  let manualFlight = false;
  /** Last value written to the dock slot, so the layout is only touched when it changes. */
  let slotValue = -1;
  let flyingBefore = false;
  let minimizedBefore = false;

  const paint = (progress: number) => {
    if (!scene) return;

    const { x, y, width, height, targetX, targetY, targetWidth, source, slice } = scene;
    context.setTransform(dpr, 0, 0, dpr, 0, 0);
    context.clearRect(0, 0, canvas.width / dpr, canvas.height / dpr);
    // Both hand over on the same threshold: for a few pixels of scroll the DOM window and
    // its canvas copy were painted at once, which read as a ghost of the text.
    const hidden = progress > 0.02;
    const minimized = progress >= 0.995;
    const flying = hidden && !minimized;
    canvas.classList.toggle("hidden", !flying);
    note.style.visibility = hidden ? "hidden" : "";

    // The dock's demo pauses while a window is in the air: a sweeping, magnified row
    // would move the slot away from the point the flight is aiming at.
    if (flying !== flyingBefore || minimized !== minimizedBefore) {
      flyingBefore = flying;
      minimizedBefore = minimized;
      window.dispatchEvent(new CustomEvent("muelle:genie", { detail: { flying, minimized } }));
    }

    // The dock opens its slot as the window arrives, so the thumbnail is not already
    // sitting there when nothing is minimized.
    const open = clamp01((progress - 0.72) / 0.28);
    if (open !== slotValue) {
      slotValue = open;
      slot.style.setProperty("--slot", String(open));
      slot.toggleAttribute("inert", open < 0.5);
      // Only the ends of the travel reflow the row; the dock asks for nothing per frame.
      if (open === 0 || open === 1) window.dispatchEvent(new CustomEvent("muelle:slot"));
    }

    if (!flying) return;

    const rows = Math.round(height / SLICE);
    const restCenter = x + width / 2;

    const rowAt = (position: number) => {
      const local = clamp01((progress - (1 - position) * STAGGER) / TRAVEL);
      const eased = smoothstep(local);
      const start = y + position * height;

      return {
        top: start + (targetY - start) * eased,
        width: width + (targetWidth - width) * eased,
        center: restCenter + (targetX - restCenter) * eased,
      };
    };

    for (let index = 0; index < rows; index += 1) {
      const from = rowAt(index / rows);
      const to = rowAt((index + 1) / rows);
      context.drawImage(
        source,
        0,
        index * slice,
        source.width,
        slice,
        from.center - from.width / 2,
        from.top,
        from.width,
        Math.max(1, to.top - from.top + 0.6),
      );
    }
  };

  let frame = 0;

  const build = () => {
    dpr = Math.min(window.devicePixelRatio || 1, MAX_DPR);
    const rootRect = root.getBoundingClientRect();
    const noteRect = note.getBoundingClientRect();

    // Aim at the slot at rest: the dock's autoplay magnifies its icons, which moves the
    // whole row, so measuring now would fly the window to a place the slot has left.
    // The slot is opened for the measurement too, because that is where it will land.
    const dockItems = [...root.querySelectorAll<HTMLElement>(".dock-item")];
    const savedSize = dockItems.map((item) => [item.style.width, item.style.height]);
    dockItems.forEach((item) => {
      item.style.width = `${DOCK_REST}px`;
      item.style.height = `${DOCK_REST}px`;
    });

    const previousSlot = slot.style.getPropertyValue("--slot");
    slot.style.setProperty("--slot", "1");
    const thumbRect = slot.getBoundingClientRect();
    if (previousSlot) slot.style.setProperty("--slot", previousSlot);
    else slot.style.removeProperty("--slot");

    dockItems.forEach((item, index) => {
      const [width, height] = savedSize[index];
      item.style.width = width;
      item.style.height = height;
    });

    canvas.width = Math.round(rootRect.width * dpr);
    canvas.height = Math.round(rootRect.height * dpr);

    const source = renderWindow(noteRect.width, noteRect.height, dpr);
    scene = {
      x: noteRect.left - rootRect.left,
      y: noteRect.top - rootRect.top,
      width: noteRect.width,
      height: noteRect.height,
      targetX: thumbRect.left - rootRect.left + thumbRect.width / 2,
      targetY: thumbRect.top - rootRect.top + thumbRect.height / 2,
      targetWidth: thumbRect.width,
      source,
      slice: SLICE * dpr,
    };

    renderThumbnail(thumb, source, dpr);
    paint(state.progress);
  };

  build();

  // The dock is wider than a phone and scrolls sideways, so the minimized slot sits off
  // screen: the flight would leave the mockup clipped at its right edge. Below 900 px the
  // window simply stays where it is.
  const wideEnough = window.matchMedia("(min-width: 900px)");
  if (reduceMotion || !wideEnough.matches) return;

  const readScrollProgress = () => {
    const top = note.getBoundingClientRect().top + window.scrollY;
    const start = Math.max(0, top - window.innerHeight * START_RATIO);
    return clamp01((window.scrollY - start) / window.innerHeight);
  };

  const syncScrollProgress = () => {
    const progress = readScrollProgress();
    scrollProgress = progress;

    // Let a manual flight finish, then let scroll take over once it catches the target.
    if (manualTarget !== null) {
      if (manualFlight || (progress - manualTarget) * manualDirection > 0) return;
      manualTarget = null;
    }

    if (progress === state.progress) {
      scrollTarget = progress;
      gsap.killTweensOf(state);
      return;
    }

    if (progress === scrollTarget) return;
    scrollTarget = progress;
    gsap.killTweensOf(state);
    gsap.to(state, {
      progress,
      duration: SCROLL_SCRUB_SECONDS,
      ease: "none",
      onUpdate: () => paint(state.progress),
    });
  };

  const followScroll = () => {
    frame = 0;
    syncScrollProgress();
  };

  window.addEventListener(
    "scroll",
    () => {
      if (!frame) frame = requestAnimationFrame(followScroll);
    },
    { passive: true },
  );

  const flyTo = (progress: number) => {
    scrollProgress = readScrollProgress();
    manualTarget = progress;
    manualDirection = Math.sign(scrollProgress - progress);
    scrollTarget = null;
    gsap.killTweensOf(state);
    manualFlight = true;
    gsap.to(state, {
      progress,
      duration: FLIGHT_SECONDS,
      ease: "power2.inOut",
      onUpdate: () => paint(state.progress),
      onComplete: () => {
        manualFlight = false;
        syncScrollProgress();
      },
    });
  };

  root.querySelector<HTMLButtonElement>("[data-minimize]")?.addEventListener("click", () => {
    flyTo(state.progress > 0.5 ? 0 : 1);
  });

  restore?.addEventListener("click", () => flyTo(0));

  const refresh = () => {
    build();
    syncScrollProgress();
  };
  window.addEventListener("resize", refresh, { passive: true });
  document.fonts.ready.then(refresh);
  window.addEventListener("load", refresh);
  syncScrollProgress();
}

/** Draws the window at rest, mirroring the DOM mock so the flight can start from it. */
function renderWindow(width: number, height: number, dpr: number) {
  const source = document.createElement("canvas");
  source.width = Math.round(width * dpr);
  source.height = Math.round(height * dpr);

  const context = source.getContext("2d");
  if (!context) return source;

  context.scale(dpr, dpr);
  context.textBaseline = "middle";

  context.save();
  context.beginPath();
  context.roundRect(0, 0, width, height, 14);
  context.clip();

  context.fillStyle = "rgba(255, 253, 247, 0.97)";
  context.fillRect(0, 0, width, height);

  // These offsets mirror the CSS in `NotesWindow.astro`; the two must line up or the
  // handover from the DOM window to this copy shows as a jump.
  const sidebar = width >= 440 ? 158 : 0;
  if (sidebar) drawSidebar(context, sidebar, height);

  const left = sidebar + 20;
  const center = (left + width - 20) / 2;

  drawTrafficLights(context, 22, 20);

  context.textAlign = "center";
  context.fillStyle = "#1d1d1f";
  context.font = '700 13px "Plus Jakarta Sans", system-ui, sans-serif';
  context.fillText("Notes", center, 22);

  context.fillStyle = "rgba(0, 0, 0, 0.38)";
  context.font = '500 11px "Plus Jakarta Sans", system-ui, sans-serif';
  context.fillText("Friday, November 14, 2026 at 9:41 AM", center, 44);

  context.textAlign = "left";
  context.fillStyle = "#1d1d1f";
  context.font = '800 26px "Plus Jakarta Sans", system-ui, sans-serif';
  context.fillText("Ship the Dock.", left, 81);

  const tasks = [
    ["Magnification, smooth curve", true],
    ["Live thumbnails when minimized", true],
    ["Taskbar always comes back", true],
    ["0% CPU when idle", true],
    ["Publish v0.1", false],
  ] as const;

  tasks.forEach(([label, done], index) => {
    const y = 117 + index * 29;
    drawCheck(context, left + 8, y, done);
    context.fillStyle = done ? "rgba(0, 0, 0, 0.4)" : "#1d1d1f";
    context.font = '500 14px "Plus Jakarta Sans", system-ui, sans-serif';
    context.fillText(label, left + 26, y);
  });

  context.restore();
  return source;
}

function drawSidebar(
  context: CanvasRenderingContext2D,
  width: number,
  height: number,
): void {
  context.strokeStyle = "rgba(0, 0, 0, 0.08)";
  context.lineWidth = 1;
  context.beginPath();
  context.moveTo(width, 0);
  context.lineTo(width, height);
  context.stroke();

  context.fillStyle = "rgba(0, 0, 0, 0.45)";
  context.font = '700 11px "Plus Jakarta Sans", system-ui, sans-serif';
  context.fillText("Folders", 12, 60);

  const folders = [
    ["All Notes", 12, false],
    ["Muelle", 5, true],
    ["Ideas", 4, false],
    ["Trash", 1, false],
  ] as const;

  folders.forEach(([name, count, active], index) => {
    const y = 90 + index * 29.7;

    if (active) {
      context.fillStyle = "#0a84ff";
      context.beginPath();
      context.roundRect(8, y - 13, width - 16, 26, 6);
      context.fill();
    }

    context.fillStyle = active ? "rgba(255, 255, 255, 0.9)" : "#f2b100";
    context.beginPath();
    context.roundRect(16, y - 6, 12, 10, 2);
    context.fill();

    context.fillStyle = active ? "#ffffff" : "#1d1d1f";
    context.font = '500 13px "Plus Jakarta Sans", system-ui, sans-serif';
    context.textAlign = "left";
    context.fillText(name, 34, y);

    context.fillStyle = active ? "rgba(255, 255, 255, 0.8)" : "rgba(0, 0, 0, 0.4)";
    context.font = '500 11px "Plus Jakarta Sans", system-ui, sans-serif';
    context.textAlign = "right";
    context.fillText(String(count), width - 16, y);
    context.textAlign = "left";
  });
}

function drawTrafficLights(context: CanvasRenderingContext2D, x: number, y: number): void {
  ["#ff5f57", "#febc2e", "#28c840"].forEach((color, index) => {
    context.beginPath();
    context.arc(x + index * 20, y, 6, 0, Math.PI * 2);
    context.fillStyle = color;
    context.fill();
    context.strokeStyle = "rgba(0, 0, 0, 0.16)";
    context.lineWidth = 0.6;
    context.stroke();
  });
}

function drawCheck(context: CanvasRenderingContext2D, x: number, y: number, done: boolean) {
  context.beginPath();
  context.arc(x, y, 8, 0, Math.PI * 2);

  if (!done) {
    context.strokeStyle = "rgba(0, 0, 0, 0.25)";
    context.lineWidth = 1.3;
    context.stroke();
    return;
  }

  context.fillStyle = "#ffb547";
  context.fill();
  context.strokeStyle = "#ffffff";
  context.lineWidth = 1.8;
  context.beginPath();
  context.moveTo(x - 3.6, y - 0.2);
  context.lineTo(x - 1, y + 2.4);
  context.lineTo(x + 4, y - 3);
  context.stroke();
}

/** The dock thumbnail is the same drawing, cropped square. */
function renderThumbnail(canvas: HTMLCanvasElement, source: HTMLCanvasElement, dpr: number) {
  const size = Math.round(THUMB_SIZE * dpr);
  canvas.width = size;
  canvas.height = size;

  const context = canvas.getContext("2d");
  if (!context) return;

  context.clearRect(0, 0, size, size);
  context.save();
  context.beginPath();
  context.roundRect(0, 0, size, size, 12 * dpr);
  context.clip();

  const scale = Math.max(size / source.width, size / source.height);
  const width = source.width * scale;
  const height = source.height * scale;
  context.drawImage(source, (size - width) / 2, (size - height) / 2, width, height);
  context.restore();
}
