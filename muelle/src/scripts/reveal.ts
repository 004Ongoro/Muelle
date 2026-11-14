import { gsap, ScrollTrigger, prefersReducedMotion } from "@/lib/motion";

const DURATION = 0.9;
const STAGGER = 0.08;
/** Four steps is all the eye notices; a full twenty-item stagger feels slow. */
const STAGGER_STEPS = 4;

/**
 * Fades in the elements marked `data-r` once. Each element gets its own trigger:
 * a batch defers elements that are already on screen at load, which leaves them
 * invisible until the visitor scrolls.
 */
export function initReveal(root: HTMLElement) {
  if (root.dataset.revealed === "true") return;
  root.dataset.revealed = "true";

  const items = [...root.querySelectorAll<HTMLElement>("[data-r]")];
  if (!items.length || prefersReducedMotion()) return;

  gsap.set(items, { opacity: 0, y: 32 });

  items.forEach((item, index) => {
    ScrollTrigger.create({
      trigger: item,
      start: "top 92%",
      once: true,
      onEnter: () =>
        gsap.to(item, {
          opacity: 1,
          y: 0,
          duration: DURATION,
          delay: (index % STAGGER_STEPS) * STAGGER,
          ease: "power3.out",
        }),
    });
  });

  const pipe = root.querySelector<HTMLElement>(".pipe");
  if (!pipe) return;

  // The connectors read `--w` from the node, so they grow out of it.
  ScrollTrigger.create({
    trigger: pipe,
    start: "top 75%",
    once: true,
          onEnter: () => {
        // The connector's CSS default is already 1, so it has to be driven from 0 to grow.
        gsap.fromTo(
          pipe.querySelectorAll(".node"),
          { "--w": 0 },
          { "--w": 1, duration: 0.8, stagger: 0.1, ease: "power2.out" },
        );
      },
  });
}
