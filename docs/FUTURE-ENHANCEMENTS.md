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
