// Saved connections, environments, live connection state and the per-connection
// schema object cache.

import { create } from "zustand";
import { api, errorMessage, type Connection, type Environment, type SchemaObject } from "../ipc/api";
import { toast } from "./toasts";

export type LiveState = "disconnected" | "connecting" | "connected" | "error";

type ConnectionsState = {
  connections: Connection[];
  environments: Environment[];
  live: Record<string, LiveState>;
  liveError: Record<string, string | undefined>;
  objects: Record<string, SchemaObject[] | undefined>;
  load: () => Promise<void>;
  connect: (id: string) => Promise<boolean>;
  disconnect: (id: string) => Promise<void>;
  refreshObjects: (id: string) => Promise<void>;
  saved: (c: Connection) => void;
  remove: (id: string) => Promise<void>;
};

const inflight = new Map<string, Promise<boolean>>();

export const useConnections = create<ConnectionsState>((set, get) => ({
  connections: [],
  environments: [],
  live: {},
  liveError: {},
  objects: {},

  load: async () => {
    const [connections, environments, connected] = await Promise.all([
      api.connList(),
      api.envList(),
      api.connConnected(),
    ]);
    const live: Record<string, LiveState> = {};
    for (const id of connected) live[id] = "connected";
    set({ connections, environments, live });
    for (const id of connected) void get().refreshObjects(id);
  },

  /** Connect (deduplicated) and load the schema tree. Resolves to success. */
  connect: (id) => {
    if (get().live[id] === "connected") return Promise.resolve(true);
    const existing = inflight.get(id);
    if (existing) return existing;
    const p = (async () => {
      set((s) => ({ live: { ...s.live, [id]: "connecting" }, liveError: { ...s.liveError, [id]: undefined } }));
      try {
        await api.connConnect(id);
        set((s) => ({ live: { ...s.live, [id]: "connected" } }));
        await get().refreshObjects(id);
        return true;
      } catch (e) {
        const msg = errorMessage(e);
        set((s) => ({ live: { ...s.live, [id]: "error" }, liveError: { ...s.liveError, [id]: msg } }));
        toast.error(`Could not connect to ${nameOf(get().connections, id)}`, msg);
        return false;
      } finally {
        inflight.delete(id);
      }
    })();
    inflight.set(id, p);
    return p;
  },

  disconnect: async (id) => {
    await api.connDisconnect(id);
    set((s) => ({ live: { ...s.live, [id]: "disconnected" }, objects: { ...s.objects, [id]: undefined } }));
  },

  refreshObjects: async (id) => {
    try {
      const objects = await api.schemaObjects(id);
      set((s) => ({ objects: { ...s.objects, [id]: objects } }));
    } catch (e) {
      toast.error("Could not load schema", errorMessage(e));
    }
  },

  saved: (c) =>
    set((s) => {
      const exists = s.connections.some((x) => x.id === c.id);
      const connections = exists ? s.connections.map((x) => (x.id === c.id ? c : x)) : [...s.connections, c];
      connections.sort((a, b) => a.name.localeCompare(b.name));
      // Saving drops the backend's live connection so new settings apply.
      return { connections, live: { ...s.live, [c.id]: "disconnected" }, objects: { ...s.objects, [c.id]: undefined } };
    }),

  remove: async (id) => {
    await api.connDelete(id);
    set((s) => ({ connections: s.connections.filter((c) => c.id !== id) }));
  },
}));

export const nameOf = (connections: Connection[], id: string | undefined) =>
  connections.find((c) => c.id === id)?.name ?? "connection";

export function environmentOf(connectionId: string | undefined): Environment | undefined {
  const { connections, environments } = useConnections.getState();
  const c = connections.find((x) => x.id === connectionId);
  return environments.find((e) => e.id === c?.environmentId);
}

export function useEnvironment(connectionId: string | undefined): Environment | undefined {
  return useConnections((s) => {
    const c = s.connections.find((x) => x.id === connectionId);
    return s.environments.find((e) => e.id === c?.environmentId);
  });
}
