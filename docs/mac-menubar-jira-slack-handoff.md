# Mac Menu Bar Work Glance + Slack Standup App

## Objective

Build a macOS-first menu bar app that gives a quick view of current Jira work and can generate a Slack standup post from a selected time period.

The app is local-first for v1 and intended for one user on one MacBook. There is no backend in v1. The app should read from Jira, store a local cache, and generate a draft Slack post that the user can review, edit if needed, and send or copy.

This app is intentionally narrow:
- Glanceable work status.
- Recent work/activity from Jira.
- Generate a standup/update post for Slack from a time window.

The output format of the Slack post should be configurable later, but for v1 the implementation should keep the formatting system broad and flexible because example posts from other people will be provided and the app will eventually learn or mirror those structures.

## Product Intent

The product should feel like a compact Mac utility rather than a traditional desktop app.

Primary use cases:
- Click the menu bar icon and instantly see work in progress.
- See recent Jira activity and status without opening Jira first.
- Generate a standup/status update based on what happened in a selected period.
- Review the generated post and either send it to Slack or copy it.

This is not a team product in v1.
This is not a multi-user sync platform in v1.
This is not a generalized work intelligence platform in v1.

## Scope

### In scope for v1

- macOS menu bar app.
- Local-first architecture.
- Jira Cloud integration.
- Local cache of work items and recent activity.
- Compact glance view for work in progress and recent work.
- Standup/post generation from a selected time range.
- Slack post delivery or copy-to-clipboard flow.
- Settings for Jira and Slack config.
- Background refresh plus manual refresh.
- Offline display of cached data with freshness indication.

### Out of scope for v1

- Multi-user support.
- Central shared backend.
- Role-based access.
- Complex team analytics.
- Full Slack conversation sync.
- Generic plugin framework.
- Support for many sources beyond Jira.
- Advanced AI orchestration or cloud inference requirements.

## Recommended Stack

Use:
- Tauri
- TypeScript
- React
- SQLite

Reasoning:
- Good fit for a Mac-first tray/menu bar app.
- Smaller and more native-feeling than a heavier Electron-first app.
- Claude can build this stack quickly.
- Local SQLite is enough for v1 and gives a clean migration path later.

Use the macOS Keychain for secrets where appropriate.
Do not store sensitive tokens in plain text SQLite fields if avoidable.

## High-Level Architecture

Split the system into these parts:

1. Shell / Desktop Host
   - Tauri app shell.
   - Menu bar icon.
   - Popover-style primary window.
   - Global shortcut to open/focus.
   - Open at login option later if easy.

2. UI Layer
   - Compact React-based menu bar UI.
   - Sections for in-progress work, recent work, and standup generation.
   - Settings UI.
   - Draft preview/edit/send UI for Slack posts.

3. Source Connector Layer
   - Jira connector in v1.
   - Designed as an abstraction so other sources could be added later.
   - Responsible for auth, fetch, field mapping, pagination, normalization.

4. Local Data Layer
   - SQLite schema and migrations.
   - Cache of issues and normalized activity events.
   - Saved views, app settings, sync run logs, post generation history if useful.

5. Standup Composer Layer
   - Takes normalized Jira data for a date/time range.
   - Produces structured summary input.
   - Renders a Slack post draft according to a configurable formatting strategy.
   - Keeps formatting broad and adaptable.

6. Delivery Layer
   - Slack delivery support.
   - At minimum support copy-to-clipboard.
   - If direct posting is implemented, keep it simple and reliable.

## Core UX

### Main popover

When the user clicks the menu bar icon, show a compact window with:
- Header with last sync time, refresh button, settings button.
- Tab or segmented control for:
  - In Progress
  - Recent
  - Standup

### In Progress tab

Each row should show:
- Issue key
- Summary
- Current status
- Last updated time
- Link/open action to Jira
- Optional small last-action text

### Recent tab

Show work touched in a recent time window such as:
- Today
- Last 24 hours
- Last 3 days
- Last 7 days

This can be filter-driven later, but keep v1 simple.

### Standup tab

This is the second major v1 feature.
The user should be able to:
- Pick a time window.
- Choose which items to include or exclude.
- Generate a post draft.
- Optionally tweak the text.
- Copy it or send it to Slack.

The app should not hardcode a single post style too aggressively.
Instead it should support a broad shaping system so examples can later guide the exact formatting.

## Standup / Slack Post Concept

This feature should be intentionally broad in implementation.
The app needs to support creating a post from work performed over a selected time period, but the exact look, sentence style, grouping, tone, and fields included should be configurable because example posts from other people will be provided later.

### v1 goal

Generate a useful standup/status draft from Jira activity during a chosen time period.

Examples of time windows:
- Since yesterday morning.
- Today.
- Last 24 hours.
- Since last standup.
- Custom date/time range.

### Draft generation model

Do not tightly bind the implementation to a single template.
Instead build a pipeline like this:

1. Collect candidate Jira items and activity from the selected period.
2. Normalize the data into a structured intermediate model.
3. Let formatting logic transform that model into a Slack-ready draft.
4. Allow editing before delivery.

### Intermediate standup model

Create a normalized structure that is format-agnostic. For example:
- time_range
- included_issues
- grouped_sections
- item summaries
- status transitions
- comments or actions if included
- carryover work
- blockers or needs-attention markers if available
- links back to Jira

This model is important because later a different formatter can reproduce the style of supplied example posts without rewriting all retrieval logic.

### Formatting strategy

Design the formatter so it can later support:
- Bullet-oriented updates.
- Grouped by project.
- Grouped by outcome.
- Grouped by status.
- Short concise standup style.
- Longer status update style.
- Optional inclusion/exclusion of links, keys, statuses, or action notes.

For v1, one simple default formatter is enough, but the code should clearly separate:
- data gathering
- summarization/selection
- string rendering

### Editing and delivery

The user should see:
- generated draft text
- copy button
- send-to-Slack button if configured
- regenerate button
- maybe include/exclude toggles for issues before final render

## Jira Integration

Use Jira Cloud REST APIs and JQL-based search.

Requirements:
- Fetch only the fields needed for the glance UI and standup generation.
- Support pagination.
- Cache fetched issue data locally.
- Prefer incremental refresh patterns where practical.

The connector should support default views such as:
- My open work.
- My recently updated work.
- Work touched in a time period.

Potential fields to normalize where available:
- issue id
- issue key
- summary
- status
- status category
- assignee
- reporter
- updated at
- created at
- project key/name
- browse URL
- labels if useful
- recent comments or changelog-derived activity if practical

Do not overbuild the Jira connector in v1.
The goal is good enough signal for glance status and standup draft generation.

## Slack Integration

Slack support in v1 should focus on one reliable outcome: getting the generated standup post into Slack with minimal friction.

Preferred delivery options:
1. Copy to clipboard, always available.
2. Direct post support if configured.

If direct posting is added, keep the abstraction simple and avoid broad Slack feature sprawl.
The app does not need to manage threads, channels, histories, or rich team workflows in v1.

Slack configuration may include:
- destination channel or webhook target
- display/test mode
- enable direct post vs copy-only

Because post format may evolve based on examples, the Slack delivery layer should accept final rendered text and not own business logic around content selection.

## Local Data Model

Suggested tables:
- jira_accounts
- jira_saved_views
- issues
- issue_activity
- sync_runs
- slack_destinations
- generated_posts
- app_settings

### Suggested issues fields

- id
- source_issue_id
- issue_key
- summary
- status_name
- status_category
- assignee_display
- reporter_display
- project_key
- project_name
- updated_at
- created_at
- browse_url
- first_seen_at
- last_seen_at
- raw_json

### Suggested issue_activity fields

Keep this broad and normalized.
Potential fields:
- id
- issue_key
- activity_type
- activity_at
- actor_display
- old_value
- new_value
- text_summary
- raw_json

The point is not perfect event sourcing.
The point is enough structured activity to support recent-work display and standup generation.

### Suggested generated_posts fields

- id
- generated_at
- time_range_start
- time_range_end
- formatter_key
- draft_text
- delivery_status
- delivered_at
- destination_summary

## Sync Model

Use polling for v1.

Suggested behavior:
- Initial sync on setup.
- Background refresh every few minutes.
- Manual refresh from the app.
- Sync errors should be visible but non-fatal.
- Cached data should still display if the network or auth fails.
- Show freshness timestamp in the UI.

Keep a sync log for troubleshooting.

## Settings

Minimum settings:
- Jira base URL
- Jira email / identity
- Jira API token or auth details
- Refresh interval
- Default recent time range
- Slack destination settings
- Enable direct posting vs copy-only
- Formatter selection if more than one exists later

Store secrets securely.
Use Keychain where possible.

## UI Details

The UI should feel like a compact utility similar to a menu bar productivity tool.
It should not feel like a shrunken enterprise dashboard.

### Key interactions

- Click menu bar icon to open.
- Keyboard navigation through items.
- Enter opens Jira item.
- Refresh shortcut.
- Fast switch between tabs.
- Generate standup draft in one step.
- Quick copy/send action.

### Important constraints

- Keep the window compact.
- Dense but readable layout.
- Show useful metadata without overwhelming the screen.
- Avoid too much configuration in the main flow.

## Extensibility Guidance

Even though v1 is local and single-user, build internal seams that allow later growth.

Design the code so these could be added later without major rewrites:
- Other issue/task sources.
- Additional Slack formatting profiles.
- Team/user profiles.
- Shared backend sync.
- Centralized config.
- AI-assisted summarization if useful later.

However, do not implement those future features now.
Only preserve the boundaries that make them possible.

## Engineering Principles

Optimize for:
- Local-first simplicity.
- Fast iteration.
- Clear interfaces.
- Maintainable code.
- Good defaults.
- Strong separation between data retrieval, normalization, rendering, and delivery.
- Minimal operational overhead.

Avoid:
- Premature microservices.
- Backend services in v1.
- Overly generic plugin systems.
- Heavy configuration burdens.
- Complex workflow engines.

## Implementation Order

Build in this order:

1. Tauri shell and menu bar behavior.
2. Fake-data UI for all three tabs: In Progress, Recent, Standup.
3. SQLite schema and repository layer.
4. Jira authentication and basic fetch.
5. In Progress and Recent real-data views.
6. Standup intermediate model and first formatter.
7. Slack delivery/copy flow.
8. Background polling and sync logs.
9. Packaging, setup docs, and polish.

## Deliverables

Claude should produce:
- Working local macOS app.
- Clean repo structure.
- Setup and run instructions.
- SQLite migrations.
- Fake-data mode for development.
- Jira connector implementation.
- Standup generation pipeline.
- Slack copy/post flow.
- Settings screen.
- Basic error handling and stale-data behavior.

## Suggested Internal Interfaces

Examples only; exact naming can vary.

- `WorkSourceConnector`
- `JiraConnector`
- `IssueRepository`
- `ActivityRepository`
- `StandupComposer`
- `StandupFormatter`
- `SlackDeliveryService`
- `SyncCoordinator`

Important design rule:
The standup feature should consume normalized work/activity data and should not be tightly coupled to raw Jira responses.

## Non-Goals

Do not spend time on:
- Multi-user onboarding.
- Team admin features.
- Rich Slack Block Kit design unless it comes almost free.
- Overly smart AI summarization in v1.
- Complex permissions models.
- Cross-platform polish beyond what is needed for Mac-first success.

## Acceptance Criteria

The app is successful for v1 if:
- It runs as a Mac menu bar app.
- It shows current Jira work in a compact, useful form.
- It shows recent work/activity clearly.
- It can generate a Slack standup/update draft from a selected time range.
- The draft flow is broad enough that future example-based formatting can be swapped in without reworking data retrieval.
- The user can copy or send the post with minimal friction.
- The app remains local-first and simple to configure.

## Final Notes

Bias toward a clean, useful, boring v1.
A compact glance view plus a solid standup-post generator is enough.
Build the standup system with a format-agnostic intermediate model so example post formats can be layered in later without restructuring the app.
