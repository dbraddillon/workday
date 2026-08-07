import type { SyncStatus } from "../types";
import { relativeTime } from "../util";

interface Props {
  sync: SyncStatus | null;
  refreshing: boolean;
  onRefresh: () => void;
  onToggleSettings: () => void;
  settingsOpen: boolean;
  pinned: boolean;
  onTogglePin: () => void;
}

export function Header({
  sync,
  refreshing,
  onRefresh,
  onToggleSettings,
  settingsOpen,
  pinned,
  onTogglePin,
}: Props) {
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
          className={`icon-btn ${pinned ? "active" : ""}`}
          onClick={onTogglePin}
          title={pinned ? "Unpin (close when it loses focus)" : "Keep open while I work"}
          aria-label={pinned ? "Unpin popover" : "Pin popover open"}
          aria-pressed={pinned}
        >
          {pinned ? "📌" : "📍"}
        </button>
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
