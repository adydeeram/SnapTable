// Pure tab-lifecycle logic (SPEC §3.2): preview tabs, identity dedupe, close/reopen.
// The workspace store wraps these; keeping them pure makes them easy to test.

export type TableTab = {
  id: string;
  kind: "table";
  connectionId: string;
  schema: string;
  table: string;
  /** Preview tabs are replaced by the next preview open; any interaction pins them. */
  preview: boolean;
};

export type QueryTab = {
  id: string;
  kind: "query";
  connectionId: string;
  title: string;
  sql: string;
  preview: false;
};

export type Tab = TableTab | QueryTab;

export type Tabs = { tabs: Tab[]; activeId: string | null; closed: Tab[] };

const MAX_CLOSED = 20;

export const tableIdentity = (t: Pick<TableTab, "connectionId" | "schema" | "table">) =>
  `table:${t.connectionId}:${t.schema}.${t.table}`;

export function openTable(
  s: Tabs,
  target: Omit<TableTab, "id" | "kind" | "preview">,
  opts: { preview: boolean; newId: () => string },
): Tabs {
  const identity = tableIdentity(target);
  const existing = s.tabs.find((t) => t.kind === "table" && tableIdentity(t) === identity);
  if (existing) {
    // Re-opening as non-preview pins it.
    const tabs = opts.preview ? s.tabs : s.tabs.map((t) => (t.id === existing.id ? ({ ...t, preview: false } as Tab) : t));
    return { ...s, tabs, activeId: existing.id };
  }
  const tab: TableTab = { id: opts.newId(), kind: "table", ...target, preview: opts.preview };
  const previewIdx = opts.preview ? s.tabs.findIndex((t) => t.preview) : -1;
  if (previewIdx >= 0) {
    const tabs = [...s.tabs];
    tabs[previewIdx] = tab;
    return { ...s, tabs, activeId: tab.id };
  }
  const activeIdx = s.tabs.findIndex((t) => t.id === s.activeId);
  const tabs = [...s.tabs];
  tabs.splice(activeIdx >= 0 ? activeIdx + 1 : tabs.length, 0, tab);
  return { ...s, tabs, activeId: tab.id };
}

export function openQuery(s: Tabs, connectionId: string, sql: string, newId: () => string): Tabs {
  const used = new Set(s.tabs.filter((t) => t.kind === "query").map((t) => (t as QueryTab).title));
  let n = 1;
  while (used.has(`Query ${n}`)) n++;
  const tab: QueryTab = { id: newId(), kind: "query", connectionId, title: `Query ${n}`, sql, preview: false };
  return { ...s, tabs: [...s.tabs, tab], activeId: tab.id };
}

export function closeTab(s: Tabs, id: string): Tabs {
  const idx = s.tabs.findIndex((t) => t.id === id);
  if (idx < 0) return s;
  const tabs = s.tabs.filter((t) => t.id !== id);
  let activeId = s.activeId;
  if (activeId === id) {
    // Activate the right neighbour, else the left one (browser/IDE convention).
    activeId = (tabs[idx] ?? tabs[idx - 1])?.id ?? null;
  }
  const closed = [...s.closed, s.tabs[idx]].slice(-MAX_CLOSED);
  return { tabs, activeId, closed };
}

export function reopenClosed(s: Tabs): Tabs {
  const last = s.closed[s.closed.length - 1];
  if (!last) return s;
  const closed = s.closed.slice(0, -1);
  if (s.tabs.some((t) => t.id === last.id)) return { ...s, closed };
  return { tabs: [...s.tabs, { ...last, preview: false } as Tab], activeId: last.id, closed };
}

export function cycle(s: Tabs, dir: 1 | -1): Tabs {
  if (s.tabs.length === 0) return s;
  const idx = s.tabs.findIndex((t) => t.id === s.activeId);
  const next = (idx + dir + s.tabs.length) % s.tabs.length;
  return { ...s, activeId: s.tabs[next].id };
}

export function pin(s: Tabs, id: string): Tabs {
  if (!s.tabs.some((t) => t.id === id && t.preview)) return s;
  return { ...s, tabs: s.tabs.map((t) => (t.id === id ? ({ ...t, preview: false } as Tab) : t)) };
}

/** Drop tabs whose connection no longer exists (e.g. after restore). */
export function pruneMissing(s: Tabs, connectionIds: Set<string>): Tabs {
  const tabs = s.tabs.filter((t) => connectionIds.has(t.connectionId));
  const activeId = tabs.some((t) => t.id === s.activeId) ? s.activeId : (tabs[0]?.id ?? null);
  return { tabs, activeId, closed: s.closed.filter((t) => connectionIds.has(t.connectionId)) };
}
