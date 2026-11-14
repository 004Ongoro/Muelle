import type { ImageMetadata } from "astro";
import downloads from "@/assets/icons/downloads.webp";
import fileExplorer from "@/assets/icons/file-explorer.webp";
import microsoftPhotos from "@/assets/icons/microsoft-photos.webp";
import microsoftStore from "@/assets/icons/microsoft-store.webp";
import recycleBin from "@/assets/icons/recycle-bin.webp";
import windowsTerminal from "@/assets/icons/windows-terminal.webp";
import { site } from "@/data/site";

/** One beat of the crash-and-recover timeline. */
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
  body: "A tiny guardian process watches the Dock. If it dies, your Windows taskbar comes back before you notice.",
  steps: [
    { time: "00:00", title: "Dock crashes", detail: "muelle.exe exits", state: "warn" },
    { time: "00:00.4", title: "Guardian wakes", detail: "--guard sees it" },
    { time: "00:01.8 // live", title: "Taskbar restored", detail: "Windows is whole again", state: "live" },
    { time: "00:03", title: "Dock relaunches", detail: "Autostart brings it back" },
  ] satisfies TimelineStep[],
};

export const material = {
  label: "Material",
  title: "Liquid glass.",
  body: "Live blur, specular edge, soft shadow. Tilt it with the cursor.",
  icons: [fileExplorer, microsoftPhotos, microsoftStore, windowsTerminal] as ImageMetadata[],
};

export const motion = {
  label: "Motion",
  title: "Follows your cursor.",
  body: "Hover the row. Smooth curve, no steps.",
  icons: [
    microsoftStore,
    fileExplorer,
    windowsTerminal,
    microsoftPhotos,
    downloads,
    recycleBin,
  ] as ImageMetadata[],
  range: { min: 28, max: 48, value: 40 },
};

export const menuBar = {
  label: "Menu bar",
  title: "Time, battery, control.",
  body: "A slim bar on top. Quick settings are one click away.",
  app: "Muelle",
  time: "9:41",
};

export const source = {
  label: "Source",
  title: "Open, in Rust.",
  body: "MIT licensed. Read it, fork it, ship it.",
  commands: [`git clone ${site.repository.replace(/^https?:\/\//, "")}`, "cargo run -p muelle"],
};
