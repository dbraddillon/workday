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
        <label className="settings-field">
          Standup format
          <select
            value={form.default_formatter}
            onChange={(e) => set("default_formatter", e.target.value)}
          >
            <option value="default">Grouped by status (bullets)</option>
            <option value="thread">Standup thread reply (5 prompts)</option>
            <option value="plain">Plain text</option>
          </select>
        </label>
        <div className="settings-row toggle-row">
          <label title="Optionally refine standup drafts with your local `claude` CLI, if one is installed. Works without it.">
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

      {form.default_formatter === "thread" && (
        <section className="settings-section">
          <h3>Standup thread template</h3>
          <p className="settings-hint">
            Match your team's standup thread. The left field is the prompt emoji
            (the subject Slack shows); the right is your default answer. "Working
            on" is always built from Jira; "Blockers" comes from Jira too, falling
            back to your default when there are none. Answers are editable per post.
          </p>

          <div className="thread-row">
            <input
              className="thread-emoji"
              type="text"
              aria-label="Doing prompt emoji"
              value={form.thread_prompt_doing}
              onChange={(e) => set("thread_prompt_doing", e.target.value)}
            />
            <input
              type="text"
              placeholder="How are you doing?"
              value={form.thread_doing}
              onChange={(e) => set("thread_doing", e.target.value)}
            />
          </div>

          <div className="thread-row">
            <input
              className="thread-emoji"
              type="text"
              aria-label="Working-on prompt emoji"
              value={form.thread_prompt_working}
              onChange={(e) => set("thread_prompt_working", e.target.value)}
            />
            <span className="thread-auto">— from Jira (in-progress + done)</span>
          </div>

          <div className="thread-row">
            <input
              className="thread-emoji"
              type="text"
              aria-label="Pairing prompt emoji"
              value={form.thread_prompt_pairing}
              onChange={(e) => set("thread_prompt_pairing", e.target.value)}
            />
            <input
              type="text"
              placeholder="Any pairing opportunities?"
              value={form.thread_pairing}
              onChange={(e) => set("thread_pairing", e.target.value)}
            />
          </div>

          <div className="thread-row">
            <input
              className="thread-emoji"
              type="text"
              aria-label="Blocker prompt emoji"
              value={form.thread_prompt_blocker}
              onChange={(e) => set("thread_prompt_blocker", e.target.value)}
            />
            <input
              type="text"
              placeholder="No-blockers fallback (e.g. Nope)"
              value={form.thread_blocker}
              onChange={(e) => set("thread_blocker", e.target.value)}
            />
          </div>

          <div className="thread-row">
            <input
              className="thread-emoji"
              type="text"
              aria-label="Post-scrum prompt emoji"
              value={form.thread_prompt_post_scrum}
              onChange={(e) => set("thread_prompt_post_scrum", e.target.value)}
            />
            <input
              type="text"
              placeholder="Anything for post scrum?"
              value={form.thread_post_scrum}
              onChange={(e) => set("thread_post_scrum", e.target.value)}
            />
          </div>
        </section>
      )}

      {error && <div className="settings-error">{error}</div>}

      <div className="settings-actions">
        <button className="btn-primary" onClick={save} disabled={saving}>
          {saving ? "Saving…" : "Save"}
        </button>
      </div>
    </div>
  );
}
