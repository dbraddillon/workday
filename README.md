# Workday

A compact macOS **menu bar app** that gives you a quick glance at your Jira work
and generates a Slack standup draft from a time window. Local-first,
single-user, no backend — everything runs on your machine.

> **In short:** it's **macOS-only**, and all you need to connect is your **Jira
> account email + a Jira API token** you create yourself. No server, no admin
> access, no shared account. It runs entirely on your Mac and stores your token
> in the macOS Keychain. (An "AI polish" step is available but fully optional —
> the app works without it.)

<!-- screenshot placeholder -->

## Try it

```bash
git clone https://github.com/dbraddillon/workday.git
cd workday && npm install && npm run install-app
```

Needs **Node, Rust, and Xcode Command Line Tools** (one-liners in
[Prerequisites](#prerequisites)). It launches in **fake-data mode** so you can
poke around immediately — then open **Settings** (⚙), turn off *Use fake data*,
and add your own **Jira email + API token** (create one at
[id.atlassian.com](https://id.atlassian.com/manage-profile/security/api-tokens)).
Everything runs locally; nothing is shared. The rest of this README covers the
details.

## What it does

- **Menu bar popover** — click the tray icon (or press **⌘⇧J**) to see your work.
  It lives in the menu bar and stays running until you Quit. (A Dock icon appears
  only while the popover is open, so it can float over fullscreen apps.)
- **In Progress** — everything assigned to you that isn't Done.
- **Recent** — work touched in the last day / 3 days / 7 days.
- **Standup** — pick a window, choose which items to include, generate a Slack
  draft, edit it inline, and copy it. Includes a day-aware **"Since standup"**
  window (Monday reaches back to Friday; other days reach back to yesterday).
- **Standup thread format** — an optional format that mirrors a 5-prompt Slack
  standup thread (see [The standup format](#the-standup-format) below).
- **Optional AI polish** — one-click refinement of a draft via a local `claude`
  CLI, if you have one. Completely optional; the app works fully without it.
- **Local cache** (SQLite) with background refresh and a freshness indicator.
  Cached data still shows if a sync fails.

Your Jira API token lives in the **macOS Keychain** — never in the database or
the repo.

## Prerequisites

- **macOS** (this is a Mac-first menu bar app).
- **Node** 20+ and **npm**.
- **Rust** — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Xcode Command Line Tools** — `xcode-select --install`

## Install

The one-step path builds the app, ad-hoc-signs it (no Apple Developer account
needed), installs it to `/Applications`, and launches it:

```bash
npm install
npm run install-app
```

After it launches, look for the icon in your menu bar (top-right), or press
**⌘⇧J** to toggle the popover.

**Launch at login:** open the app → **Settings** (⚙) → toggle *Launch at login*.

**To update after pulling code changes:** run `npm run install-app` again. It
quits any running copy, rebuilds, re-signs, and reinstalls.

### Dev mode

To iterate on the code without installing, run the app straight from the repo:

```bash
npm run app          # tauri dev — hot-reloading dev build
```

You can also build a `.app` bundle without installing it:

```bash
npm run app:build    # release bundle → src-tauri/target/release/bundle/macos/
```

## First run: fake-data mode

On first launch the app is in **fake-data mode**: it serves sample issues and
activity, so all three tabs and the standup generator work with **zero config**
and no Jira access. Explore it, generate a standup, and try the tabs before
connecting anything.

When you're ready, open **Settings** (⚙) and turn off *Use fake data* to connect
your real Jira.

## Connecting Jira

1. Create an Atlassian API token at
   <https://id.atlassian.com/manage-profile/security/api-tokens>
   (Atlassian account → **Security** → **API tokens**). The app only reads, so a
   token scoped to your account is enough.
2. In the app: **Settings** (⚙) → uncheck *Use fake data* → fill in:
   - **Base URL** — `https://yourorg.atlassian.net`
   - **Email** — your Atlassian account email
   - **API token** — paste it (stored in the **macOS Keychain**, not the DB)
3. **Save**. The app syncs immediately, then on the *Refresh interval*.

Sync errors are non-fatal: the last-good cache still shows and the header marks
the failure. See [docs/SETUP.md](docs/SETUP.md) for JQL customization and more.

## The standup format

Pick a format under **Settings → Standup format**. Three are built in:

- **Standup thread reply (5 prompts)** — the default: mirrors a team standup
  thread's five emoji-prefixed prompts, one line each.
- **Grouped by status (bullets)** — sections grouped by status with
  Slack-friendly bullets.
- **Plain text** — key-and-summary only, no markdown.

The **thread** format renders the five prompts in order:

- `:city_sunrise:` — how you're doing (freeform; editable default in Settings)
- `:computer:` — what you're working on (derived from Jira)
- `:two-peas-in-a-pod:` — pairing opportunities (freeform; editable default)
- `:blocker:` — blockers (derived from Jira; falls back to `Nope`)
- `:high-five:` — anything for post scrum (freeform; editable default)

The `:computer:` line lists everything touched in the window — in-progress items
first, then recently-done ones — one flush-left bullet each (Slack strips leading
whitespace, so bullets keep items distinct). Each item can also get a **state
emoji** from its Jira status: `:pull_request:` (in review / PR / QA),
`:merged:`, `:deployparrot:` (deployed / released), or `:white_check_mark:`
(done). Example generated draft:

```
:city_sunrise: :batman: :thumbsup_all:
:computer:
• ABC-101 — Wire up the export pipeline
• ABC-98 — Fix flaky pagination test :white_check_mark:
• ABC-72 — Ship the settings migration :deployparrot:
:two-peas-in-a-pod: :available:
:blocker: Nope
:high-five: Nope
```

The **whole template is editable** in **Settings → Standup thread template** —
both the left-side prompt emoji (to match your team's exact thread) and the
default answers for the four freeform lines (doing / pairing / blockers /
post-scrum). You can also tweak those four per post on the Standup tab before
copying. Paste the output into your Slack thread.

> **About the emoji:** these are Slack emoji *shortcodes* (`:merged:`,
> `:deployparrot:`, etc.). If your Slack workspace doesn't have a given custom
> emoji, Slack simply shows the literal `:shortcode:` text — harmless, and you
> can swap any of them in Settings for emoji your workspace does have.

## Optional: AI polish

AI polish is **optional and OFF by default. The app works fully without it** —
the deterministic formatter above is always the baseline and never needs any
external tool.

If enabled, the **AI polish** toggle refines a generated draft by shelling out to
a locally installed `claude` CLI (`claude -p`) that must be on your `PATH`. If
the CLI isn't present or errors for any reason, the app **silently falls back**
to the deterministic draft — nothing breaks.

To enable it: **Settings → "AI polish standups by default"**. This assumes you
have a working `claude` CLI. How that CLI authenticates is entirely up to your
own setup — a normal Anthropic API key, or any other backend the CLI supports —
Workday doesn't care and doesn't manage credentials. (The author's machine
happens to use a Bedrock-backed CLI; that's just one way to set it up, not a
requirement.)

There's also an **on-demand escape hatch** that needs no toggle at all: every
time you generate a draft, the app writes a `standup-context.json` file to its
data directory. You can point any Claude instance at that file and ask it to
reformat or summarize, feeding in example posts. See
[docs/SETUP.md](docs/SETUP.md#on-demand-summarization-no-app-change) for details.

## Troubleshooting

- **Keychain "Always Allow" prompt after a rebuild.** Because each local build is
  ad-hoc signed with a fresh signature, macOS may re-prompt for Keychain access
  to your Jira token after you reinstall. Click **Always Allow** to stop the
  prompts for that build.
- **A Dock icon appears while the popover is open.** This is intentional: to
  bring the popover in front of a fullscreen/maximized app, the app briefly
  promotes itself to a regular (Dock-visible) app while the window is open, then
  drops back to menu-bar-only when you dismiss it. If it still opens behind
  something, click the tray icon again or press **⌘⇧J**.
- **Dev mode vs. installed app.** `npm run app` (dev) and the installed
  `/Applications/Workday.app` are separate copies and can each show their own
  tray icon. If you see two icons, one is the dev build — quit whichever you
  don't want from its tray menu.
- **First launch blocked by Gatekeeper.** `npm run install-app` strips the
  quarantine flag so this usually doesn't happen. If it does, right-click
  `Workday.app` in `/Applications` → **Open** → **Open** once.
- **Reset the local cache.** Delete the cache DB (see
  [docs/SETUP.md](docs/SETUP.md#where-data-lives)); the app rebuilds it on the
  next sync.

## Docs

- [docs/SETUP.md](docs/SETUP.md) — connecting Jira, JQL customization, the
  optional AI-polish setup, file locations, and the escape hatch.
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — layers, seams, and how to extend.
- [CLAUDE.md](CLAUDE.md) — working notes for Claude Code sessions.
- [docs/mac-menubar-jira-slack-handoff.md](docs/mac-menubar-jira-slack-handoff.md) — the original product brief.

## License

[MIT](LICENSE) — do what you like with it.
