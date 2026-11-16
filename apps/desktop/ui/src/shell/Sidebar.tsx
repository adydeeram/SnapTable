// Connection tree (SPEC §3.1): Environment → Connection → Schema → Tables/Views.

import { confirm } from "@tauri-apps/plugin-dialog";
import { useMemo, useState } from "react";
import { runCommand, shortcutFor } from "../app/commands";
import type { Connection, SchemaObject } from "../ipc/api";
import { errorMessage } from "../ipc/api";
import { IconChevron, IconDatabase, IconEdit, IconPlug, IconPlus, IconQuery, IconRefresh, IconTable, IconTrash, IconView } from "../lib/icons";
import { useConnections } from "../stores/connections";
import { disposeTab, forgetConnectionSessions } from "../stores/queryRuns";
import { toast } from "../stores/toasts";
import { useWorkspace } from "../stores/workspace";

export function Sidebar() {
  const connections = useConnections((s) => s.connections);
  const environments = useConnections((s) => s.environments);
  const [filter, setFilter] = useState("");

  const groups = useMemo(() => {
    const byEnv = environments.map((env) => ({ env, items: connections.filter((c) => c.environmentId === env.id) }));
    const none = connections.filter((c) => !environments.some((e) => e.id === c.environmentId));
    return [...byEnv.filter((g) => g.items.length > 0), ...(none.length ? [{ env: undefined, items: none }] : [])];
  }, [connections, environments]);

  return (
    <aside className="flex w-64 shrink-0 flex-col border-r border-line bg-sidebar">
      <div className="flex items-center gap-1 px-3 pt-2 pb-1">
        <span className="flex-1 text-[11px] font-semibold tracking-wide text-faint uppercase">Connections</span>
        <button className="btn-ghost h-6 px-1.5" onClick={() => runCommand("conn.new")} title={`New connection ${shortcutFor("conn.new")}`}>
          <IconPlus />
        </button>
      </div>
      <div className="px-2 pb-2">
        <input className="field h-6 text-[12px]" placeholder="Filter tables…" value={filter} onChange={(e) => setFilter(e.target.value)} />
      </div>
      <nav className="min-h-0 flex-1 overflow-y-auto px-1 pb-3" role="tree">
        {connections.length === 0 && (
          <div className="px-3 py-6 text-center text-[12px] text-faint">
            No connections yet.
            <br />
            <button className="mt-2 text-accent hover:underline" onClick={() => runCommand("conn.new")}>
              Add one
            </button>
          </div>
        )}
        {groups.map(({ env, items }) => (
          <div key={env?.id ?? "none"} className="mb-1">
            <div className="flex items-center gap-1.5 px-2 pt-2 pb-0.5 text-[10.5px] font-semibold tracking-wide text-faint uppercase">
              <span className="h-1.5 w-1.5 rounded-full" style={{ background: env?.color ?? "var(--fg-faint)" }} />
              {env?.name ?? "Other"}
            </div>
            {items.map((c) => (
              <ConnectionNode key={c.id} connection={c} filter={filter.trim().toLowerCase()} />
            ))}
          </div>
        ))}
      </nav>
    </aside>
  );
}

function ConnectionNode({ connection, filter }: { connection: Connection; filter: string }) {
  const live = useConnections((s) => s.live[connection.id] ?? "disconnected");
  const error = useConnections((s) => s.liveError[connection.id]);
  const objects = useConnections((s) => s.objects[connection.id]);
  const [open, setOpen] = useState(false);
  const expanded = open || (filter !== "" && !!objects);

  const toggle = async () => {
    if (open) return setOpen(false);
    setOpen(true);
    if (live !== "connected") await useConnections.getState().connect(connection.id);
  };

  const disconnect = async () => {
    const ws = useWorkspace.getState();
    for (const t of ws.tabs.filter((t) => t.connectionId === connection.id && t.kind === "query")) disposeTab(t.id);
    forgetConnectionSessions(connection.id);
    await useConnections.getState().disconnect(connection.id);
    setOpen(false);
  };

  const remove = async () => {
    const ok = await confirm(`Delete “${connection.name}”? Its saved password is removed from the Keychain.`, {
      title: "Delete connection",
      kind: "warning",
    });
    if (!ok) return;
    try {
      const ws = useWorkspace.getState();
      for (const t of ws.tabs.filter((t) => t.connectionId === connection.id)) {
        disposeTab(t.id);
        ws.closeTab(t.id);
      }
      await useConnections.getState().remove(connection.id);
    } catch (e) {
      toast.error("Delete failed", errorMessage(e));
    }
  };

  const bySchema = useMemo(() => {
    const m = new Map<string, SchemaObject[]>();
    for (const o of objects ?? []) {
      if (filter && !o.name.toLowerCase().includes(filter)) continue;
      const list = m.get(o.schema) ?? [];
      list.push(o);
      m.set(o.schema, list);
    }
    return m;
  }, [objects, filter]);

  const dot =
    live === "connected" ? "var(--success)" : live === "error" ? "var(--danger)" : live === "connecting" ? "var(--warning)" : "transparent";

  return (
    <div role="treeitem" aria-expanded={expanded}>
      <div
        className="group flex h-7 items-center gap-1 rounded-md px-1.5 text-[12.5px] hover:bg-hover"
        onClick={() => void toggle()}
        title={error ?? connectionSubtitle(connection)}
      >
        <IconChevron size={11} className={`shrink-0 text-faint transition-transform ${expanded ? "rotate-90" : ""}`} />
        <span className="relative">
          <IconDatabase className="text-muted" />
          <span className="absolute -right-0.5 -bottom-0.5 h-1.5 w-1.5 rounded-full" style={{ background: dot }} />
        </span>
        <span className="ml-1 min-w-0 flex-1 truncate font-medium">{connection.name}</span>
        {live === "connecting" && <span className="spinner" />}
        <span className="hidden items-center group-hover:flex" onClick={(e) => e.stopPropagation()}>
          <IconAction title="New query" onClick={() => useWorkspace.getState().openQuery(connection.id)}>
            <IconQuery size={12} />
          </IconAction>
          {live === "connected" && (
            <IconAction title="Refresh schema" onClick={() => void useConnections.getState().refreshObjects(connection.id)}>
              <IconRefresh size={12} />
            </IconAction>
          )}
          <IconAction title="Edit" onClick={() => useWorkspace.getState().openConnectionDialog(connection.id)}>
            <IconEdit size={12} />
          </IconAction>
          {live === "connected" ? (
            <IconAction title="Disconnect" onClick={() => void disconnect()}>
              <IconPlug size={12} />
            </IconAction>
          ) : (
            <IconAction title="Delete" onClick={() => void remove()}>
              <IconTrash size={12} />
            </IconAction>
          )}
        </span>
      </div>
      {expanded && (
        <div role="group" className="ml-3 border-l border-line pl-1">
          {live === "error" && <div className="px-2 py-1 text-[11.5px] text-danger">{error}</div>}
          {live === "connecting" && !objects && <div className="px-2 py-1 text-[11.5px] text-faint">Connecting…</div>}
          {objects && bySchema.size === 0 && <div className="px-2 py-1 text-[11.5px] text-faint">{filter ? "No matches" : "No tables"}</div>}
          {[...bySchema.entries()].map(([schema, items]) =>
            bySchema.size === 1 && (schema === "public" || schema === "main") ? (
              <ObjectList key={schema} connectionId={connection.id} items={items} />
            ) : (
              <SchemaNode key={schema} connectionId={connection.id} schema={schema} items={items} forceOpen={!!filter} />
            ),
          )}
        </div>
      )}
    </div>
  );
}

function SchemaNode({ connectionId, schema, items, forceOpen }: { connectionId: string; schema: string; items: SchemaObject[]; forceOpen: boolean }) {
  const [open, setOpen] = useState(schema === "public" || schema === "main");
  const expanded = open || forceOpen;
  return (
    <div role="treeitem" aria-expanded={expanded}>
      <div className="flex h-6 items-center gap-1 rounded-md px-1.5 text-[12px] text-muted hover:bg-hover" onClick={() => setOpen(!open)}>
        <IconChevron size={10} className={`text-faint transition-transform ${expanded ? "rotate-90" : ""}`} />
        <span className="truncate">{schema}</span>
        <span className="ml-auto text-[10.5px] text-faint">{items.length}</span>
      </div>
      {expanded && (
        <div className="ml-2.5">
          <ObjectList connectionId={connectionId} items={items} />
        </div>
      )}
    </div>
  );
}

function ObjectList({ connectionId, items }: { connectionId: string; items: SchemaObject[] }) {
  const activeKey = useWorkspace((s) => {
    const t = s.tabs.find((x) => x.id === s.activeId);
    return t?.kind === "table" ? `${t.connectionId}:${t.schema}.${t.table}` : null;
  });
  const open = (o: SchemaObject, preview: boolean) =>
    useWorkspace.getState().openTable({ connectionId, schema: o.schema, table: o.name }, { preview });
  return (
    <>
      {items.map((o) => {
        const active = activeKey === `${connectionId}:${o.schema}.${o.name}`;
        return (
          <div
            key={`${o.schema}.${o.name}`}
            role="treeitem"
            className={`flex h-6 items-center gap-1.5 rounded-md px-2 text-[12px] ${active ? "bg-selected text-fg" : "hover:bg-hover"}`}
            onClick={() => open(o, true)}
            onDoubleClick={() => open(o, false)}
            title={`${o.schema}.${o.name}`}
          >
            {o.kind === "table" ? <IconTable size={12} className="shrink-0 text-muted" /> : <IconView size={12} className="shrink-0 text-muted" />}
            <span className="truncate">{o.name}</span>
          </div>
        );
      })}
    </>
  );
}

function IconAction({ title, onClick, children }: { title: string; onClick: () => void; children: React.ReactNode }) {
  return (
    <button className="rounded p-1 text-faint hover:bg-hover hover:text-fg" title={title} aria-label={title} onClick={onClick}>
      {children}
    </button>
  );
}

function connectionSubtitle(c: Connection) {
  if (c.engine === "sqlite") return c.sqlitePath ?? "";
  return `${c.username ? c.username + "@" : ""}${c.host ?? ""}${c.port ? ":" + c.port : ""}/${c.database ?? ""}`;
}
