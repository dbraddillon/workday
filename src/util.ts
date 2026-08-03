// Small formatting helpers shared across components.

import { openUrl } from "@tauri-apps/plugin-opener";

/** "3m ago", "2h ago", "yesterday", or a short date. */
export function relativeTime(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "";
  const diffMs = Date.now() - then;
  const min = Math.round(diffMs / 60000);
  if (min < 1) return "just now";
  if (min < 60) return `${min}m ago`;
  const hr = Math.round(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const days = Math.round(hr / 24);
  if (days === 1) return "yesterday";
  if (days < 7) return `${days}d ago`;
  return new Date(iso).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

/** Map Jira status category to a status-dot class. */
export function statusClass(category: string): string {
  switch (category) {
    case "done":
      return "dot-done";
    case "new":
      return "dot-new";
    default:
      return "dot-progress";
  }
}

/** Open a URL in the default browser (via the opener plugin). */
export function openExternal(url: string) {
  openUrl(url).catch(() => {});
}
