# Workday — Claude working notes

macOS menu bar app: glance at current Jira work and generate a Slack standup
draft from a time window. **Local-first, single-user, no backend** (v1). Built
from `docs/mac-menubar-jira-slack-handoff.md` — read that for product intent.

## Stack

- **Tauri 2** shell (Rust) — tray icon + popover window, background polling.
- **React 19 + TypeScript + Vite** — the popover UI.
- **SQLite** (rusqlite, bundled) — local cache; migrations in `src-tauri/src/db/migrations.rs`.
- **macOS Keychain** (keyring crate) — secrets (Jira API token). Never SQLite, never repo.

## Run / build

```bash
npm install
npm run app          # tauri dev — launches the menu bar app (needs Rust: `rustup`)
npm run app:build    # release .app bundle
npx tsc --noEmit     # frontend type-check
(cd src-tauri && cargo check)   # backend check
```

First run starts in **fake-data mode** (no Jira needed) so all three tabs and
standup generation work immediately. Turn it off in Settings after adding Jira
creds. Toggle the popover with the tray icon or **⌘⇧J**.

## Architecture (layers = the doc's design; keep the seams)

```
src-tauri/src/
  lib.rs            Shell/host: tray, popover positioning, global shortcut, polling loop, command registry
  commands.rs       Tauri IPC surface (the only thing the React app calls)
  model.rs          Normalized domain types + format-agnostic StandupModel  ← the core seam
  config.rs         AppSettings (non-secret) + Keychain accessors (secret)
  db/               Local data layer: connection, migrations, repositories
  connector/        WorkSourceConnector trait; jira.rs (real) + fake.rs (dev). Normalizes to model.rs
  standup/          compose() → formatter (render) → summarizer (optional AI polish)
  delivery.rs       SlackDeliveryService seam (v1 = copy-to-clipboard)
  sync.rs           SyncCoordinator: pick connector, fetch, store, log sync_run
src/                React UI (api.ts is the typed invoke wrapper; components/ per tab)
```

**Hard design rule (from the doc):** the standup feature consumes *normalized*
data (`model.rs`), never raw Jira JSON. All Jira-shape parsing stays in
`connector/jira.rs`. Data gathering, summarization/selection, and string
rendering are separate — don't collapse them.

## The AI-summary decision (important context)

The standup "AI polish" reuses the **locally authenticated `claude` CLI**, not a
hosted API. On this machine the CLI is **Bedrock-backed** (`CLAUDE_CODE_USE_BEDROCK=1`,
`AWS_PROFILE=ClaudeCode` in `~/.claude/settings.json`), so `standup/summarizer.rs`
spawns `claude -p …` and it authenticates via the user's own AWS creds — no API
key, no separate billing. This mirrors the local `~/Repos/ClaudeChat` repo's
approach.

Two escape hatches, both intentional:
1. **In-app polish** — `ClaudeCliSummarizer` (opt-in toggle; off by default).
   Falls back to the deterministic draft on any error.
2. **On-demand, out-of-app** — every generate writes `standup-context.json` to
   the app data dir (the normalized model + draft + a style sample). You can
   point *any* running Claude at that file and ask it to reformat/summarize,
   feeding in example posts as they arrive — no app change needed.

The deterministic formatter (`standup/formatter.rs`) is always the baseline and
never needs the CLI.

## Conventions

- Keep the popover **compact and dense**; it's a utility, not a dashboard.
- Add a new formatter by implementing `StandupFormatter` and adding a key in
  `render_with`. Don't hardcode a single post style.
- Add a new source by implementing `WorkSourceConnector` and normalizing to
  `model.rs`. Selection happens in `sync.rs`.
- Errors from sync are **non-fatal**: recorded in `sync_runs`, cached data still
  shows, freshness/failure surfaced in the header.
- Secrets → Keychain only. If you add Slack tokens later, follow the Jira-token
  pattern in `config.rs`.

## Known v1 limitations (intentional; fix when they bite)

- **Jira Search API:** migrated to the cursor-based `/rest/api/3/search/jql`
  (`nextPageToken`/`isLast`); the classic `/rest/api/3/search` was removed by
  Atlassian (410 Gone, CHANGE-2046). Note `/search/jql` returns only issue `id`
  unless `fields` is passed — keep `FIELDS` populated in `connector/jira.rs`.
- **AI polish is silent-fallback:** if the `claude` CLI errors or times out
  (`DEFAULT_TIMEOUT`), `generate_standup` falls back to the deterministic draft
  without telling the UI. Acceptable for v1; surface the fallback if it confuses.
- **Poll loop** applies linear backoff on repeated sync failures but has no
  network-reachability check; it just keeps retrying at a longer interval.

## Debugging gotchas (learned the hard way)

- **`tauri dev` does not reliably deliver tray-icon clicks or global shortcuts**
  on this machine — the popover appears dead (icon shows, clicks/⌘⇧J do nothing,
  no error). The event loop / status-item wiring only behaves in a real signed
  `.app` bundle. **To test tray/window behavior, `npm run install-app` and test
  the installed app — not `npm run app`.** Dev mode is fine for UI/logic work.
- **Popover window must be `transparent: false`.** With `transparent: true` +
  `decorations: false` (the original config) the window renders fully invisible
  on some macOS setups (`is_visible()==true`, nothing painted). The body CSS also
  used `background: transparent` + `backdrop-filter` for a frosted look that only
  worked over a transparent window — now `body` uses `--bg-solid`. Keep the
  window opaque; borderless is fine.
- **Keychain re-prompts every dev rebuild.** `AppState::new()` reads the token
  from the Keychain once at startup; each `cargo` rebuild re-signs the dev binary,
  so macOS treats it as a new app and re-prompts even after "Always Allow". The
  installed (ad-hoc signed) bundle has a stable signature, so "Always Allow"
  sticks there. If debugging something unrelated to Jira in dev, temporarily stub
  the token read to `None` to stop the spam.
- Tray/window positioning is done in **physical** px, clamped to the monitor
  under the tray icon (`toggle_window`) — mixing physical `outer_size` with
  logical math threw the popover off-screen on multi-monitor + Retina.

## Not in v1 (preserve seams, don't build)

Multi-user, shared backend, direct Slack posting (stubbed in `delivery.rs`),
OAuth for Jira (API token only for now), extra sources, heavy AI orchestration.
