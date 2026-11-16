// Command registry (SPEC §3.3). Ids, labels and accelerators come from the shared
// shortcuts.json (also used to build the native menu). Global handlers live here;
// tab-scoped handlers (run query, refresh…) are registered by the active tab.

import { confirm } from "@tauri-apps/plugin-dialog";
import registry from "../../../shortcuts.json";
import { useConnections } from "../stores/connections";
import { disposeTab, getRun } from "../stores/queryRuns";
import { useWorkspace } from "../stores/workspace";

export type CommandDef = { id: string; label: string; menu: string; accelerator?: string };
export const commandDefs: CommandDef[] = registry.commands;

type Handler = () => void | Promise<void>;
const tabHandlers = new Map<string, Partial<Record<string, Handler>>>();

/** Register handlers for a tab; they apply while that tab is active. */
export function registerTabCommands(tabId: string, handlers: Partial<Record<string, Handler>>) {
  tabHandlers.set(tabId, handlers);
  return () => {
    if (tabHandlers.get(tabId) === handlers) tabHandlers.delete(tabId);
  };
}

const ws = () => useWorkspace.getState();

export async function closeTabWithConfirm(id: string) {
  const tab = ws().tabs.find((t) => t.id === id);
  if (!tab) return;
  if (getRun(id)?.status === "running") {
    const ok = await confirm("A query is still running in this tab. Cancel it and close?", { title: "Close tab", kind: "warning" });
    if (!ok) return;
  }
  disposeTab(id);
  ws().closeTab(id);
}

function newQueryTab() {
  const active = ws().tabs.find((t) => t.id === ws().activeId);
  const { connections, live } = useConnections.getState();
  const connectionId =
    active?.connectionId ?? connections.find((c) => live[c.id] === "connected")?.id ?? connections[0]?.id;
  if (!connectionId) {
    ws().openConnectionDialog();
    return;
  }
  ws().openQuery(connectionId);
}

const globalHandlers: Record<string, Handler> = {
  "app.palette": () => ws().setOverlay(ws().overlay === "palette" ? null : "palette"),
  "app.quickOpen": () => ws().setOverlay(ws().overlay === "quickOpen" ? null : "quickOpen"),
  "app.history": () => ws().setOverlay(ws().overlay === "history" ? null : "history"),
  "mode.db": () => ws().setMode("db"),
  "mode.api": () => ws().setMode("api"),
  "view.toggleSidebar": () => ws().toggleSidebar(),
  "conn.new": () => ws().openConnectionDialog(),
  "tab.newQuery": newQueryTab,
  "tab.close": () => {
    const id = ws().activeId;
    if (id) return closeTabWithConfirm(id);
  },
  "tab.reopen": () => ws().reopenClosed(),
  "tab.prev": () => ws().cycle(-1),
  "tab.next": () => ws().cycle(1),
};

export function runCommand(id: string) {
  const activeId = ws().activeId;
  const scoped = activeId ? tabHandlers.get(activeId)?.[id] : undefined;
  const handler = scoped ?? globalHandlers[id];
  if (handler) void handler();
}

export function isCommandAvailable(id: string): boolean {
  const activeId = ws().activeId;
  return Boolean(globalHandlers[id] || (activeId && tabHandlers.get(activeId)?.[id]));
}

const KEY_GLYPHS: Record<string, string> = {
  CmdOrCtrl: "⌘",
  Cmd: "⌘",
  Shift: "⇧",
  Alt: "⌥",
  Option: "⌥",
  Ctrl: "⌃",
  Enter: "↵",
  Backspace: "⌫",
  Escape: "⎋",
};

/** "CmdOrCtrl+Shift+Enter" → "⌘⇧↵" */
export function acceleratorGlyphs(acc: string | undefined): string {
  if (!acc) return "";
  return acc
    .split("+")
    .map((k) => KEY_GLYPHS[k] ?? k.toUpperCase())
    .join("");
}

export const shortcutFor = (id: string) => acceleratorGlyphs(commandDefs.find((c) => c.id === id)?.accelerator);
