//! Standup formatters. `StandupFormatter` renders a `StandupModel` to text.
//!
//! This is the seam the doc stresses hardest: keep formatting broad so example
//! posts from other people can be mirrored later without touching retrieval.
//! v1 ships one solid default; add variants by implementing the trait and
//! registering a key in `render_with`.

use crate::model::{StandupModel, StandupSection};

pub trait StandupFormatter {
    /// Stable identifier; used when registering/persisting the chosen formatter.
    #[allow(dead_code)]
    fn key(&self) -> &'static str;
    fn render(&self, model: &StandupModel) -> String;
}

/// Render with the named formatter, falling back to the default.
pub fn render_with(formatter_key: &str, model: &StandupModel) -> String {
    match formatter_key {
        "plain" => PlainFormatter.render(model),
        "thread" => ThreadFormatter.render(model),
        _ => DefaultFormatter.render(model),
    }
}

/// Concise, grouped-by-status Slack-friendly default. Uses light markdown that
/// Slack renders (bold section headers, bullet items).
pub struct DefaultFormatter;

impl StandupFormatter for DefaultFormatter {
    fn key(&self) -> &'static str {
        "default"
    }

    fn render(&self, model: &StandupModel) -> String {
        let mut out = String::new();
        out.push_str(&format!("*Standup — {}*\n", model.time_range.label));
        for section in &model.sections {
            let included: Vec<_> = section.items.iter().filter(|i| i.included).collect();
            if included.is_empty() {
                continue;
            }
            out.push_str(&format!("\n*{}*\n", section.title));
            for item in included {
                let notes = if item.activity_notes.is_empty() {
                    String::new()
                } else {
                    format!(" _({})_", item.activity_notes.join("; "))
                };
                out.push_str(&format!(
                    "• {} — {}{}\n",
                    item.issue_key, item.summary, notes
                ));
            }
        }
        if !model.blockers.is_empty() {
            out.push_str("\n*Blockers*\n");
            for b in &model.blockers {
                out.push_str(&format!("• {b}\n"));
            }
        }
        out.trim_end().to_string()
    }
}

/// Plain, key-and-summary only — no notes, no markdown emphasis. Useful when the
/// destination doesn't render markdown, or as a minimal alternative.
pub struct PlainFormatter;

impl StandupFormatter for PlainFormatter {
    fn key(&self) -> &'static str {
        "plain"
    }

    fn render(&self, model: &StandupModel) -> String {
        let mut out = String::new();
        out.push_str(&format!("Standup — {}\n", model.time_range.label));
        for section in &model.sections {
            render_plain_section(&mut out, section);
        }
        out.trim_end().to_string()
    }
}

fn render_plain_section(out: &mut String, section: &StandupSection) {
    let included: Vec<_> = section.items.iter().filter(|i| i.included).collect();
    if included.is_empty() {
        return;
    }
    out.push_str(&format!("\n{}:\n", section.title));
    for item in included {
        out.push_str(&format!("- {} {}\n", item.issue_key, item.summary));
    }
}

/// Mirrors the team's standup thread's five prompts as a threaded reply, one
/// emoji-prefixed line each. "Working on" is built from the in-progress section;
/// "Blockers" from `model.blockers`; the other three come from the user-authored
/// `narrative` (defaults set in Settings). Kept as a formatter so the thread's
/// prompt emoji live in one place and can change without touching retrieval.
pub struct ThreadFormatter;

impl ThreadFormatter {
    // Builtin prompt emoji, used only when the model carries no override (older
    // drafts, or a formatter invoked without settings). The live template lives
    // in AppSettings and rides in on `StandupNarrative`.
    const DOING: &'static str = ":city_sunrise:";
    const WORKING: &'static str = ":computer:";
    const PAIRING: &'static str = ":two-peas-in-a-pod:";
    const BLOCKER: &'static str = ":blocker:";
    const POST_SCRUM: &'static str = ":high-five:";
}

/// Use the model's prompt emoji if set, else the builtin fallback.
fn prompt<'a>(override_: &'a str, fallback: &'a str) -> &'a str {
    if override_.trim().is_empty() { fallback } else { override_ }
}

/// Pick a Slack emoji suffix for a work item from its Jira status, so the reader
/// sees where each ticket sits in the pipeline. Status *names* vary per Jira
/// instance, so we match on keywords rather than exact strings, most-specific
/// first. Returns "" (no marker) for plainly in-progress/backlog items.
///
///   deployed / released / shipped        → :deployparrot:
///   merged                               → :merged:
///   in review / PR / code review / QA    → :pull_request:
///   otherwise done (statusCategory=done) → :white_check_mark:
fn state_marker(status_name: &str, status_category: &str) -> String {
    let s = status_name.to_ascii_lowercase();
    let has = |needle: &str| s.contains(needle);

    if has("deploy") || has("released") || has("release") || has("shipped") || has("prod") {
        " :deployparrot:".into()
    } else if has("merged") {
        " :merged:".into()
    } else if has("review") || has("pr ") || s == "pr" || has("pull request") || has("qa") {
        " :pull_request:".into()
    } else if status_category == "done" {
        " :white_check_mark:".into()
    } else {
        String::new()
    }
}

impl StandupFormatter for ThreadFormatter {
    fn key(&self) -> &'static str {
        "thread"
    }

    fn render(&self, model: &StandupModel) -> String {
        let n = &model.narrative;
        let mut out = String::new();

        let p_doing = prompt(&n.prompt_doing, Self::DOING);
        let p_working = prompt(&n.prompt_working, Self::WORKING);
        let p_pairing = prompt(&n.prompt_pairing, Self::PAIRING);
        let p_blocker = prompt(&n.prompt_blocker, Self::BLOCKER);
        let p_post_scrum = prompt(&n.prompt_post_scrum, Self::POST_SCRUM);

        // How are you doing?
        out.push_str(&format!("{} {}\n", p_doing, blank_to_dash(&n.doing)));

        // What are you working on? — everything touched in the window: in-progress
        // first, then recently-done (marked done). Together these cover
        // "what I worked on / finished".
        //
        // Layout: the prompt on its own line, then one flush-left bullet per item.
        // Slack strips leading whitespace, so we must NOT indent continuation
        // lines — bullets keep items visually distinct without it.
        let mut working: Vec<String> = Vec::new();
        for key in ["in_progress", "done"] {
            if let Some(s) = model.sections.iter().find(|s| s.key == key) {
                for i in s.items.iter().filter(|i| i.included) {
                    let mark = state_marker(&i.status_name, &i.status_category);
                    working.push(format!("• {} — {}{}", i.issue_key, i.summary, mark));
                }
            }
        }
        if working.is_empty() {
            out.push_str(&format!("{} —\n", p_working));
        } else {
            out.push_str(&format!("{p_working}\n"));
            for line in &working {
                out.push_str(&format!("{line}\n"));
            }
        }

        // Any pairing opportunities?
        out.push_str(&format!("{} {}\n", p_pairing, blank_to_dash(&n.pairing)));

        // Any Blockers? — derived from Jira status; falls back to the configured
        // default (blank → "—"). Multiple blockers use flush-left bullets so Slack
        // doesn't collapse them.
        if model.blockers.is_empty() {
            out.push_str(&format!("{} {}\n", p_blocker, blank_to_dash(&n.blocker)));
        } else {
            out.push_str(&format!("{p_blocker}\n"));
            for b in &model.blockers {
                out.push_str(&format!("• {b}\n"));
            }
        }

        // Anything for post scrum?
        out.push_str(&format!("{} {}\n", p_post_scrum, blank_to_dash(&n.post_scrum)));

        out.trim_end().to_string()
    }
}

/// A blank freeform answer renders as an em dash so the line still reads as a
/// deliberate "nothing here" rather than a formatting glitch.
fn blank_to_dash(s: &str) -> String {
    let t = s.trim();
    if t.is_empty() { "—".to_string() } else { t.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{StandupItem, StandupNarrative, TimeRange};

    fn item(key: &str, summary: &str) -> StandupItem {
        item_with(key, summary, "In Progress", "indeterminate")
    }

    fn item_with(key: &str, summary: &str, status_name: &str, status_category: &str) -> StandupItem {
        StandupItem {
            issue_key: key.into(),
            summary: summary.into(),
            status_name: status_name.into(),
            status_category: status_category.into(),
            project_key: None,
            browse_url: String::new(),
            activity_notes: vec![],
            is_carryover: true,
            included: true,
        }
    }

    fn model(sections: Vec<StandupSection>, blockers: Vec<String>) -> StandupModel {
        StandupModel {
            time_range: TimeRange {
                start: String::new(),
                end: String::new(),
                label: "Today".into(),
            },
            sections,
            blockers,
            narrative: StandupNarrative {
                doing: "Doing well!".into(),
                pairing: "Free after standup".into(),
                post_scrum: String::new(), // exercises blank → dash
                blocker: "Nope".into(),
                // Prompt emoji left empty → formatter uses its builtin fallbacks.
                ..Default::default()
            },
        }
    }

    #[test]
    fn thread_renders_five_prompts_in_order() {
        let m = model(
            vec![StandupSection {
                key: "in_progress".into(),
                title: "In progress".into(),
                items: vec![item("ABC-1", "First"), item("ABC-2", "Second")],
            }],
            vec![],
        );
        let out = ThreadFormatter.render(&m);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], ":city_sunrise: Doing well!");
        assert_eq!(lines[1], ":computer:"); // prompt on its own line
        assert_eq!(lines[2], "• ABC-1 — First"); // flush-left bullets (Slack-safe)
        assert_eq!(lines[3], "• ABC-2 — Second");
        assert_eq!(lines[4], ":two-peas-in-a-pod: Free after standup");
        assert_eq!(lines[5], ":blocker: Nope");
        assert_eq!(lines[6], ":high-five: —"); // blank post-scrum → dash
    }

    #[test]
    fn thread_lists_blockers_and_handles_no_work() {
        let m = model(vec![], vec!["ABC-9 — Stuck (Blocked)".into()]);
        let out = ThreadFormatter.render(&m);
        assert!(out.contains(":computer: —")); // no in-progress work
        // Prompt on its own line, blocker as a flush-left bullet (Slack-safe).
        assert!(out.contains(":blocker:\n• ABC-9 — Stuck (Blocked)"));
    }

    #[test]
    fn working_line_includes_done_items_marked() {
        let m = model(
            vec![
                StandupSection {
                    key: "in_progress".into(),
                    title: "In progress".into(),
                    items: vec![item("ABC-1", "Ongoing")],
                },
                StandupSection {
                    key: "done".into(),
                    title: "Done".into(),
                    items: vec![item_with("ABC-9", "Finished", "Done", "done")],
                },
            ],
            vec![],
        );
        let out = ThreadFormatter.render(&m);
        // In-progress (no mark), done after (checkmark), both bulleted.
        assert!(out.contains("• ABC-1 — Ongoing"));
        assert!(out.contains("• ABC-9 — Finished :white_check_mark:"));
    }

    #[test]
    fn state_markers_by_status() {
        // in-progress / backlog → no marker
        assert_eq!(state_marker("In Progress", "indeterminate"), "");
        assert_eq!(state_marker("To Do", "new"), "");
        // review family → :pull_request:
        assert_eq!(state_marker("In Review", "indeterminate"), " :pull_request:");
        assert_eq!(state_marker("Code Review", "indeterminate"), " :pull_request:");
        assert_eq!(state_marker("QA", "indeterminate"), " :pull_request:");
        // merged → :merged:
        assert_eq!(state_marker("Merged", "indeterminate"), " :merged:");
        // deployed/released → :deployparrot: (wins even if category=done)
        assert_eq!(state_marker("Deployed", "done"), " :deployparrot:");
        assert_eq!(state_marker("Released to Prod", "done"), " :deployparrot:");
        // plain done → checkmark
        assert_eq!(state_marker("Done", "done"), " :white_check_mark:");
        assert_eq!(state_marker("Closed", "done"), " :white_check_mark:");
    }
}
