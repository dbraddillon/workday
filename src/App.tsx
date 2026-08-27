import { useCallback, useEffect, useState } from "react";
import "./App.css";
import { api } from "./api";
import type { AppSettings, SyncStatus, Tab } from "./types";
import { Header } from "./components/Header";
import { TabBar } from "./components/TabBar";
import { InProgressTab } from "./components/InProgressTab";
import { RecentTab } from "./components/RecentTab";
import { ReviewsTab } from "./components/ReviewsTab";
import { StandupTab } from "./components/StandupTab";
import { SettingsPanel } from "./components/SettingsPanel";

function App() {
  const [tab, setTab] = useState<Tab>("in_progress");
  const [showSettings, setShowSettings] = useState(false);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [sync, setSync] = useState<SyncStatus | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  // Popover pinned open? Session state owned by the backend (see popover.rs), so
  // read it on mount — the popover can be reopened while still pinned.
  const [pinned, setPinned] = useState(false);
  // Bumped after a sync so tabs re-fetch.
  const [dataVersion, setDataVersion] = useState(0);

  const loadSettings = useCallback(async () => {
    setSettings(await api.getSettings());
  }, []);

  const loadSync = useCallback(async () => {
    setSync(await api.getSyncStatus());
  }, []);

  useEffect(() => {
    api.uiReady().catch(() => {});
    loadSettings();
    loadSync();
    api.getPinned().then(setPinned).catch(() => {});
    // Poll sync status so freshness stays current while the popover is open.
    const t = setInterval(loadSync, 5000);

    // The tray icon and ⌘⇧J also clear the pin when they dismiss the popover, and
    // those paths never reach this webview (it isn't remounted on hide/show). Re-
    // read on focus so the pin icon can't drift from the real backend state.
    const onFocus = () => {
      api.getPinned().then(setPinned).catch(() => {});
    };
    window.addEventListener("focus", onFocus);

    return () => {
      clearInterval(t);
      window.removeEventListener("focus", onFocus);
    };
  }, [loadSettings, loadSync]);

  const refresh = useCallback(async () => {
    setRefreshing(true);
    try {
      const s = await api.refreshNow();
      setSync(s);
      setDataVersion((v) => v + 1);
    } finally {
      setRefreshing(false);
    }
  }, []);

  // Only reflect the pin locally once the backend has it, so the icon never
  // claims a state the hide-on-blur handler isn't actually honoring.
  const togglePin = useCallback(async () => {
    const next = !pinned;
    try {
      setPinned(await api.setPinned(next));
    } catch {
      /* leave the toggle as-is; the popover still works, it just isn't pinned */
    }
  }, [pinned]);

  // Escape closes the popover (matches native menu bar feel).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (showSettings) setShowSettings(false);
        else {
          // `hide_window` also clears the pin (an explicit dismissal ends "keep
          // this open"). The webview isn't remounted on hide/show, so mirror that
          // locally or the pin icon would stay lit against a cleared backend.
          api.hideWindow();
          setPinned(false);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [showSettings]);

  return (
    <div className="app">
      <Header
        sync={sync}
        refreshing={refreshing}
        onRefresh={refresh}
        onToggleSettings={() => setShowSettings((s) => !s)}
        settingsOpen={showSettings}
        pinned={pinned}
        onTogglePin={togglePin}
      />

      {showSettings ? (
        <SettingsPanel
          settings={settings}
          onSaved={async (s) => {
            setSettings(s);
            setShowSettings(false);
            await refresh();
          }}
        />
      ) : (
        <>
          <TabBar tab={tab} onChange={setTab} />
          <main className="content">
            {tab === "in_progress" && <InProgressTab dataVersion={dataVersion} />}
            {tab === "recent" && (
              <RecentTab
                dataVersion={dataVersion}
                defaultRange={settings?.default_recent_range ?? "24h"}
              />
            )}
            {tab === "reviews" && (
              <ReviewsTab
                dataVersion={dataVersion}
                onOpenSettings={() => setShowSettings(true)}
              />
            )}
            {tab === "standup" && (
              <StandupTab
                dataVersion={dataVersion}
                defaultFormatter={settings?.default_formatter ?? "default"}
                aiPolishDefault={settings?.ai_polish_enabled ?? false}
              />
            )}
          </main>
        </>
      )}
    </div>
  );
}

export default App;
