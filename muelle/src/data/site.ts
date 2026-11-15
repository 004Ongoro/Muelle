/** Links point at the public repository. `download` always serves the newest
 * published release (the release workflow attaches a stable Muelle-Setup.exe). */
export const site = {
  name: "Muelle",
  tagline: "The macOS Dock, made for Windows.",
  /** Kept under ~155 characters so Google does not truncate it. */
  description:
    "Free macOS-style Windows dock with AI usage limits for Claude Code, Codex, GitHub Copilot, Cursor, OpenCode Go, Gemini CLI and Antigravity.",
  /** Canonical origin. Must match `site` in astro.config.mjs and the sitemap. */
  url: "https://muellee.vercel.app",
  /** Path to the social card in `public/`. */
  ogImage: "/og-image.png",
  locale: "en_US",
  repository: "https://github.com/crafter-station/Muelle",
  releases: "https://github.com/crafter-station/Muelle/releases",
  download: "https://github.com/crafter-station/Muelle/releases/latest/download/Muelle-Setup.exe",
  issues: "https://github.com/crafter-station/Muelle/issues",
  version: "0.1.5",
  license: "MIT",
} as const;
