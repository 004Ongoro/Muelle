/** Figures measured on the product; keep them in sync with the dock's own plan. */
export const figures = {
  memory: { value: "16", unit: "MB", label: "private memory" },
  cpu: { value: "0", unit: "%", label: "cpu when idle" },
  genie: { value: "0.5", unit: "s", label: "genie minimize" },
  restore: { value: "2", unit: "s", label: "taskbar restore" },
} as const;

/** The stat strip, in the order the landing shows them. */
export const stats = Object.values(figures);
