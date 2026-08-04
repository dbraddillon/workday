# Setup

## Prerequisites

- **macOS** (this is a Mac-first menu bar app).
- **Node** 20+ and **npm**.
- **Rust** — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Xcode Command Line Tools** — `xcode-select --install`

## Install

The one-step path builds a release bundle, ad-hoc-signs it (no Apple Developer
account needed), installs it into `/Applications`, strips the quarantine flag,
and launches it:

```bash
npm install
npm run install-app
```

The app lives in the **menu bar** (a Dock icon shows only while the popover is
open). After launch, look for its icon
top-right, or press **⌘⇧J** to toggle the popover. It stays running until you
Quit from the tray menu.

**Launch at login:** open the app → **Settings** (⚙) → toggle *Launch at login*.

**To update after code changes:** run `npm run install-app` again — it quits the
running copy, rebuilds, re-signs, reinstalls, and relaunches.

### Dev mode and manual builds

```bash
npm run app          # tauri dev — hot-reloading dev build, run from the repo
npm run app:build    # release .app bundle → src-tauri/target/release/bundle/macos/
```

`npm run install-app` is `app:build` plus the sign/install/launch steps in
`scripts/install-app.sh`.

**First launch (unsigned/ad-hoc):** `npm run install-app` strips quarantine so
Gatekeeper usually stays quiet. If macOS still blocks it, right-click
`Workday.app` in `/Applications` → **Open** → **Open** once; thereafter it
launches normally.

## Fake-data mode

On first launch the app is in **fake-data mode**: it serves sample issues and
activity, so all three tabs and the standup generator work with zero config and
no Jira access. Turn it off in **Settings** (⚙) once you've connected Jira.

## Connecting Jira

1. Create an Atlassian API token:
   <https://id.atlassian.com/manage-profile/security/api-tokens>
   (Atlassian account → **Security** → **API tokens**). A token tied to your
   account is enough — the app only reads.
2. In the app: **Settings** → uncheck *Use fake data* → fill in:
   - **Base URL** — `https://yourorg.atlassian.net`
   - **Email** — your Atlassian account email
   - **API token** — paste it (stored in the **macOS Keychain**, not the DB)
3. Optionally adjust the JQL (see below).
4. **Save**. The app syncs immediately, then every *Refresh interval* seconds.

Sync errors are non-fatal: the last-good cache still shows, and the header marks
the failure.

### Customizing JQL

Two queries drive the tabs; both are editable in Settings. The defaults:

- **In-progress:** `assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC`
- **Recent:** `assignee = currentUser() AND updated >= -7d ORDER BY updated DESC`

Adjust these to your workflow — e.g. narrow to a project with
`AND project = ABC`, or widen the recent window. The `:computer:` "working on"
line in the standup and the two tabs are all built from these queries.

### Note on auth

v1 uses an email + API token (HTTP Basic) because it's the lowest-friction path
for a single user and the token is long-lived and effectively read-only. OAuth
2.0 (3LO) is the natural upgrade if this is ever shared more broadly — it slots
behind the same connector auth code without changing callers.

## The standup thread format

If you set **Settings → Standup format** to *Standup thread reply (5 prompts)*,
the generator renders five emoji-prefixed lines:

- `:city_sunrise:` — how you're doing (freeform)
- `:computer:` — what you're working on (derived from Jira: in-progress items
  first, then recently-done items marked `:white_check_mark:`, one flush-left
  bullet each)
- `:two-peas-in-a-pod:` — pairing opportunities (freeform)
- `:blocker:` — blockers (derived from Jira; falls back to `Nope`)
- `:high-five:` — anything for post scrum (freeform)

The three freeform lines have editable defaults under **Settings → Standup
thread defaults** (they only appear when the thread format is selected). You can
still edit any line per post before copying.

The **"Since standup"** window in the Standup tab is day-aware: on Monday it
reaches back to the previous Friday; on other days it reaches back to yesterday.

## AI polish for standups (optional)

AI polish is **optional and OFF by default. The app works fully without it** —
the deterministic formatter is always the baseline and never requires any
external tool or credential.

When enabled (**Settings → "AI polish standups by default"**), the app refines a
generated draft by shelling out to a locally installed `claude` CLI in headless
mode (`claude -p …`). Requirements and behavior:

- The `claude` CLI must be on your `PATH`. That's the only requirement Workday
  imposes.
- **How that CLI authenticates is entirely your own setup.** A normal Anthropic
  API key works; so does any other backend the CLI supports. Workday does not
  manage or store any credentials for it and does not care which backend you use.
- If the CLI is **absent or errors** for any reason (not installed, not logged
  in, times out), the app **silently falls back** to the deterministic draft.
  Nothing breaks; you just get the un-polished version.

> One optional configuration: the author's machine runs a Bedrock-backed
> `claude` CLI (via `CLAUDE_CODE_USE_BEDROCK=1` / an `AWS_PROFILE` in
> `~/.claude/settings.json`), so no API key is needed there. That is just one way
> to set the CLI up — it is **not** a requirement of this app. A teammate with no
> `claude` CLI, or one backed by an Anthropic API key, is equally fine.

### On-demand summarization (no app change)

Independent of the toggle above, every time you generate a draft the app writes a
**`standup-context.json`** file to its data directory:

```
~/Library/Application Support/com.dbraddillon.workday/standup-context.json
```

It contains the normalized standup model, the current draft, and a
`style_sample` placeholder. You can point **any** running Claude at this file —
e.g. "read standup-context.json and rewrite it to match this example post" — and
paste in a real example. This is the seam for mirroring other people's post
formats without any change to the app, and it needs no CLI configuration inside
Workday at all.

## Where data lives

- **Cache DB:** `~/Library/Application Support/com.dbraddillon.workday/workday.db`
- **Secrets:** macOS Keychain, service `com.dbraddillon.workday`.
- **Standup context:** `standup-context.json` in the same data dir (gitignored).

Delete the DB to reset the cache; the app rebuilds it on the next sync.
