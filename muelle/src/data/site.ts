/** Links point at the public repository. `download` always serves the newest
 * published release (the release workflow attaches a stable Muelle-Setup.exe). */
export const site = {
  name: "Muelle",
  tagline: "The macOS Dock, made for Windows.",
  description:
    "Muelle replaces the Windows 11 taskbar with a macOS-style dock: icons that grow under the cursor, minimized windows as live thumbnails, an optional menu bar and Genie effect. Free and open source.",
  repository: "https://github.com/crafter-station/Muelle",
  releases: "https://github.com/crafter-station/Muelle/releases",
  download: "https://github.com/crafter-station/Muelle/releases/latest/download/Muelle-Setup.exe",
  issues: "https://github.com/crafter-station/Muelle/issues",
  version: "0.1",
  license: "MIT",
} as const;
