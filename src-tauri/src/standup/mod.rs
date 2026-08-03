//! Standup composer layer.
//!
//! The pipeline the doc calls for, with strict separation:
//!   1. gather   — issues + activity in range (repo functions)
//!   2. compose  — build the format-agnostic `StandupModel` (this file)
//!   3. render   — a `StandupFormatter` turns the model into text (formatter.rs)
//!   4. polish   — an optional `Summarizer` refines the text (summarizer.rs)
//!
//! Retrieval (steps 1-2) knows nothing about output format. Swapping or adding
//! formatters/summarizers never touches it — that's the whole point.

pub mod formatter;
pub mod summarizer;

use crate::model::{
    ActivityEvent, Issue, StandupItem, StandupModel, StandupSection, TimeRange,
};
use std::collections::BTreeMap;

/// Compose a normalized standup model from candidate issues + activity.
///
/// v1 grouping = by status disposition (Done / In progress / To do), which is
/// the most broadly useful default. Because the model carries `key`/`title` per
/// section, a future formatter can regroup (by project, by outcome) off the same
/// data, or a composer variant can group differently without changing formatters.
pub fn compose(
    range: TimeRange,
    issues: &[Issue],
    activity: &[ActivityEvent],
) -> StandupModel {
    // Index activity by issue for quick note attachment.
    let mut notes_by_issue: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for ev in activity {
        let note = match ev.activity_type.as_str() {
            "status_change" => ev.text_summary.clone(),
            "comment" => ev
                .text_summary
                .clone()
                .map(|t| format!("commented: {t}")),
            "assigned" => Some("reassigned".to_string()),
            "resolved" => Some("resolved".to_string()),
            _ => ev.text_summary.clone(),
        };
        if let Some(note) = note {
            notes_by_issue.entry(ev.issue_key.clone()).or_default().push(note);
        }
    }

    // Bucket issues by disposition.
    let mut done = StandupSection { key: "done".into(), title: "Done".into(), items: vec![] };
    let mut in_progress = StandupSection {
        key: "in_progress".into(),
        title: "In progress".into(),
        items: vec![],
    };
    let mut todo = StandupSection { key: "todo".into(), title: "Up next".into(), items: vec![] };

    for i in issues {
        let notes = notes_by_issue.get(&i.issue_key).cloned().unwrap_or_default();
        let item = StandupItem {
            issue_key: i.issue_key.clone(),
            summary: i.summary.clone(),
            status_name: i.status_name.clone(),
            status_category: i.status_category.clone(),
            project_key: i.project_key.clone(),
            browse_url: i.browse_url.clone(),
            activity_notes: notes,
            is_carryover: i.status_category == "indeterminate",
            included: true,
        };
        match i.status_category.as_str() {
            "done" => done.items.push(item),
            "new" => todo.items.push(item),
            _ => in_progress.items.push(item),
        }
    }

    let sections = [in_progress, done, todo]
        .into_iter()
        .filter(|s| !s.items.is_empty())
        .collect();

    StandupModel { time_range: range, sections, blockers: vec![] }
}
