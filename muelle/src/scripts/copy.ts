/** How long the button shows its confirmation before going back to normal. */
const FEEDBACK_MS = 1400;

type CopyState = "idle" | "copied" | "failed";

/**
 * The Clipboard API only exists in a secure context (https or localhost), so a page
 * opened over plain http from another machine simply reports that it could not copy.
 */
async function writeClipboard(text: string): Promise<boolean> {
  if (!navigator.clipboard?.writeText) return false;

  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

/** Copies every `[data-copy]` command and confirms it in place. */
export function initCopy(root: ParentNode) {
  root.querySelectorAll<HTMLButtonElement>("[data-copy]").forEach((button) => {
    const value = button.dataset.copy ?? "";
    const copyIcon = button.querySelector(".icon-copy");
    const checkIcon = button.querySelector(".icon-check");
    const label = button.querySelector<HTMLElement>("[data-copy-label]");
    let resetTimer = 0;

    const setState = (state: CopyState) => {
      const message = state === "copied" ? "Copied" : state === "failed" ? "Copy failed" : "Copy command";
      button.dataset.copied = String(state === "copied");
      copyIcon?.classList.toggle("hidden", state === "copied");
      checkIcon?.classList.toggle("hidden", state !== "copied");
      button.setAttribute("aria-label", message);
      if (label) label.textContent = message;
    };

    button.addEventListener("click", async () => {
      const copied = await writeClipboard(value);
      setState(copied ? "copied" : "failed");

      // The failure message needs the same reset, or the button stays wrong for good.
      window.clearTimeout(resetTimer);
      resetTimer = window.setTimeout(() => setState("idle"), FEEDBACK_MS);
    });
  });
}
