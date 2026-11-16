import { closeTabWithConfirm, runCommand, shortcutFor } from "../app/commands";
import { IconClose, IconPlus, IconQuery, IconTable } from "../lib/icons";
import { useConnections } from "../stores/connections";
import { useRun } from "../stores/queryRuns";
import type { Tab } from "../stores/tabs";
import { useWorkspace } from "../stores/workspace";

export function TabStrip() {
  const tabs = useWorkspace((s) => s.tabs);
  const activeId = useWorkspace((s) => s.activeId);
  if (tabs.length === 0) return null;
  return (
    <div role="tablist" className="flex h-9 shrink-0 items-stretch overflow-x-auto border-b border-line bg-surface">
      {tabs.map((t) => (
        <TabButton key={t.id} tab={t} active={t.id === activeId} />
      ))}
      <button className="btn-ghost my-1 ml-1" onClick={() => runCommand("tab.newQuery")} title={`New query ${shortcutFor("tab.newQuery")}`}>
        <IconPlus />
      </button>
    </div>
  );
}

function TabButton({ tab, active }: { tab: Tab; active: boolean }) {
  const ws = useWorkspace.getState();
  const env = useConnections((s) => {
    const c = s.connections.find((x) => x.id === tab.connectionId);
    return s.environments.find((e) => e.id === c?.environmentId);
  });
  const run = useRun(tab.id);
  const running = run?.status === "running";
  const title = tab.kind === "table" ? tab.table : tab.title;
  return (
    <div
      role="tab"
      aria-selected={active}
      onMouseDown={(e) => e.button === 0 && ws.activate(tab.id)}
      onDoubleClick={() => ws.pin(tab.id)}
      onAuxClick={(e) => e.button === 1 && void closeTabWithConfirm(tab.id)}
      className={`group relative flex max-w-56 min-w-28 items-center gap-1.5 border-r border-line px-3 text-[12px] ${
        active ? "bg-bg text-fg" : "text-muted hover:bg-hover"
      }`}
      title={tab.kind === "table" ? `${tab.schema}.${tab.table}` : tab.title}
    >
      <span
        className="absolute inset-x-0 bottom-0 h-[2px]"
        style={{ background: active ? (env?.color ?? "var(--accent)") : env && env.protectionLevel !== "none" ? env.color : "transparent", opacity: active ? 1 : 0.5 }}
      />
      {running ? <span className="spinner shrink-0" /> : tab.kind === "table" ? <IconTable className="shrink-0" /> : <IconQuery className="shrink-0" />}
      <span className={`truncate ${tab.preview ? "italic" : ""}`}>{title}</span>
      <button
        className="ml-auto rounded p-0.5 text-faint opacity-0 group-hover:opacity-100 hover:bg-hover hover:text-fg aria-selected:opacity-100"
        style={{ opacity: active ? 1 : undefined }}
        onMouseDown={(e) => e.stopPropagation()}
        onClick={() => void closeTabWithConfirm(tab.id)}
        aria-label="Close tab"
      >
        <IconClose size={12} />
      </button>
    </div>
  );
}
