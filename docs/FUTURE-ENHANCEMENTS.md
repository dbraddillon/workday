# Future enhancements

Ideas that fit the design but are deliberately **not built** in v1. Each preserves
the existing seams (see `ARCHITECTURE.md`) so it can be added later without a
rewrite. Nothing here is a commitment — they exist so the design decisions are
already settled when a real need shows up.

---

## GitHub state enrichment for standup markers

**Status:** deferred. Not built. Build only if the manual-fix friction below
actually nags.

### The gap

Work items get a Slack emoji from their Jira status via
`standup/formatter.rs::state_marker`:

| Status signal | Marker |
|---|---|
| actively in development (`indeterminate` / "in progress" / "wip" / …) | `:work-in-progress:` |
| in review / PR / code review / QA | `:pull_request:` |
| merged | `:merged:` |
| deployed / released / shipped / prod | `:deployparrot:` |
| otherwise done | `:white_check_mark:` |
| backlog / To Do (`new`) | *(none)* |

That marker is only as accurate as how faithfully the *Jira status* tracks the
real work. If a PR is open but the ticket still reads "In Progress," the marker
says `:work-in-progress:`, not `:pull_request:`. GitHub knows the real state;
Jira may lag it.

### Design (agreed)

- **Jira stays the source of truth** for *which items* appear. GitHub is a pure
  **enrichment** pass — it only refines the *state* of an item Jira already
  surfaced (branch-only / PR open / merged / deployed, and to which env). It never
  adds or removes items.
- **Runs only at standup-generate time — never on the sync/poll loop.** So there
  is no per-poll cost; enrichment happens once, when the user hits generate.
- **Ticket ↔ PR match by convention:** PRs/branches are prefixed with the Jira
  ticket number → a simple prefix match, no fuzzy linking.
- **The formatter still decides which emoji.** `state_marker`'s precedence is
  unchanged; GitHub only supplies more accurate *facts* for it to map. Keep the
  enriched result constrained to a small enum (e.g. `pr_state ∈ {none, open,
  merged}`, `deployed_env: Option<String>`) — the mapping is near-mechanical, so
  there is very little for any intermediary to get wrong.
- **Graceful fallback is required**, exactly like the AI-polish path: preflight
  the dependency, gate the toggle (à la `ai_polish_available`), and if anything is
  missing or errors, silently fall back to the Jira-derived marker. A wrong marker
  costs the user a two-second manual edit — never fail the draft over it.
- **Seam:** an enrichment step at generate time, alongside `standup/summarizer.rs`,
  that overlays GitHub state onto the normalized items before render. Everything
  upstream (retrieval, `model.rs`) is untouched; the formatter stays
  format-agnostic.

### Two mechanism options (flexible — pick per machine)

Both reach the same enriched state; they differ only in *how* GitHub is queried.

**Option A — reuse the `claude` CLI, which shells out to the `gh` CLI**
*(the expected default — most setups already have both)*

- Reuses the same locally-configured `claude` instance already used for AI polish
  (`summarizer.rs`), which invokes the locally-authed `gh` CLI.
- **No new secret.** `gh auth` already holds the GitHub credentials; nothing new
  in Keychain.
- Fits the app's "reuse what's installed, degrade gracefully" spirit.
- Preflight `claude --version` **and** `gh auth status`; fall back if either is
  absent/unauthed.
- Note: this puts a language model in front of a deterministic lookup. Acceptable
  *only* because it runs once per generate (not per poll) and the output is
  constrained to a small enum. Not a fit for the poll loop.

**Option B — direct GitHub PAT + REST**
*(fallback when `gh`/`claude` isn't available, or when strict determinism is
preferred)*

- A fine-grained GitHub token (minimal scope), stored in **Keychain** following
  the Jira-token pattern in `config.rs`.
- Fully deterministic: same PR state → same marker, no intermediary.
- Cost: a second secret to provision and manage, and a second auth surface.
- Preflight the token; fall back if missing or the call errors.

Neither is required to be present — the feature is opt-in, and absence of *both*
simply leaves the Jira-derived marker in place.

### Why deferred

This is a convenience layered on a convenience. The honest baseline — today's
Jira-derived marker plus a two-second manual tweak when it's off — covers the
common case for zero added surface. Cheaper interim win: tune the `state_marker`
keywords to the actual Jira instance's status names. Let recurring manual-fix
friction be the trigger to build this, not novelty.

---

## Outlook / calendar integration

**Status:** deferred, and **blocked on an unanswered access question** — unlike the
GitHub idea above, this one cannot be built at will. Read the mechanism finding
first; it invalidates the obvious approach.

### The blocking finding: AppleScript can't see the mailbox

Outlook 16.111.3 on the author's machine runs in **New Outlook** mode
(`defaults read com.microsoft.Outlook` → `IsRunningNewOutlook = 1`). Real mail and
calendar live in the Hx store (`~/Library/Group Containers/UBF8T346G9.Office/
Outlook/Outlook 15 Profiles/Main Identity/HxStore.hxd` + `hxcore.hfl`, ~200MB,
actively written). The AppleScript bridge only exposes the **empty legacy "On My
Computer" identity**.

Measured with automation permission granted (so these are not permission errors):

| Probe | Result |
|---|---|
| `get name of accounts` | error `-1728` "Can't get every account" |
| `count of messages of inbox` | `0` |
| messages summed over `every mail folder` | `0` |
| `count of calendar events` | `0` |
| `count of every meeting message` | `0` |
| `name of every mail folder` | `On My Computer, Inbox, Outbox, …` (legacy tree) |

**`Outlook.sdef` is misleading — do not design against it.** It advertises exactly
the verbs this feature would want (`accept meeting`, `decline meeting`, `accept
tentatively meeting`), but they would drive the empty legacy store.

Side channels checked and ruled out: `~/Library/Calendars` is empty and there is no
Exchange account in macOS Internet Accounts, so the work calendar is **not** mirrored
into system Calendar. No `az` / `m365` / `mgc` CLI installed.

### Consequence: Microsoft Graph is the only path

Delegated auth (device-code), refresh token → **Keychain**, following the Jira-token
pattern in `config.rs`. It fits the existing seams well — a second
`WorkSourceConnector` normalizing into `model.rs`, selection in `sync.rs`.

**What's unresolved:** whether the tenant (`revspringinc.com` →
`0c7b2a3d-9015-4503-80e7-5c617cbf7d55`, Entra ID, NA) permits app registration or
delegated `Mail.Read` / `Calendars.Read` consent. A tenant's consent policy is not
readable unauthenticated — the only way to find out is to actually run a device-code
grant, which was deliberately **not** done. Corporate tenants commonly block it.
Settle this with IT before writing code; everything below is contingent on it.

A shipped feature needs **its own registered app ID**. (A probe can lean on the
well-known public Azure CLI client ID as a diagnostic, but that is not a shipping
mechanism.)

### Scope, in build order (all read-only)

1. **Invite/conflict digest** — today's invites plus double-booking warnings in the
   popover.
2. **Standup meeting-load enrichment** — see the on-by-default note below.
3. **Phishing flag** — see the injection caution below.

### Two decisions already settled

**Don't rebuild auto-accept.** Exchange has a server-side "automatically process
meeting requests" setting that works with the laptop closed — a menu bar app never
will. Anything Workday did here would be strictly worse. Auto-accept is also the one
irreversible write in this space. Use the native setting; keep Workday read-only.
The additive part is *visibility* (conflicts, unusual meeting load), not the accept.

**Phishing triage must not feed the `claude` CLI untrusted text.** `standup/
summarizer.rs` spawns a tool-capable local CLI. Piping attacker-authored email bodies
into it is a prompt-injection path, and a phishing email is precisely the payload that
would attempt it. Required shape, mirroring the constrained-enum argument above:
- Output constrained to a small enum (e.g. `risk ∈ {none, suspicious}` + a fixed
  reason code) — never free-form text acted upon.
- **Flag to the user only.** Never auto-reply to "verify" a sender: replying confirms
  a live address to a spammer.
- Preflight and degrade silently, like `ai_polish_available` /
  `ai_polish_fell_back`.

### Standup enrichment: on by default, individually skippable

Requested behavior — the meeting line should be **generated by default and visible in
the draft**, with a checkbox to *exclude it from the copy* rather than to stop
computing it. Rationale: most meetings are recurring and unremarkable, so the line is
usually noise the user unchecks; but on a day with unusual load (e.g. two extra
one-offs on top of the recurring set) the user wants it already sitting there to grab,
not something to go turn on.

So: compute always, render always, and let the toggle control inclusion — not
generation. Toggle state belongs in `AppSettings` alongside the `thread_*` fields, and
rides to the formatter via `StandupNarrative` so the formatter stays
format-agnostic (it consumes the model, not settings).

Worth surfacing only what's *notable* (count vs. the recurring baseline, one-offs
called out) rather than dumping the full agenda — consistent with keeping the popover
compact and dense.

### Why deferred

The mechanism the idea assumed doesn't exist, and the replacement depends on a tenant
permission nobody has confirmed. Native Exchange already covers the auto-accept half.
Resolve the Graph-consent question first; if it's a no, this stays parked
permanently rather than being faked through a scraped local store.
