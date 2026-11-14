import type { ImageMetadata } from "astro";
import downloads from "@/assets/icons/downloads.webp";
import fileExplorer from "@/assets/icons/file-explorer.webp";
import microsoftPhotos from "@/assets/icons/microsoft-photos.webp";
import microsoftStore from "@/assets/icons/microsoft-store.webp";
import recycleBin from "@/assets/icons/recycle-bin.webp";
import windowsTerminal from "@/assets/icons/windows-terminal.webp";
import { site } from "@/data/site";

/** One beat of the "your taskbar always comes back" timeline. */
export interface TimelineStep {
  time: string;
  title: string;
  detail: string;
  state?: "live" | "warn";
}

export const features = {
  eyebrow: "Features",
  heading: "Small details.",
  headingAccent: "Big difference.",
} as const;

export const safety = {
  label: "Safety",
  title: "Never stranded.",
  body: "Muelle hides the Windows taskbar, never removes it. Quit, uninstall or even a crash: the taskbar comes back.",
  steps: [
    { time: "Quit", title: "You quit Muelle", detail: "From the Dock menu" },
    { time: "Crash", title: "Dock stops", detail: "For any reason", state: "warn" },
    { time: "At once // live", title: "Taskbar is back", detail: "A tiny guardian sees to it", state: "live" },
    { time: "Uninstall", title: "Windows as it was", detail: "Nothing left behind" },
  ] satisfies TimelineStep[],
};

export const material = {
  label: "Material",
  title: "Liquid glass.",
  body: "Real system blur behind the glass, a light edge and a soft shadow. Tilt this one with your cursor.",
  icons: [fileExplorer, microsoftPhotos, microsoftStore, windowsTerminal] as ImageMetadata[],
};

export const motion = {
  label: "Motion",
  title: "Follows your cursor.",
  body: "Icons grow under the pointer on a smooth curve. Size and magnification are yours to set.",
  icons: [
    microsoftStore,
    fileExplorer,
    windowsTerminal,
    microsoftPhotos,
    downloads,
    recycleBin,
  ] as ImageMetadata[],
  range: { min: 32, max: 48, value: 40 },
};

export const menuBar = {
  label: "Menu bar",
  title: "Time, battery, control.",
  body: "Optional slim bar on top: the app in front, battery, Windows quick settings and notifications. It can hide itself too.",
  app: "Muelle",
  time: "9:41",
};

export const source = {
  label: "Source",
  title: "Open, in Rust.",
  body: "MIT licensed and written in Rust. Read it, build it, improve it.",
  // A bare `github.com/user/repo` is not a URL git understands, so the full https one is shown.
  commands: [`git clone ${site.repository}`, "cd Muelle && cargo run -p muelle"],
};
