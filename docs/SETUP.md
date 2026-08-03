# Setup

## Prerequisites

- **macOS** (this is a Mac-first menu bar app).
- **Node** 20+ and **npm**.
- **Rust** — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Xcode Command Line Tools** — `xcode-select --install`

```bash
npm install
npm run app          # dev
npm run app:build    # release .app bundle → src-tauri/target/release/bundle/macos/
```

## Install as a real Mac app

To run Workday like any other app (Spotlight, Launchpad, click-to-run) instead
of `npm run`:

```bash
npm run app:build
# Ad-hoc sign so Gatekeeper is calmer (no Apple account needed):
codesign --force --deep --sign - "src-tauri/target/release/bundle/macos/Workday.app"
# Install:
cp -R "src-tauri/target/release/bundle/macos/Workday.app" /Applications/
open /Applications/Workday.app
```

It lives in the **menu bar** (no Dock icon), so after launch look for its icon
in the top-right, or press **⌘⇧J**. It stays running until you Quit from the
tray menu.

**Launch at login:** open the app → **Settings** (⚙) → toggle *Launch at login*.

**First launch (unsigned/ad-hoc):** if macOS blocks it, right-click
`Workday.app` → **Open** → **Open** once; thereafter it launches normally.

To update after code changes: rebuild, re-sign, and copy over the old copy in
`/Applications`.

## Fake-data mode

On first launch the app is in **fake-data mode**: it serves sample issues and
activity, so all three tabs and the standup generator work with zero config.
Turn it off in **Settings** (⚙) once you've connected Jira.

## Connecting Jira

1. Create an Atlassian API token: <https://id.atlassian.com/manage-profile/security/api-tokens>.
   A token tied to your account is enough — the app only reads.
2. In the app: **Settings** → uncheck *Use fake data* → fill in:
   - **Base URL** — `https://yourorg.atlassian.net`
   - **Email** — your Atlassian account email
   - **API token** — paste it (stored in the **macOS Keychain**, not the DB)
3. Optionally adjust the JQL:
   - *In-progress*: `assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC`
   - *Recent*: `assignee = currentUser() AND updated >= -7d ORDER BY updated DESC`
4. **Save**. The app syncs immediately, then every *Refresh interval* seconds.

Sync errors are non-fatal: the last-good cache still shows, and the header marks
the failure.

### Note on auth

v1 uses an email + API token (HTTP Basic) because it's the lowest-friction path
for a single user and the token is long-lived and effectively read-only. OAuth
2.0 (3LO) is the natural upgrade if this is ever shared with teammates — it
slots behind the same `JiraConnector` auth code without changing callers.

## AI polish for standups (optional)

The **AI polish** toggle refines a generated draft using the **local `claude`
CLI** already installed and authenticated on this machine. On this setup the CLI
is **Bedrock-backed** (`CLAUDE_CODE_USE_BEDROCK=1`, `AWS_PROFILE=ClaudeCode` in
`~/.claude/settings.json`), so it runs against Bedrock with your own AWS creds —
**no API key, no separate billing**. If the CLI isn't present or errors, the app
silently falls back to the deterministic draft.

### On-demand summarization (no app change)

Every time you generate a draft, the app also writes a
**`standup-context.json`** file to its data directory:

```
~/Library/Application Support/com.dbraddillon.workday/standup-context.json
```

It contains the normalized standup model, the current draft, and a `style_sample`
placeholder. You can point **any** running Claude at this file — e.g.
"read standup-context.json and rewrite it to match this example post" — and paste
in a real example. This is the seam for mirroring other people's post formats
before that logic is built into the app.

## Where data lives

- **Cache DB:** `~/Library/Application Support/com.dbraddillon.workday/workday.db`
- **Secrets:** macOS Keychain, service `com.dbraddillon.workday`.
- **Standup context:** `standup-context.json` in the same data dir (gitignored).

Delete the DB to reset the cache; the app rebuilds it on next sync.
