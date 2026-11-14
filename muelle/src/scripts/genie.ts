/**
 * Genie minimize effect: the window is drawn once into an offscreen canvas, sliced into
 * two-pixel rows, and each row is flown towards the dock along a smooth curve.
 */

import { gsap } from "@/lib/motion";

const SLICE = 2;
const MAX_DPR = 2;
const FLIGHT_SECONDS = 0.9;
const THUMB_SIZE = 52;
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
  const context = canvas?.getContext("2d");
  if (!canvas || !note || !thumb || !context) return;

  const dpr = Math.min(window.devicePixelRatio || 1, MAX_DPR);
  const state = { progress: 0 };
  let scene: Scene | null = null;

  const paint = (progress: number) => {
    if (!scene) return;

    const { x, y, width, height, targetX, targetY, targetWidth, source, slice } = scene;
    context.setTransform(dpr, 0, 0, dpr, 0, 0);
    context.clearRect(0, 0, canvas.width / dpr, canvas.height / dpr);
    note.style.visibility = progress > 0.02 ? "hidden" : "";

    if (progress <= 0.001 || progress >= 0.995) return;

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

  // The flight is measured from the document, not from where the window happens to sit on
  // load: with a tall viewport it is already above the start line, and the flight would
  // begin before anyone scrolls.
  let originTop = 0;
  let frame = 0;

  const measureOrigin = () => {
    originTop = note.getBoundingClientRect().top + window.scrollY;
  };

  const build = () => {
    const rootRect = root.getBoundingClientRect();
    const noteRect = note.getBoundingClientRect();
    const thumbRect = thumb.getBoundingClientRect();

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
    measureOrigin();
    paint(state.progress);
  };

  build();

  if (reduceMotion) return;

  const followScroll = () => {
    frame = 0;
    const start = Math.max(0, originTop - window.innerHeight * START_RATIO);
    const progress = clamp01((window.scrollY - start) / window.innerHeight);
    if (progress === state.progress) return;
    state.progress = progress;
    paint(progress);
  };

  window.addEventListener(
    "scroll",
    () => {
      if (!frame) frame = requestAnimationFrame(followScroll);
    },
    { passive: true },
  );

  root.querySelector<HTMLButtonElement>("[data-minimize]")?.addEventListener("click", () => {
    gsap.to(state, {
      progress: state.progress > 0.5 ? 0 : 1,
      duration: FLIGHT_SECONDS,
      ease: "power2.inOut",
      onUpdate: () => paint(state.progress),
    });
  });

  window.addEventListener("resize", build, { passive: true });
  // Fonts and icons change the layout the geometry was measured from.
  document.fonts.ready.then(build);
  window.addEventListener("load", build);
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

  const sidebar = width >= 440 ? 158 : 0;
  if (sidebar) drawSidebar(context, sidebar, height);

  const left = sidebar + 20;
  const center = (left + width - 20) / 2;

  drawTrafficLights(context, 18, 18);

  context.textAlign = "center";
  context.fillStyle = "#1d1d1f";
  context.font = '700 13px "Plus Jakarta Sans", system-ui, sans-serif';
  context.fillText("Notes", center, 14);

  context.fillStyle = "rgba(0, 0, 0, 0.38)";
  context.font = '500 11px "Plus Jakarta Sans", system-ui, sans-serif';
  context.fillText("Friday, November 14, 2026 at 9:41 AM", center, 36);

  context.textAlign = "left";
  context.fillStyle = "#1d1d1f";
  context.font = '800 26px "Plus Jakarta Sans", system-ui, sans-serif';
  context.fillText("Ship the Dock.", left, 74);

  const tasks = [
    ["Magnification, smooth curve", true],
    ["Genie minimize in 0.5 s", true],
    ["Taskbar back in under 2 s", true],
    ["0% CPU when idle", true],
    ["Publish v0.1", false],
  ] as const;

  tasks.forEach(([label, done], index) => {
    const y = 104 + index * 30;
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
  context.fillText("Folders", 12, 70);

  const folders = [
    ["All Notes", 12, false],
    ["Muelle", 5, true],
    ["Ideas", 4, false],
    ["Trash", 1, false],
  ] as const;

  folders.forEach(([name, count, active], index) => {
    const y = 94 + index * 30;

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
