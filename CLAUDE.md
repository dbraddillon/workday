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
                    github.rs is a second source, not a WorkSourceConnector (PRs aren't Issues)
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

**AI polish is fully optional and OFF by default — the app works with no LLM at
all.** The deterministic formatter (`standup/formatter.rs`) is always the
baseline. Everything below is an opt-in enhancement.

When enabled, "AI polish" reuses a **locally installed `claude` CLI** (spawns
`claude -p …` in `standup/summarizer.rs`), inheriting whatever auth the CLI is
configured with — an Anthropic API key, or a Bedrock/AWS setup, etc. It does not
require any specific backend. (The original author's machine happens to run a
Bedrock-backed CLI — `CLAUDE_CODE_USE_BEDROCK=1` / `AWS_PROFILE=…` — but that's
one configuration, not a requirement.)

Graceful degradation (matters for sharing):
- `summarizer::claude_cli_available()` preflights `claude --version`; the
  `ai_polish_available` command gates the UI toggle so a teammate with no CLI is
  never offered an option that would silently no-op.
- If polish is requested but the CLI is missing/errors, `generate_standup`
  returns the deterministic draft AND sets `StandupDraft.ai_polish_fell_back`, so
  the UI shows a note instead of failing silently.

Two escape hatches, both intentional:
1. **In-app polish** — `ClaudeCliSummarizer` (opt-in toggle; hidden if no CLI).
2. **On-demand, out-of-app** — every generate writes `standup-context.json` to
   the app data dir (the normalized model + draft + a style sample). Point *any*
   running Claude at that file and ask it to reformat/summarize — no app change,
   no in-app LLM needed.

## Conventions

- Keep the popover **compact and dense**; it's a utility, not a dashboard.
- Add a new formatter by implementing `StandupFormatter` and adding a key in
  `render_with`. Don't hardcode a single post style. The default is `thread`.
- The **thread template is fully user-configurable** (a team's standup thread):
  all 5 prompt emoji + the doing/pairing/blocker/post-scrum answer defaults live
  in `AppSettings` (`thread_prompt_*`, `thread_*`) and ride to the formatter via
  `StandupNarrative` — the formatter stays format-agnostic (consumes the model,
  not settings). The working-on line is always Jira-derived; blockers derive from
  Jira and fall back to `thread_blocker`.
- Work items get a **state emoji** from their Jira status via
  `formatter::state_marker` (keyword match: deploy/released → `:deployparrot:`,
  merged → `:merged:`, review/PR/QA → `:pull_request:`, else done →
  `:white_check_mark:`). Status *names* vary per instance — tune the keywords there.
- Standup lines are **flush-left with `•` bullets**, never space-indented: Slack
  strips leading whitespace, which mashed multi-item lines together.
- **Copy writes two clipboard flavors** (`api.ts` → `copyToClipboard`): a
  `text/html` rendering from `util.ts` → `draftToSlackHtml`, with the formatter's
  plain text as the fallback flavor. Slack's composer converts a `<ul>` into a
  *native* list block, so a bullet the user then types by hand matches the pasted
  ones — pasting literal `•` text gives a different glyph and looser spacing.
  Formatters stay plain-text-only; the HTML is a delivery-time concern.
  - The block structure mirrors **what Slack itself puts on the clipboard**, found
    by copying a message with a typed bullet and decoding the HTML flavor
    (`osascript -e 'the clipboard as «class HTML»'` → hex → `xxd -r -p`). Slack
    emits runs of non-bullet lines as ONE `<div>` joined by `<br>`, each bullet
    run as a sibling `<ul>`. Use that same shape — a `<div>` per line risks being
    read as separate blocks with extra spacing. Re-run that decode if paste
    fidelity ever regresses.
  - `draftToSlackHtml` also converts mrkdwn `*bold*`/`_italic_` to `<b>`/`<i>`:
    Slack honors those in a plain-text paste but NOT inside an HTML one, so the
    `default`/`plain` formatters would otherwise paste literal asterisks. The
    whitespace boundary on the opening delimiter is what stops
    `:white_check_mark:` becoming `white<i>check</i>mark`.
- Add a new source by implementing `WorkSourceConnector` and normalizing to
  `model.rs`. Selection happens in `sync.rs`. A source that isn't issue-shaped
  (GitHub PRs) skips the trait but keeps the rule: raw shapes stay in the
  connector, `model.rs` is the boundary.
- **A source with its own failure mode syncs outside the Jira result.** In
  `run_sync`, GitHub errors are dropped rather than folded in: a broken `gh` must
  not mark the Jira sync failed, and a Jira outage must not blank the Reviews tab.
- Errors from sync are **non-fatal**: recorded in `sync_runs`, cached data still
  shows, freshness/failure surfaced in the header.
- Secrets → Keychain only. If you add Slack tokens later, follow the Jira-token
  pattern in `config.rs`.

## Reviews tab (GitHub PR queue)

Second source, off by default (`github_enabled`). Answers "what's waiting on my
team" when a PR was tagged to the team and never posted in Slack, and feeds the
standup's PR-reviews line.

- **No new secret.** `connector/github.rs` shells the local `gh` CLI, which owns
  the credential - same reuse pattern as the `claude` CLI in `summarizer.rs`.
  Nothing GitHub-related goes in the Keychain. `gh_available()` (runs `gh auth
  status`) gates the tab, mirroring `ai_polish_available`.
- **GraphQL, not REST search.** `gh api graphql`. REST `search/issues` is capped
  at 30 req/min; a GraphQL search costs 1 point against 5000/hour, which is what
  makes poll-loop refresh affordable. One request carries all the searches as
  aliases; each row is tagged with its alias, which becomes `reasons`.
- **Two independent queries, two tables.** They answer different questions and
  neither substitutes for the other:
  - `fetch_review_queue` → `pull_requests` (full replace per sync, so a merged PR
    disappears). Open, non-draft, not approved, within `github_window_days`,
    union of team-requested and teammate-authored. Trumped by
    `user-review-requested:` / `assignee:` on the user, which ignore both the
    window and the approval filter and sort to the top (`is_direct`).
  - `fetch_submitted_reviews` → `submitted_reviews` (upsert; each fetch is a
    bounded window and a replace would drop older rows). **~97% of reviewed PRs
    merge within a day, so a completed review is essentially never findable in the
    open queue** - without this query the tab shows none of the work you did.
- **`reviewed-by:` cannot be date-filtered on the review.** GitHub search only
  offers the PR's `updated:`, which counts old reviews on recently-touched PRs
  (measured: 26 vs the true 27-on-one-day figure, wrong rows entirely). The fix in
  `fetch_submitted_reviews`: widen the *search* window, then filter precisely on
  each review's own `submittedAt` from `reviews(author:)`. Don't "simplify" that
  back to a single search qualifier.
- **Copilot is excluded from `human_reviewers`** (`BOT_REVIEWER_MARKERS`). Bot
  reviews inflate `latestReviews` and make an unreviewed PR look attended-to,
  which defeats the "no reviewers" flag the tab exists to show.
- **Standup credit is a UNION, counted distinct per PR.**
  `repo::review_credit_count_in_range` unions submitted reviews with manual
  checkoffs: three passes over one PR is one review, and a PR both reviewed and
  ticked off counts once. Checkoffs live in their own table on purpose - a
  checkoff has to outlive the PR leaving the queue.
- The thread line renders **presence, not a count** (`:pull_request: PR reviews`).
  `StandupModel.reviewed_pr_count` still carries the number for other formatters.
- Author rosters are chunked at `AUTHORS_PER_QUERY` (40): GitHub search queries
  cap around 1000 chars.

## Known v1 limitations (intentional; fix when they bite)

- **Jira Search API:** migrated to the cursor-based `/rest/api/3/search/jql`
  (`nextPageToken`/`isLast`); the classic `/rest/api/3/search` was removed by
  Atlassian (410 Gone, CHANGE-2046). Note `/search/jql` returns only issue `id`
  unless `fields` is passed — keep `FIELDS` populated in `connector/jira.rs`.
- **Poll loop** applies linear backoff on repeated sync failures but has no
  network-reachability check; it just keeps retrying at a longer interval.
- **`tauri-nspanel` is a pinned git dependency** (no crates.io release). It's
  pinned to an exact `rev` in `src-tauri/Cargo.toml` — never relax that to a bare
  `branch`, which would silently re-resolve unsafe objc interop under us. See
  "Popover = NSPanel" below.

## Popover = NSPanel (don't regress this)

The popover is an **`NSPanel`**, not a plain window — that's what lets it appear
over fullscreen/maximized apps *without a Dock icon*. It's the same mechanism
native menu bar apps (JetBrains Toolbox, Spotlight) use; no special permissions
are involved. All of it lives in `src-tauri/src/popover.rs`, which owns
show/hide/pin; `lib.rs` keeps only tray positioning. Three attributes matter:
`.nonactivatingPanel` style mask, `fullScreenAuxiliary | canJoinAllSpaces`
collection behavior, and a floating level.

Three ways to accidentally undo it:

- **Never call `set_always_on_top` or `set_visible_on_all_workspaces` on the
  window.** Both write the same AppKit properties the panel manages, and
  `set_visible_on_all_workspaces` sets collection behavior to `canJoinAllSpaces`
  *alone* — dropping `fullScreenAuxiliary` and restoring the original bug. (The
  `alwaysOnTop: true` in `tauri.conf.json` is fine: it's applied at window
  creation, before `popover::init` runs, and is a fallback if conversion fails.)
- **Never install the plugin's `set_event_handler`.** It *replaces* the window's
  `NSWindowDelegate`, which on a Tauri window is Tauri's own — and that's what
  raises `WindowEvent::Focused(false)`, i.e. all of hide-on-click-away.
- **Keep `core:window:deny-internal-toggle-maximize`** in
  `capabilities/default.json`. Maximizing a `fullScreenAuxiliary` panel *crashes*;
  `core:default` would permit it, and double-clicking the header's
  `data-tauri-drag-region` is exactly that gesture.

**Pin** (📍/📌 in the header) suppresses only the blur-hide, so the popover can
stay open while you work elsewhere. It's session state in `popover.rs` — not
`AppSettings` — and any explicit dismissal (tray, Escape, ⌘⇧J) clears it. The
webview isn't remounted on show/hide, so `App.tsx` re-reads it on `focus`.

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
