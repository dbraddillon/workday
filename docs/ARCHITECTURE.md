# Architecture

Local-first Tauri app. All logic runs on-device; there is no backend. The design
mirrors the layers in `mac-menubar-jira-slack-handoff.md`, and the layer
boundaries are deliberate **seams** — the point of v1 is to make later growth
(more sources, richer formatters, direct Slack posting, a shared backend)
possible without a rewrite.

## Layer map

| Layer | Code | Responsibility |
|---|---|---|
| Shell / host | `src-tauri/src/lib.rs` | Tray icon, popover window + positioning, global shortcut (⌘⇧J), background polling loop, command registry. macOS runs as an *accessory* (no Dock icon). |
| IPC | `src-tauri/src/commands.rs` | The only surface the React UI calls (`invoke`). Thin glue. |
| Domain model | `src-tauri/src/model.rs` | Normalized `Issue` / `ActivityEvent` and the **format-agnostic `StandupModel`**. The seam between sources and everything downstream. |
| Config / secrets | `src-tauri/src/config.rs` | `AppSettings` (non-secret) + Keychain accessors (secret). |
| Data | `src-tauri/src/db/` | SQLite connection, migrations, repositories. |
| Connectors | `src-tauri/src/connector/` | `WorkSourceConnector` trait; `jira.rs` (real) + `fake.rs` (dev). Owns *all* raw-Jira parsing. |
| Standup | `src-tauri/src/standup/` | `compose()` → `formatter` (render) → `summarizer` (optional AI polish). |
| Delivery | `src-tauri/src/delivery.rs` | `SlackDeliveryService` seam. v1 = copy-to-clipboard. |
| Sync | `src-tauri/src/sync.rs` | `SyncCoordinator`: choose connector, fetch, store, record a `sync_run`. |
| UI | `src/` | React popover. `api.ts` = typed invoke wrapper; `components/` = one file per tab. |

## The standup pipeline (kept strictly separated)

```
gather        compose              render                 polish (optional)
──────        ───────              ──────                 ─────────────────
repo::         standup::compose()   formatter::             summarizer::
issues_in_ ─▶  builds a         ─▶  render_with(key,    ─▶  ClaudeCliSummarizer
range +        StandupModel         model) → text          (falls back to
activity_      (sections, items,                           deterministic text)
in_range       notes, carryover)
```

- **Retrieval never knows the output format.** Swapping/adding a formatter or
  summarizer touches nothing upstream.
- **Formatters** implement `StandupFormatter`; register a key in `render_with`.
  v1 ships `default` (grouped-by-status, Slack markdown), `thread` (a team
  standup thread's five emoji-prefixed prompts), and `plain`.
- The composer emits a normalized model so a future formatter can *mirror a
  supplied example post's style* without changing retrieval — the core
  requirement from the brief.

## The AI-summary seam (design decision)

The `Summarizer` trait has three concrete pieces:

1. `PassthroughSummarizer` — returns the deterministic draft unchanged (default;
   AI polish is opt-in and never required).
2. `ClaudeCliSummarizer` — spawns a local `claude -p` CLI if one is on `PATH`.
   How that CLI authenticates is the user's own setup (Anthropic API key,
   Bedrock, or anything else the CLI supports); the app manages no credentials.
   Any error falls back to the passthrough draft.
3. `write_context_file()` — dumps the model + draft + style sample to
   `standup-context.json` so any external Claude instance can reformat it on
   demand, with no CLI configured inside the app.

This is the answer to "how do we do AI summaries without a backend": reuse
whatever local CLI the user already has, and always leave a file on disk as a
fallback. Later this can become a hosted-API summarizer by adding a fourth impl —
callers don't change.

## Data flow

```
tray/⌘⇧J ─▶ popover (React) ─┬─ invoke get_in_progress / get_recent ─▶ repo (SQLite cache)
                             ├─ invoke build_standup_model ─▶ compose(model)
                             ├─ invoke generate_standup ─▶ render (+ optional CLI polish) ─▶ save post
                             └─ copyToClipboard (clipboard plugin)

background loop ─▶ sync::run_sync ─▶ connector (jira|fake) ─▶ normalize ─▶ repo upsert ─▶ sync_runs log
```

## Storage & schema

SQLite in the app data dir. Migrations are an ordered list in
`db/migrations.rs`, gated by a `schema_version` table — append steps, never edit
shipped ones. Tables: `app_settings`, `issues`, `issue_activity`, `sync_runs`,
`generated_posts`. Secrets are **not** stored here — they live in the Keychain.

## Extension points (seams to keep intact)

- **New source:** implement `WorkSourceConnector`, normalize to `model.rs`,
  select it in `sync.rs`. Nothing else changes.
- **New standup style:** implement `StandupFormatter`, add a key in `render_with`.
- **Direct Slack posting:** implement `SlackDeliveryService` (webhook or bot),
  swap it in `commands::record_delivery`. Content selection stays out of it.
- **Shared backend / multi-user:** the repository layer is the swap point; the
  UI and standup logic already speak only normalized types.
