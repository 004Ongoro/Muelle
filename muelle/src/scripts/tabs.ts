/**
 * Roving-tabindex tabs: one tab stop for the group, arrow keys to move between
 * tabs, and one panel visible at a time.
 */
export function initTabs(root: ParentNode) {
  root.querySelectorAll<HTMLElement>("[data-tabs]").forEach((group) => {
    const tabs = [...group.querySelectorAll<HTMLButtonElement>("[data-tab]")];
    const panels = [...group.querySelectorAll<HTMLElement>("[data-panel]")];
    if (!tabs.length || tabs.length !== panels.length) return;

    const select = (index: number, focus = false) => {
      tabs.forEach((tab, position) => {
        const active = position === index;
        tab.setAttribute("aria-selected", String(active));
        tab.tabIndex = active ? 0 : -1;
        panels[position].hidden = !active;
      });
      if (focus) tabs[index].focus();
    };

    const moves: Record<string, (index: number) => number> = {
      ArrowRight: (index) => (index + 1) % tabs.length,
      ArrowLeft: (index) => (index - 1 + tabs.length) % tabs.length,
      Home: () => 0,
      End: () => tabs.length - 1,
    };

    tabs.forEach((tab, index) => {
      tab.addEventListener("click", () => select(index));
      tab.addEventListener("keydown", (event) => {
        const move = moves[event.key];
        if (!move) return;
        event.preventDefault();
        select(move(index), true);
      });
    });
  });
}
