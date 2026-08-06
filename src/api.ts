// Typed wrappers around the Tauri command surface (src-tauri/src/commands.rs).
// One place for every `invoke`, so components never spell command names.

import { invoke } from "@tauri-apps/api/core";
import { writeHtml, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { draftToSlackHtml } from "./util";
import type {
  AppSettings,
  Issue,
  StandupDraft,
  StandupModel,
  SyncStatus,
} from "./types";

export const api = {
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) =>
    invoke<AppSettings>("save_settings", { settings }),
  setJiraToken: (token: string) => invoke<boolean>("set_jira_token", { token }),

  getInProgress: () => invoke<Issue[]>("get_in_progress"),
  getRecent: (range: string) => invoke<Issue[]>("get_recent", { range }),
  getSyncStatus: () => invoke<SyncStatus>("get_sync_status"),
  refreshNow: () => invoke<SyncStatus>("refresh_now"),

  buildStandupModel: (range: string) =>
    invoke<StandupModel>("build_standup_model", { range }),
  generateStandup: (
    model: StandupModel,
    formatterKey: string,
    aiPolish: boolean,
    styleHint?: string,
  ) =>
    invoke<StandupDraft>("generate_standup", {
      model,
      formatterKey,
      aiPolish,
      styleHint: styleHint ?? null,
    }),

  /** Whether a `claude` CLI is on PATH so AI polish can work. */
  aiPolishAvailable: () => invoke<boolean>("ai_polish_available"),

  recordDelivery: (method: string) =>
    invoke("record_delivery", { method }),

  getAutostart: () => invoke<boolean>("get_autostart"),
  setAutostart: (enabled: boolean) =>
    invoke<boolean>("set_autostart", { enabled }),

  hideWindow: () => invoke("hide_window"),
  uiReady: () => invoke("ui_ready"),
};

/**
 * Copy a standup draft to the clipboard and record the delivery outcome.
 *
 * Writes two flavors: `text/html` (so Slack's composer turns the `• ` runs into
 * its own native list block — matching a bullet typed by hand) and the original
 * plain text as the fallback for anywhere that doesn't take HTML. `writeHtml`
 * takes the plain-text alternative as its second argument, so this is one
 * clipboard write, not two competing ones.
 *
 * If the HTML flavor fails for any reason, fall back to plain text — a paste
 * that looks slightly off beats a Copy button that does nothing.
 */
export async function copyToClipboard(text: string): Promise<void> {
  try {
    await writeHtml(draftToSlackHtml(text), text);
  } catch {
    await writeText(text);
  }
  await api.recordDelivery("clipboard");
}
