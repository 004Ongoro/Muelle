import { site } from "@/data/site";

/** One line of the terminal panel: a step, or a command that can be copied. */
export interface TerminalRow {
  text: string;
  command?: boolean;
}

export interface InstallTab {
  id: string;
  label: string;
  rows: TerminalRow[];
}

export const install = {
  eyebrow: "Install",
  heading: "One download.",
  headingAccent: "No wizard.",
} as const;

export const installTabs: InstallTab[] = [
  {
    id: "installer",
    label: "Installer",
    rows: [
      { text: "Download Muelle-Setup.exe (Windows 11, 64-bit)." },
      { text: "Double-click it. Per user, no admin rights, no wizard." },
      { text: "Not signed yet: if SmartScreen asks, choose More info → Run anyway." },
      { text: "The Dock appears, the taskbar tucks away, and Muelle starts with Windows." },
    ],
  },
  {
    id: "source",
    label: "From source",
    rows: [
      { text: `git clone ${site.repository}`, command: true },
      { text: "cd Muelle", command: true },
      { text: "cargo run --release -p muelle", command: true },
    ],
  },
  {
    id: "restore",
    label: "Get taskbar back",
    rows: [
      { text: "Right-click the Dock and choose Quit Muelle." },
      { text: "To remove it: Settings → Apps → Installed apps → Muelle → Uninstall." },
      { text: "Either way, the Windows taskbar comes back." },
    ],
  },
];

export const closing = {
  heading: "Dock it.",
  body: "Free and open source. Installs per user without admin rights, and uninstalling gives you your taskbar back.",
  cta: "Download for Windows",
} as const;
