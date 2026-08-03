import type { Tab } from "../types";

const TABS: { id: Tab; label: string }[] = [
  { id: "in_progress", label: "In Progress" },
  { id: "recent", label: "Recent" },
  { id: "standup", label: "Standup" },
];

export function TabBar({ tab, onChange }: { tab: Tab; onChange: (t: Tab) => void }) {
  return (
    <nav className="tabbar" role="tablist">
      {TABS.map((t) => (
        <button
          key={t.id}
          role="tab"
          aria-selected={tab === t.id}
          className={`tab ${tab === t.id ? "tab-active" : ""}`}
          onClick={() => onChange(t.id)}
        >
          {t.label}
        </button>
      ))}
    </nav>
  );
}
