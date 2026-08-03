# Workday

A compact macOS **menu bar app** for a quick glance at your Jira work, and for
generating a Slack standup/status draft from a time window. Local-first,
single-user, no backend.

<!-- screenshot placeholder -->

## What it does (v1)

- **Menu bar popover** — click the tray icon (or press **⌘⇧J**) to see your work.
- **In Progress** — everything assigned to you that isn't Done.
- **Recent** — work touched in the last day / 3 days / 7 days.
- **Standup** — pick a window, toggle which items to include, generate a Slack
  draft, tweak it, and copy it. Optional one-click **AI polish** via your local
  `claude` CLI.
- **Local cache** (SQLite) with background refresh and a freshness indicator;
  cached data still shows if a sync fails.

Secrets (your Jira API token) live in the **macOS Keychain**, never in the
database or the repo.

## Quick start

Prereqs: **Node** and **Rust** (`curl https://sh.rustup.rs -sSf | sh`), plus
Xcode Command Line Tools.

```bash
npm install
npm run app        # launches the menu bar app in dev mode
```

The app starts in **fake-data mode**, so every tab and the standup generator
work immediately — no Jira needed. When you're ready, open **Settings** (⚙),
turn off fake data, and connect Jira (see [docs/SETUP.md](docs/SETUP.md)).

Build a distributable `.app`:

```bash
npm run app:build
```

## Docs

- [docs/SETUP.md](docs/SETUP.md) — connecting Jira, the AI-polish option, packaging.
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — layers, seams, and how to extend.
- [CLAUDE.md](CLAUDE.md) — working notes for Claude Code sessions.
- [docs/mac-menubar-jira-slack-handoff.md](docs/mac-menubar-jira-slack-handoff.md) — the original product brief.

## Status

v1 — a personal utility. Built to try, with clean internal seams (source
connector, standup formatter, delivery) so it could grow later without a
rewrite. Not a team product yet.
