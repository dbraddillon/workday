import { useCallback, useEffect, useState } from "react";
import { api, copyToClipboard } from "../api";
import type { RecentRange, StandupModel } from "../types";
import { EmptyState } from "./EmptyState";

const RANGES: { id: RecentRange; label: string }[] = [
  { id: "standup", label: "Since standup" },
  { id: "today", label: "Today" },
  { id: "24h", label: "24h" },
  { id: "3d", label: "3 days" },
  { id: "7d", label: "7 days" },
];

export function StandupTab({
  dataVersion,
  defaultFormatter,
  aiPolishDefault,
}: {
  dataVersion: number;
  defaultFormatter: string;
  aiPolishDefault: boolean;
}) {
  const [range, setRange] = useState<RecentRange>("standup");
  const [model, setModel] = useState<StandupModel | null>(null);
  const [draft, setDraft] = useState<string>("");
  const [aiPolish, setAiPolish] = useState(aiPolishDefault);
  const [aiAvailable, setAiAvailable] = useState(false);
  const [fallbackNote, setFallbackNote] = useState<string | null>(null);
  const [generating, setGenerating] = useState(false);
  const [copied, setCopied] = useState(false);

  const rebuild = useCallback(async () => {
    setDraft("");
    setModel(null);
    setFallbackNote(null);
    const m = await api.buildStandupModel(range);
    setModel(m);
  }, [range]);

  useEffect(() => {
    rebuild();
  }, [rebuild, dataVersion]);

  // Detect whether AI polish is even possible on this machine (a `claude` CLI
  // on PATH). If not, we hide the toggle entirely rather than offer an option
  // that would silently no-op.
  useEffect(() => {
    api.aiPolishAvailable().then(setAiAvailable).catch(() => setAiAvailable(false));
  }, []);

  const isThread = defaultFormatter === "thread";

  const setNarrative = (
    field: "doing" | "pairing" | "blocker" | "post_scrum",
    value: string,
  ) => {
    if (!model) return;
    setModel({ ...model, narrative: { ...model.narrative, [field]: value } });
  };

  const toggleItem = (sectionKey: string, issueKey: string) => {
    if (!model) return;
    setModel({
      ...model,
      sections: model.sections.map((s) =>
        s.key !== sectionKey
          ? s
          : {
              ...s,
              items: s.items.map((it) =>
                it.issue_key === issueKey ? { ...it, included: !it.included } : it,
              ),
            },
      ),
    });
  };

  const generate = async () => {
    if (!model) return;
    setGenerating(true);
    setCopied(false);
    setFallbackNote(null);
    try {
      const d = await api.generateStandup(model, defaultFormatter, aiPolish && aiAvailable);
      setDraft(d.text);
      // Surface a silent AI fallback so the user knows they got the plain draft.
      if (d.ai_polish_fell_back) {
        setFallbackNote(`AI polish unavailable — used the plain draft. (${d.ai_polish_fell_back})`);
      }
    } catch (e) {
      setDraft(`Could not generate: ${String(e)}`);
    } finally {
      setGenerating(false);
    }
  };

  const copy = async () => {
    await copyToClipboard(draft);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  const hasItems = model && model.sections.some((s) => s.items.length > 0);

  return (
    <div className="standup">
      <div className="segmented">
        {RANGES.map((r) => (
          <button
            key={r.id}
            className={`seg ${range === r.id ? "seg-active" : ""}`}
            onClick={() => setRange(r.id)}
          >
            {r.label}
          </button>
        ))}
      </div>

      {model === null ? (
        <div className="loading">Building…</div>
      ) : !hasItems ? (
        <EmptyState
          title="No activity in this window"
          hint="Pick a wider range, or sync first."
        />
      ) : (
        <>
          <div className="standup-items">
            {model.sections.map((section) => (
              <div key={section.key} className="standup-section">
                <div className="standup-section-title">{section.title}</div>
                {section.items.map((item) => (
                  <label key={item.issue_key} className="standup-item">
                    <input
                      type="checkbox"
                      checked={item.included}
                      onChange={() => toggleItem(section.key, item.issue_key)}
                    />
                    <span className="standup-item-key">{item.issue_key}</span>
                    <span className="standup-item-summary">{item.summary}</span>
                  </label>
                ))}
              </div>
            ))}
          </div>

          {isThread && (
            <div className="standup-narrative">
              <label className="narrative-field">
                <span title=":city_sunrise: How are you doing?">🌅 Doing</span>
                <input
                  type="text"
                  value={model.narrative.doing}
                  onChange={(e) => setNarrative("doing", e.target.value)}
                />
              </label>
              <label className="narrative-field">
                <span title=":two-peas-in-a-pod: Any pairing opportunities?">🫛 Pairing</span>
                <input
                  type="text"
                  value={model.narrative.pairing}
                  onChange={(e) => setNarrative("pairing", e.target.value)}
                />
              </label>
              <label className="narrative-field">
                <span title=":blocker: Any blockers?">🚧 Blockers</span>
                <input
                  type="text"
                  value={model.narrative.blocker ?? ""}
                  onChange={(e) => setNarrative("blocker", e.target.value)}
                  disabled={model.blockers.length > 0}
                  placeholder={
                    model.blockers.length > 0
                      ? model.blockers.join("; ")
                      : "e.g. Nope"
                  }
                />
              </label>
              {model.blockers.length > 0 && (
                <div className="narrative-blockers" title="Derived from Jira status">
                  From Jira: {model.blockers.join("; ")}
                </div>
              )}
              <label className="narrative-field">
                <span title=":high-five: Anything for post scrum?">🙌 Post scrum</span>
                <input
                  type="text"
                  value={model.narrative.post_scrum}
                  onChange={(e) => setNarrative("post_scrum", e.target.value)}
                />
              </label>
            </div>
          )}

          <div className="standup-controls">
            {aiAvailable ? (
              <label
                className="ai-toggle"
                title="Optionally refine the draft with your local `claude` CLI. The plain draft is used if it fails."
              >
                <input
                  type="checkbox"
                  checked={aiPolish}
                  onChange={(e) => setAiPolish(e.target.checked)}
                />
                AI polish
              </label>
            ) : (
              <span />
            )}
            <button className="btn-primary" onClick={generate} disabled={generating}>
              {generating ? "Generating…" : draft ? "Regenerate" : "Generate draft"}
            </button>
          </div>
        </>
      )}

      {fallbackNote && <div className="standup-note">{fallbackNote}</div>}

      {draft && (
        <div className="draft">
          <textarea
            className="draft-text"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            spellCheck={false}
          />
          <div className="draft-actions">
            <button className="btn-primary" onClick={copy}>
              {copied ? "Copied ✓" : "Copy"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
