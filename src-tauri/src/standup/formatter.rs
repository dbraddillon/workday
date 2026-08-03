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
