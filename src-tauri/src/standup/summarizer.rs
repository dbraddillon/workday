//! Optional AI polish for standup drafts — the `Summarizer` seam.
//!
//! Design decision (see docs/ARCHITECTURE.md): v1 reuses the locally
//! authenticated `claude` CLI rather than a hosted API. On this machine the CLI
//! is Bedrock-backed (`CLAUDE_CODE_USE_BEDROCK=1`, `AWS_PROFILE=ClaudeCode` in
//! ~/.claude/settings.json), so spawning `claude -p` runs against Bedrock with
//! the user's own AWS creds — no API keys, no separate billing. This mirrors the
//! approach in the local `ClaudeChat` repo.
//!
//! Three things live here, matching what was asked for:
//!   1. `Summarizer` trait — the swap point (deterministic → CLI → hosted API).
//!   2. `PassthroughSummarizer` — always-works default (returns the draft as-is).
//!   3. `ClaudeCliSummarizer` — opt-in polish via `claude -p`.
//! Plus `write_context_file`: dumps the normalized model + draft + a style
//! sample to a known path so *any* running Claude instance can be pointed at it
//! ("summarize this") without the app calling out at all.

use crate::model::{StandupDraft, StandupModel};
use std::path::PathBuf;
use std::process::Stdio;

/// Refines a rendered draft. Implementors must never *require* network/credentials
/// — callers fall back to the passthrough result if a summarizer errors.
#[allow(async_fn_in_trait)]
pub trait Summarizer {
    async fn polish(&self, draft: &str, style_hint: &str) -> Result<String, String>;
}

/// No-op. The deterministic formatter output is already useful on its own.
pub struct PassthroughSummarizer;

impl Summarizer for PassthroughSummarizer {
    async fn polish(&self, draft: &str, _style_hint: &str) -> Result<String, String> {
        Ok(draft.to_string())
    }
}

/// Best-effort check for whether AI polish can work: is a `claude` CLI on PATH
/// and does it respond to `--version`? Used to gate the UI toggle so teammates
/// without the CLI aren't offered an option that silently no-ops. This does NOT
/// verify the CLI is authenticated (Bedrock/API key) — that only surfaces at
/// actual polish time, where we fall back to the deterministic draft anyway.
pub async fn claude_cli_available() -> bool {
    tokio::process::Command::new("claude")
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Polishes the draft by piping it through the local `claude` CLI in headless
/// mode. Inherits the ambient CLI auth (Bedrock, an Anthropic API key, or
/// whatever the CLI is logged in with), so it "just works" wherever the CLI is
/// configured — and is skipped entirely when it isn't.
pub struct ClaudeCliSummarizer;

impl Summarizer for ClaudeCliSummarizer {
    async fn polish(&self, draft: &str, style_hint: &str) -> Result<String, String> {
        let system = "You are refining a developer's daily standup update for Slack. \
            Keep it concise and skimmable. Preserve every issue key and its meaning. \
            Do not invent work that isn't in the draft. Return ONLY the refined post text, \
            no preamble, no code fences.";

        let prompt = format!(
            "Refine this standup draft.\n\nStyle guidance: {}\n\n--- DRAFT ---\n{}\n--- END DRAFT ---",
            if style_hint.trim().is_empty() {
                "concise bullets, grouped by status, Slack markdown"
            } else {
                style_hint
            },
            draft
        );

        // `claude -p <prompt> --system-prompt <system> --output-format json`
        let output = tokio::process::Command::new("claude")
            .arg("-p")
            .arg(&prompt)
            .arg("--system-prompt")
            .arg(system)
            .arg("--output-format")
            .arg("json")
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|e| format!("could not launch `claude` CLI: {e}"))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("claude CLI failed: {}", err.trim()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        // The CLI emits a JSON envelope: { "result": "...", "is_error": bool, ... }
        match serde_json::from_str::<serde_json::Value>(&stdout) {
            Ok(v) => {
                if v.get("is_error").and_then(|e| e.as_bool()).unwrap_or(false) {
                    return Err(format!(
                        "claude reported error: {}",
                        v.get("result").and_then(|r| r.as_str()).unwrap_or("unknown")
                    ));
                }
                let text = v
                    .get("result")
                    .and_then(|r| r.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if text.is_empty() {
                    Err("claude returned empty result".into())
                } else {
                    Ok(text)
                }
            }
            // Not JSON? Treat raw stdout as the result (CLI fallback behavior).
            Err(_) => {
                let t = stdout.trim();
                if t.is_empty() {
                    Err("claude returned no output".into())
                } else {
                    Ok(t.to_string())
                }
            }
        }
    }
}

/// Where the on-demand context file is written. Kept next to the app's data dir
/// but the *filename* is stable so you can tell any Claude instance to read it.
pub fn context_file_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("standup-context.json")
}

/// Serialize the normalized model + current draft + a style sample to disk.
/// This is the "grab it on demand" escape hatch: rather than the app calling
/// Claude, you can point any running Claude at this file and ask it to
/// summarize/reformat, feeding it example posts as they become available.
pub fn write_context_file(
    data_dir: &std::path::Path,
    model: &StandupModel,
    draft: &StandupDraft,
) -> Result<PathBuf, String> {
    let payload = serde_json::json!({
        "note": "Normalized standup context for on-demand summarization. \
                 Point any Claude instance at this file and ask it to reformat/summarize. \
                 Replace `style_sample` with a real example post to mirror its structure.",
        "generated_at": draft.time_range.end,
        "time_range": model.time_range,
        "model": model,
        "current_draft": draft.text,
        "style_sample": "• KEY-123 — short outcome-oriented sentence\n• KEY-124 — what changed and why",
    });
    let path = context_file_path(data_dir);
    let json = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("write context file: {e}"))?;
    Ok(path)
}
