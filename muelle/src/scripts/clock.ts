// The landing is written in English, so the mock clock keeps that language even on a Spanish system.
const timeFormat = new Intl.DateTimeFormat("en-US", { hour: "numeric", minute: "2-digit" });
const dateFormat = new Intl.DateTimeFormat("en-US", { weekday: "short", day: "numeric", month: "short" });

/** Keeps the fake menu bar clock in step with the machine, aligned to the half minute. */
export function initClock() {
  const clocks = document.querySelectorAll<HTMLElement>("[data-clock]");
  const dates = document.querySelectorAll<HTMLElement>("[data-date]");
  if (!clocks.length && !dates.length) return;

  const update = () => {
    const now = new Date();
    clocks.forEach((node) => {
      node.textContent = timeFormat.format(now);
    });
    dates.forEach((node) => {
      node.textContent = dateFormat.format(now);
    });
  };

  update();
  window.setInterval(update, 30_000);
}
