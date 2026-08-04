// Typed wrappers around the Tauri command surface (src-tauri/src/commands.rs).
// One place for every `invoke`, so components never spell command names.

import { invoke } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
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

/** Copy text to the clipboard and record the delivery outcome. */
export async function copyToClipboard(text: string): Promise<void> {
  await writeText(text);
  await api.recordDelivery("clipboard");
}
