/** Writes the pointer position each bento card needs for its spotlight. */
export function initSpotlight(root: ParentNode) {
  root.querySelectorAll<HTMLElement>("[data-spotlight]").forEach((card) => {
    card.addEventListener(
      "pointermove",
      (event) => {
        const rect = card.getBoundingClientRect();
        card.style.setProperty("--mx", `${event.clientX - rect.left}px`);
        card.style.setProperty("--my", `${event.clientY - rect.top}px`);
      },
      { passive: true },
    );
  });
}
