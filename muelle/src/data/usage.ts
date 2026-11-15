import claude from "@/assets/usage/claude.svg";
import codex from "@/assets/usage/codex.svg";

/** One plan limit the menu bar island shows, with example values. */
export interface UsageProvider {
  name: string;
  logo: ImageMetadata;
  /** Accent of its progress ring and bar. */
  color: string;
  /** Share of the five-hour window used, 0-100. */
  session: number;
  /** Share of the weekly window used, 0-100. */
  week: number;
  resets: string;
}

export const usageProviders: UsageProvider[] = [
  { name: "Claude", logo: claude, color: "#d97757", session: 63, week: 19, resets: "Resets 9:40 PM" },
  { name: "Codex", logo: codex, color: "#8ab4f8", session: 28, week: 41, resets: "Resets tomorrow 8:00 AM" },
];

export const usageIntro = {
  eyebrow: "AI usage",
  title: "Your AI limits, at a glance.",
  lead: "While a supported AI client is open, a curved island in the menu bar shows its available plan limits. Click for details and reset times.",
} as const;

export const usageFacts = [
  {
    title: "Only while you use them",
    body: "Claude, Codex, GitHub Copilot, Cursor, OpenCode Go, and Gemini or Antigravity appear while their clients are running.",
  },
  {
    title: "Your existing session",
    body: "It reuses the sign-in those tools already keep on your PC. No new Muelle account or token to paste.",
  },
  {
    title: "Straight to the provider",
    body: "Each provider is asked about its limits every five minutes or when you press Refresh; Antigravity uses its local service. A separate process keeps the dock responsive.",
  },
  {
    title: "One switch to turn it off",
    body: "Settings → Menu bar → Show AI provider usage. Muelle stores no credentials.",
  },
] as const;
