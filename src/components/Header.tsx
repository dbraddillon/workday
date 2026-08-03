import type { SyncStatus } from "../types";
import { relativeTime } from "../util";

interface Props {
  sync: SyncStatus | null;
  refreshing: boolean;
  onRefresh: () => void;
  onToggleSettings: () => void;
  settingsOpen: boolean;
}

export function Header({ sync, refreshing, onRefresh, onToggleSettings, settingsOpen }: Props) {
  const stale = sync && !sync.ok;
  const last = sync?.last_success_at ?? sync?.last_run_at ?? null;

  return (
    <header className="header" data-tauri-drag-region>
      <div className="header-title" data-tauri-drag-region>
        <span className="app-name">Workday</span>
        <span className={`freshness ${stale ? "freshness-stale" : ""}`} title={sync?.message ?? ""}>
          {refreshing
            ? "syncing…"
            : last
              ? `updated ${relativeTime(last)}`
              : "not synced yet"}
          {stale ? " · sync failed" : ""}
        </span>
      </div>
      <div className="header-actions">
        <button
          className="icon-btn"
          onClick={onRefresh}
          disabled={refreshing}
          title="Refresh now"
          aria-label="Refresh now"
        >
          {refreshing ? "…" : "⟳"}
        </button>
        <button
          className={`icon-btn ${settingsOpen ? "active" : ""}`}
          onClick={onToggleSettings}
          title="Settings"
          aria-label="Settings"
        >
          ⚙
        </button>
      </div>
    </header>
  );
}
