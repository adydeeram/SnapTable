// Workspace layout + tabs. Tab descriptors (not results) persist to the metadata DB
// and are restored on launch; connections reconnect lazily when a tab is focused.

import { create } from "zustand";
import { api } from "../ipc/api";
import * as T from "./tabs";

export type Mode = "db" | "api";
export type Overlay = null | "palette" | "quickOpen" | "history";

type WorkspaceState = T.Tabs & {
  mode: Mode;
  sidebarVisible: boolean;
  overlay: Overlay;
  connectionDialog: { open: boolean; editId?: string };
  restored: boolean;

  openTable: (target: Omit<T.TableTab, "id" | "kind" | "preview">, opts?: { preview?: boolean }) => void;
  openQuery: (connectionId: string, sql?: string) => void;
  closeTab: (id: string) => void;
  reopenClosed: () => void;
  activate: (id: string) => void;
  cycle: (dir: 1 | -1) => void;
  pin: (id: string) => void;
  updateQuery: (id: string, patch: Partial<Pick<T.QueryTab, "sql" | "connectionId">>) => void;
  setMode: (mode: Mode) => void;
  toggleSidebar: () => void;
  setOverlay: (overlay: Overlay) => void;
  openConnectionDialog: (editId?: string) => void;
  closeConnectionDialog: () => void;
  restore: (connectionIds: Set<string>) => Promise<void>;
};

const newId = () => crypto.randomUUID();
const tabsOf = (s: T.Tabs): T.Tabs => ({ tabs: s.tabs, activeId: s.activeId, closed: s.closed });

export const useWorkspace = create<WorkspaceState>((set, get) => ({
  tabs: [],
  activeId: null,
  closed: [],
  mode: "db",
  sidebarVisible: true,
  overlay: null,
  connectionDialog: { open: false },
  restored: false,

  openTable: (target, opts) => set((s) => T.openTable(tabsOf(s), target, { preview: opts?.preview ?? true, newId })),
  openQuery: (connectionId, sql = "") => set((s) => ({ ...T.openQuery(tabsOf(s), connectionId, sql, newId), mode: "db" })),
  closeTab: (id) => set((s) => T.closeTab(tabsOf(s), id)),
  reopenClosed: () => set((s) => T.reopenClosed(tabsOf(s))),
  activate: (id) => set({ activeId: id }),
  cycle: (dir) => set((s) => T.cycle(tabsOf(s), dir)),
  pin: (id) => set((s) => T.pin(tabsOf(s), id)),
  updateQuery: (id, patch) =>
    set((s) => ({ tabs: s.tabs.map((t) => (t.id === id && t.kind === "query" ? { ...t, ...patch } : t)) })),
  setMode: (mode) => set({ mode }),
  toggleSidebar: () => set((s) => ({ sidebarVisible: !s.sidebarVisible })),
  setOverlay: (overlay) => set({ overlay }),
  openConnectionDialog: (editId) => set({ connectionDialog: { open: true, editId } }),
  closeConnectionDialog: () => set({ connectionDialog: { open: false } }),

  restore: async (connectionIds) => {
    try {
      const saved = await api.workspaceGet<{ tabs: T.Tab[]; activeId: string | null }>("tabs");
      if (saved?.tabs) {
        const pruned = T.pruneMissing({ tabs: saved.tabs, activeId: saved.activeId, closed: [] }, connectionIds);
        set({ ...pruned });
      }
    } finally {
      set({ restored: true });
    }
    // Persist tab descriptors (debounced) from now on.
    let timer: ReturnType<typeof setTimeout> | undefined;
    useWorkspace.subscribe((s, prev) => {
      if (s.tabs === prev.tabs && s.activeId === prev.activeId) return;
      clearTimeout(timer);
      timer = setTimeout(() => {
        const { tabs, activeId } = get();
        void api.workspaceSet("tabs", { tabs, activeId });
      }, 400);
    });
  },
}));

export const useActiveTab = () => useWorkspace((s) => s.tabs.find((t) => t.id === s.activeId));
