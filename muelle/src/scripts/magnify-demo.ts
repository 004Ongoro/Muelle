import { prefersReducedMotion } from "@/lib/motion";

/** How much the icon under the cursor grows, and how fast the effect falls off. */
const GROWTH = 0.5;
const FALLOFF = 5000;

/**
 * Drives the size slider and the cursor magnification of the icon row. Both
 * read the same size, so dragging the slider while hovering keeps one scale.
 */
export function initMagnifyDemo(root: ParentNode) {
  const row = root.querySelector<HTMLElement>("[data-magnify-row]");
  const range = root.querySelector<HTMLInputElement>("[data-magnify-range]");
  const output = root.querySelector<HTMLOutputElement>("[data-magnify-value]");
  if (!row || !range || !output) return;

  const icons = [...row.querySelectorAll<HTMLImageElement>("img")];
  const min = Number(range.min);
  const max = Number(range.max);
  let centers: number[] = [];

  const applySize = () => {
    const size = Number(range.value);
    row.style.setProperty("--s", `${size}px`);
    range.style.setProperty("--p", String((size - min) / (max - min)));
    output.textContent = `${size} px`;
  };

  range.addEventListener("input", applySize);
  applySize();

  if (prefersReducedMotion()) return;

  // The icons move when the slider resizes the row, so measure on every enter.
  row.addEventListener("pointerenter", () => {
    centers = icons.map((icon) => {
      const rect = icon.getBoundingClientRect();
      return rect.left + rect.width / 2;
    });
  });

  row.addEventListener("pointermove", (event) => {
    if (!centers.length) return;
    const size = Number(range.value);

    icons.forEach((icon, index) => {
      const distance = event.clientX - centers[index];
      const scale = size * (1 + GROWTH * Math.exp(-(distance * distance) / FALLOFF));
      icon.style.width = `${Math.round(scale)}px`;
      icon.style.height = `${Math.round(scale)}px`;
    });
  });

  row.addEventListener("pointerleave", () => {
    icons.forEach((icon) => {
      icon.style.width = "";
      icon.style.height = "";
    });
  });
}
