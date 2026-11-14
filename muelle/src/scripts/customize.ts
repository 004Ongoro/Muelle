import { prefersReducedMotion } from "@/lib/motion";

/** The preview desk is smaller than a screen, so the dock is drawn at this share of its size. */
const PREVIEW_SCALE = 0.7;
/** Width of the magnified area around the pointer, in preview pixels. */
const SIGMA = 46;
/** How close to an edge the pointer must be to reveal an auto-hidden bar. */
const EDGE = 34;
const CHECK_MS = 1100;

type Strings = Record<string, string>;

/**
 * The Settings replica: switches, sliders, the language picker and the update check
 * all behave like the app, and the preview desk follows them as the dock would.
 */
export function initCustomize(root: HTMLElement) {
  const scene = root.querySelector<HTMLElement>("[data-customize]");
  const preview = root.querySelector<HTMLElement>("[data-preview]");
  const dock = root.querySelector<HTMLElement>("[data-preview-dock]");
  const bar = root.querySelector<HTMLElement>("[data-preview-bar]");
  const textNode = root.querySelector<HTMLScriptElement>("[data-settings-text]");
  if (!scene || !preview || !dock || !bar || !textNode) return;

  const texts = JSON.parse(textNode.textContent ?? "{}") as Record<"en" | "es", Strings>;
  let strings = texts.en;
  const icons = [...dock.querySelectorAll<HTMLImageElement>("img")];
  const ranges = new Map(
    [...scene.querySelectorAll<HTMLInputElement>("input[data-setting]")].map((input) => [
      input.dataset.setting ?? "",
      input,
    ]),
  );
  const switches = new Map(
    [...scene.querySelectorAll<HTMLButtonElement>("button[role=switch]")].map((button) => [
      button.dataset.setting ?? "",
      button,
    ]),
  );

  const isOn = (key: string) => switches.get(key)?.getAttribute("aria-checked") === "true";
  const value = (key: string) => Number(ranges.get(key)?.value ?? 0);

  const paintRange = (input: HTMLInputElement) => {
    const min = Number(input.min);
    const max = Number(input.max);
    input.style.setProperty("--p", String((Number(input.value) - min) / (max - min || 1)));
  };

  /* ---- Preview -------------------------------------------------------- */

  let pointerX: number | null = null;
  let nearBottom = false;
  let nearTop = false;

  const layoutDock = () => {
    const size = value("iconSize") * PREVIEW_SCALE;
    const peak = value("magnifiedSize") * PREVIEW_SCALE;
    const magnify = isOn("magnification") && pointerX !== null && !prefersReducedMotion();
    const centers = icons.map((_, index) => {
      // Centres of the resting row: the magnified widths are derived from these.
      const gap = 6;
      const rowWidth = icons.length * size + (icons.length - 1) * gap;
      const left = (dock.parentElement?.clientWidth ?? 0) / 2 - rowWidth / 2;
      return left + index * (size + gap) + size / 2;
    });
    icons.forEach((icon, index) => {
      let width = size;
      if (magnify && pointerX !== null) {
        const distance = pointerX - centers[index];
        width = size + (Math.max(peak, size) - size) * Math.exp(-(distance * distance) / (2 * SIGMA * SIGMA));
      }
      icon.style.width = `${width}px`;
      icon.style.height = `${width}px`;
    });
    dock.style.setProperty("--rest", `${size}px`);
  };

  const applyBars = () => {
    const dockHidden = isOn("autoHide") && !nearBottom;
    const barShown = isOn("menuBar") && (!isOn("menuBarAutoHide") || nearTop);
    dock.classList.toggle("is-hidden", dockHidden);
    bar.classList.toggle("is-hidden", !barShown);
    const autoRow = scene.querySelector<HTMLElement>("[data-row=menuBarAutoHide]");
    if (autoRow) autoRow.hidden = !isOn("menuBar");
  };

  preview.addEventListener("pointermove", (event) => {
    const rect = preview.getBoundingClientRect();
    const y = event.clientY - rect.top;
    nearBottom = rect.height - y < EDGE + (isOn("autoHide") ? 0 : 60);
    nearTop = y < EDGE;
    pointerX = nearBottom ? event.clientX - rect.left : null;
    applyBars();
    layoutDock();
  });
  preview.addEventListener("pointerleave", () => {
    pointerX = null;
    nearBottom = false;
    nearTop = false;
    applyBars();
    layoutDock();
  });

  /* ---- Controls ------------------------------------------------------- */

  const syncMagnified = () => {
    const magnified = ranges.get("magnifiedSize");
    if (!magnified) return;
    // As in the app: the magnified size never goes below the icon size.
    magnified.min = String(value("iconSize"));
    if (Number(magnified.value) < value("iconSize")) magnified.value = String(value("iconSize"));
    magnified.disabled = !isOn("magnification");
    paintRange(magnified);
  };

  ranges.forEach((input) => {
    paintRange(input);
    input.addEventListener("input", () => {
      paintRange(input);
      syncMagnified();
      layoutDock();
    });
  });

  switches.forEach((button) => {
    button.addEventListener("click", () => {
      button.setAttribute("aria-checked", String(!isOn(button.dataset.setting ?? "")));
      syncMagnified();
      applyBars();
      layoutDock();
    });
  });

  /* ---- Panes and language ---------------------------------------------- */

  const title = scene.querySelector<HTMLElement>("[data-pane-title]");
  let pane = "dock";
  const showPane = (next: string) => {
    pane = next;
    scene.querySelectorAll<HTMLElement>("[data-view]").forEach((view) => {
      view.hidden = view.dataset.view !== next;
    });
    scene.querySelectorAll<HTMLButtonElement>("[data-pane]").forEach((button) => {
      if (button.dataset.pane === next) button.setAttribute("aria-current", "page");
      else button.removeAttribute("aria-current");
    });
    if (title) title.textContent = strings[next] ?? next;
  };
  scene.querySelectorAll<HTMLButtonElement>("[data-pane]").forEach((button) => {
    button.addEventListener("click", () => showPane(button.dataset.pane ?? "dock"));
  });

  const status = scene.querySelector<HTMLElement>("[data-update-status]");
  const setLanguage = (choice: string) => {
    const resolved = choice === "auto" ? (navigator.language.toLowerCase().startsWith("es") ? "es" : "en") : choice;
    strings = texts[resolved === "es" ? "es" : "en"];
    scene.querySelectorAll<HTMLElement>("[data-t]").forEach((node) => {
      const text = strings[node.dataset.t ?? ""];
      if (text) node.textContent = text;
    });
    scene.querySelectorAll<HTMLElement>("[aria-label]").forEach((node) => {
      const key = node.dataset.setting;
      if (key && strings[key]) node.setAttribute("aria-label", strings[key]);
    });
    if (status) status.textContent = strings.updatesCurrent;
    scene.querySelectorAll<HTMLButtonElement>("[data-lang]").forEach((button) => {
      button.setAttribute("aria-checked", String(button.dataset.lang === choice));
    });
    showPane(pane);
  };
  scene.querySelectorAll<HTMLButtonElement>("[data-lang]").forEach((button) => {
    button.addEventListener("click", () => setLanguage(button.dataset.lang ?? "auto"));
  });

  const check = scene.querySelector<HTMLButtonElement>("[data-update]");
  check?.addEventListener("click", () => {
    if (!status) return;
    check.disabled = true;
    status.textContent = strings.updatesChecking;
    window.setTimeout(() => {
      status.textContent = strings.updatesCurrent;
      check.disabled = false;
    }, CHECK_MS);
  });

  syncMagnified();
  applyBars();
  layoutDock();
  window.addEventListener("resize", layoutDock, { passive: true });
}
