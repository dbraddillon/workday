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
  const [generating, setGenerating] = useState(false);
  const [copied, setCopied] = useState(false);

  const rebuild = useCallback(async () => {
    setDraft("");
    setModel(null);
    const m = await api.buildStandupModel(range);
    setModel(m);
  }, [range]);

  useEffect(() => {
    rebuild();
  }, [rebuild, dataVersion]);

  const isThread = defaultFormatter === "thread";

  const setNarrative = (
    field: "doing" | "pairing" | "post_scrum",
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
    try {
      const d = await api.generateStandup(model, defaultFormatter, aiPolish);
      setDraft(d.text);
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
                <span title=":high-five: Anything for post scrum?">🙌 Post scrum</span>
                <input
                  type="text"
                  value={model.narrative.post_scrum}
                  onChange={(e) => setNarrative("post_scrum", e.target.value)}
                />
              </label>
              {model.blockers.length > 0 && (
                <div className="narrative-blockers" title="Derived from Jira status">
                  🚧 Blockers: {model.blockers.join("; ")}
                </div>
              )}
            </div>
          )}

          <div className="standup-controls">
            <label className="ai-toggle" title="Polish the draft with your local Claude CLI (Bedrock)">
              <input
                type="checkbox"
                checked={aiPolish}
                onChange={(e) => setAiPolish(e.target.checked)}
              />
              AI polish
            </label>
            <button className="btn-primary" onClick={generate} disabled={generating}>
              {generating ? "Generating…" : draft ? "Regenerate" : "Generate draft"}
            </button>
          </div>
        </>
      )}

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
