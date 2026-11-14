import { prefersReducedMotion } from "@/lib/motion";

/** Tilts `[data-tilt]` cards towards the pointer; the CSS reads `--tx` and `--ty`. */
export function initTilt(root: ParentNode) {
  if (prefersReducedMotion()) return;

  root.querySelectorAll<HTMLElement>("[data-tilt]").forEach((card) => {
    const setTilt = (x: number, y: number) => {
      card.style.setProperty("--tx", x.toFixed(3));
      card.style.setProperty("--ty", y.toFixed(3));
    };

    card.addEventListener(
      "pointermove",
      (event) => {
        const rect = card.getBoundingClientRect();
        setTilt((event.clientX - rect.left) / rect.width - 0.5, (event.clientY - rect.top) / rect.height - 0.5);
      },
      { passive: true },
    );

    card.addEventListener("pointerleave", () => setTilt(0, 0));
  });
}
