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

const HTML_ESCAPES: Record<string, string> = {
  "&": "&amp;",
  "<": "&lt;",
  ">": "&gt;",
};

function escapeHtml(s: string): string {
  return s.replace(/[&<>]/g, (c) => HTML_ESCAPES[c]);
}

/**
 * Convert Slack mrkdwn emphasis to real tags. Slack interprets `*bold*` and
 * `_italic_` in a *plain-text* paste but not inside an HTML one, so the
 * markdown-using formatters (`default`, `plain`) would otherwise paste literal
 * asterisks. Run this on already-escaped text — `*`/`_` aren't escaped.
 *
 * Both delimiters require a start-or-whitespace boundary on the open side, which
 * is what keeps `:white_check_mark:` from reading as `white<i>check</i>mark`.
 */
function emphasize(escaped: string): string {
  return escaped
    .replace(/(^|\s)\*([^*\n]+)\*/g, "$1<b>$2</b>")
    .replace(/(^|\s)_([^_\n]+)_/g, "$1<i>$2</i>");
}

/**
 * Turn a rendered standup draft into HTML whose bullet lines are a real list.
 *
 * The formatters emit flush-left `• ` lines because Slack strips leading
 * whitespace from *plain text* pastes. But a bullet the user types in Slack's
 * composer becomes a native rich-text list item — different glyph, hanging
 * indent, tighter spacing — so a hand-added bullet never matches a pasted one.
 * Slack's composer does honour a `text/html` clipboard flavor, so we hand it
 * `<ul><li>` for the bullet runs and it builds its own list block; mixing in a
 * manual bullet then looks identical.
 *
 * The block structure below mirrors what Slack itself puts on the clipboard when
 * you copy a message containing a typed bullet — verified by decoding that
 * flavor. Namely: runs of non-bullet lines go in ONE `<div>` joined by `<br>`
 * (not a div per line, which Slack may read as separate blocks and space out),
 * and each bullet run is its own sibling `<ul>`. Emoji shortcodes are left as
 * literal `:name:` text; Slack's own copy renders them as `<img alt=":name:">`,
 * and it resolves the plain shortcode on paste.
 */
export function draftToSlackHtml(text: string): string {
  const blocks: string[] = [];
  let items: string[] = [];
  let lines: string[] = [];

  const flushLines = () => {
    if (lines.length === 0) return;
    // <br> between lines, matching Slack's own soft-break markup.
    blocks.push(`<div>${lines.join("<br>")}</div>`);
    lines = [];
  };

  const flushList = () => {
    if (items.length === 0) return;
    blocks.push(`<ul>${items.map((i) => `<li>${i}</li>`).join("")}</ul>`);
    items = [];
  };

  // Trim outer blank lines: the draft is editable in the textarea, and a stray
  // leading/trailing newline would paste as an empty line in Slack.
  for (const line of text.replace(/\r\n/g, "\n").replace(/^\n+|\n+$/g, "").split("\n")) {
    // Any bullet marker the formatters (or the user, editing the draft) might
    // use, with or without stray indentation.
    const bullet = line.match(/^\s*[•·*-]\s+(.*)$/);
    if (bullet) {
      flushLines();
      items.push(emphasize(escapeHtml(bullet[1])));
      continue;
    }
    flushList();
    // A blank line contributes an empty entry, so the <br> join alone spells the
    // gap ("A<br><br>B"). Pushing a literal <br> here would triple it.
    lines.push(line.trim() === "" ? "" : emphasize(escapeHtml(line)));
  }
  flushLines();
  flushList();

  return blocks.join("");
}
