import { useCallback, useEffect, useState } from "react";
import "./App.css";
import { api } from "./api";
import type { AppSettings, SyncStatus, Tab } from "./types";
import { Header } from "./components/Header";
import { TabBar } from "./components/TabBar";
import { InProgressTab } from "./components/InProgressTab";
import { RecentTab } from "./components/RecentTab";
import { StandupTab } from "./components/StandupTab";
import { SettingsPanel } from "./components/SettingsPanel";

function App() {
  const [tab, setTab] = useState<Tab>("in_progress");
  const [showSettings, setShowSettings] = useState(false);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [sync, setSync] = useState<SyncStatus | null>(null);
  const [refreshing, setRefreshing] = useState(false);
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
    // Poll sync status so freshness stays current while the popover is open.
    const t = setInterval(loadSync, 5000);
    return () => clearInterval(t);
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

  // Escape closes the popover (matches native menu bar feel).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (showSettings) setShowSettings(false);
        else api.hideWindow();
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
