import { useEffect, useState } from "react";
import { api } from "../api";
import type { AppSettings } from "../types";

export function SettingsPanel({
  settings,
  onSaved,
}: {
  settings: AppSettings | null;
  onSaved: (s: AppSettings) => void;
}) {
  const [form, setForm] = useState<AppSettings | null>(settings);
  const [token, setToken] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [autostart, setAutostartState] = useState(false);

  useEffect(() => setForm(settings), [settings]);

  // Launch-at-login is an OS-level setting, read/written directly (not part of
  // the saved AppSettings blob).
  useEffect(() => {
    api.getAutostart().then(setAutostartState).catch(() => {});
  }, []);

  const toggleAutostart = async (enabled: boolean) => {
    try {
      setAutostartState(await api.setAutostart(enabled));
    } catch (e) {
      setError(String(e));
    }
  };

  if (!form) return <div className="loading">Loading settings…</div>;

  const set = <K extends keyof AppSettings>(k: K, v: AppSettings[K]) =>
    setForm({ ...form, [k]: v });

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      // Store token in Keychain if the user typed one.
      if (token.trim()) {
        await api.setJiraToken(token.trim());
        setToken("");
      }
      const saved = await api.saveSettings(form);
      onSaved(saved);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="settings">
      <section className="settings-section">
        <div className="settings-row toggle-row">
          <label>Use fake data (no Jira calls)</label>
          <input
            type="checkbox"
            checked={form.fake_data_mode}
            onChange={(e) => set("fake_data_mode", e.target.checked)}
          />
        </div>
        <p className="settings-hint">
          Great for trying the app before connecting Jira. Turn off to use your real Jira.
        </p>
      </section>

      <section className="settings-section" data-disabled={form.fake_data_mode}>
        <h3>Jira</h3>
        <label className="settings-field">
          Base URL
          <input
            type="text"
            placeholder="https://yourorg.atlassian.net"
            value={form.jira_base_url}
            onChange={(e) => set("jira_base_url", e.target.value)}
          />
        </label>
        <label className="settings-field">
          Email
          <input
            type="email"
            placeholder="you@company.com"
            value={form.jira_email}
            onChange={(e) => set("jira_email", e.target.value)}
          />
        </label>
        <label className="settings-field">
          API token {form.has_jira_token && <span className="badge">stored ✓</span>}
          <input
            type="password"
            placeholder={form.has_jira_token ? "•••••• (leave blank to keep)" : "paste token"}
            value={token}
            onChange={(e) => setToken(e.target.value)}
          />
        </label>
        <p className="settings-hint">
          Stored in your macOS Keychain, never in the app database. Create one at
          id.atlassian.com → Security → API tokens.
        </p>
        <label className="settings-field">
          In-progress JQL
          <input
            type="text"
            value={form.jira_jql_in_progress}
            onChange={(e) => set("jira_jql_in_progress", e.target.value)}
          />
        </label>
        <label className="settings-field">
          Recent JQL
          <input
            type="text"
            value={form.jira_jql_recent}
            onChange={(e) => set("jira_jql_recent", e.target.value)}
          />
        </label>
      </section>

      <section className="settings-section">
        <h3>Behavior</h3>
        <label className="settings-field">
          Refresh interval (seconds)
          <input
            type="number"
            min={30}
            value={form.refresh_interval_secs}
            onChange={(e) => set("refresh_interval_secs", Number(e.target.value) || 300)}
          />
        </label>
        <label className="settings-field">
          Default recent range
          <select
            value={form.default_recent_range}
            onChange={(e) => set("default_recent_range", e.target.value)}
          >
            <option value="today">Today</option>
            <option value="24h">Last 24 hours</option>
            <option value="3d">Last 3 days</option>
            <option value="7d">Last 7 days</option>
          </select>
        </label>
        <div className="settings-row toggle-row">
          <label title="Refine standup drafts with your local Claude CLI (Bedrock)">
            AI polish standups by default
          </label>
          <input
            type="checkbox"
            checked={form.ai_polish_enabled}
            onChange={(e) => set("ai_polish_enabled", e.target.checked)}
          />
        </div>
        <div className="settings-row toggle-row">
          <label title="Start Workday automatically when you log in">
            Launch at login
          </label>
          <input
            type="checkbox"
            checked={autostart}
            onChange={(e) => toggleAutostart(e.target.checked)}
          />
        </div>
      </section>

      {error && <div className="settings-error">{error}</div>}

      <div className="settings-actions">
        <button className="btn-primary" onClick={save} disabled={saving}>
          {saving ? "Saving…" : "Save"}
        </button>
      </div>
    </div>
  );
}
