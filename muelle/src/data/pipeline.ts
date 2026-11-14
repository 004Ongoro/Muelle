import type { ImageMetadata } from "astro";
import fileExplorer from "@/assets/icons/file-explorer.webp";
import microsoftStore from "@/assets/icons/microsoft-store.webp";
import settings from "@/assets/icons/settings.webp";
import windowsTerminal from "@/assets/icons/windows-terminal.webp";
import { figures } from "@/data/stats";

/** One box of the "How it works" diagram. */
export interface PipelineStep {
  label: string;
  title: string;
  detail: string;
}

export interface PipelineCore {
  name: string;
  memory: { value: string; unit: string };
  icons: ImageMetadata[];
  status: string;
}

export const howItWorks = {
  eyebrow: "How it works",
  heading: "One small process.",
  headingAccent: "Zero polling.",
} as const;

/** What wakes the dock up. */
export const inputs: PipelineStep[] = [
  { label: "Shell events", title: "Windows open and close", detail: "Notified, never polled" },
  { label: "Foreground", title: "Minimize, restore, fullscreen", detail: "Public Windows APIs only" },
  { label: "config.json", title: "Settings change", detail: "One message reloads it live" },
];

/** What the dock does in response. */
export const outputs: PipelineStep[] = [
  { label: "Dock", title: "Drawn by Windows composition", detail: "System blur, no GPU load" },
  { label: "Minimize", title: "Live thumbnail in the Dock", detail: "Optional Genie effect" },
  { label: "Taskbar", title: "Hidden, always restored", detail: "On quit, uninstall or crash" },
];

export const core: PipelineCore = {
  name: "muelle.exe",
  memory: figures.memory,
  icons: [fileExplorer, microsoftStore, windowsTerminal, settings],
  status: "idle · sleeping on the message queue",
};
