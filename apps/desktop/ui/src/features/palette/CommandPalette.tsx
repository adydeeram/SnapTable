// ⌘K command palette and ⌘P quick-open (SPEC §3.3): fuzzy search over commands,
// connections and every loaded table/view.

import { useEffect, useMemo, useRef, useState } from "react";
import { commandDefs, isCommandAvailable, runCommand, shortcutFor } from "../../app/commands";
import { IconBolt, IconDatabase, IconTable, IconView } from "../../lib/icons";
import { fuzzyScore } from "../../lib/fuzzy";
import { useConnections } from "../../stores/connections";
import { useWorkspace } from "../../stores/workspace";

type Item = { id: string; label: string; detail?: string; hint?: string; icon: React.ReactNode; run: () => void };

export function CommandPalette({ mode }: { mode: "palette" | "quickOpen" }) {
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const connections = useConnections((s) => s.connections);
  const objects = useConnections((s) => s.objects);
  const live = useConnections((s) => s.live);
  const close = () => useWorkspace.getState().setOverlay(null);

  useEffect(() => inputRef.current?.focus(), [mode]);

  const items = useMemo<Item[]>(() => {
    const ws = useWorkspace.getState();
    const tables: Item[] = connections.flatMap((c) =>
      (objects[c.id] ?? []).map((o) => ({
        id: `table:${c.id}:${o.schema}.${o.name}`,
        label: o.schema === "public" || o.schema === "main" ? o.name : `${o.schema}.${o.name}`,
        detail: c.name,
        icon: o.kind === "table" ? <IconTable /> : <IconView />,
        run: () => ws.openTable({ connectionId: c.id, schema: o.schema, table: o.name }, { preview: false }),
      })),
    );
    if (mode === "quickOpen") return tables;

    const commands: Item[] = commandDefs
      .filter((c) => c.id !== "app.palette" && isCommandAvailable(c.id))
      .map((c) => ({ id: `cmd:${c.id}`, label: c.label.replace(/…$/, ""), hint: shortcutFor(c.id), detail: c.menu, icon: <IconBolt />, run: () => runCommand(c.id) }));
    const conns: Item[] = connections.flatMap((c) => [
      live[c.id] === "connected"
        ? { id: `disc:${c.id}`, label: `Disconnect ${c.name}`, detail: "Connection", icon: <IconDatabase />, run: () => void useConnections.getState().disconnect(c.id) }
        : { id: `conn:${c.id}`, label: `Connect to ${c.name}`, detail: "Connection", icon: <IconDatabase />, run: () => void useConnections.getState().connect(c.id) },
      { id: `query:${c.id}`, label: `New Query on ${c.name}`, detail: "Connection", icon: <IconDatabase />, run: () => ws.openQuery(c.id) },
      { id: `edit:${c.id}`, label: `Edit ${c.name}`, detail: "Connection", icon: <IconDatabase />, run: () => ws.openConnectionDialog(c.id) },
    ]);
    return [...commands, ...conns, ...tables];
  }, [mode, connections, objects, live]);

  const results = useMemo(() => {
    if (!query.trim()) return items.slice(0, 80);
    return items
      .map((it) => ({ it, s: fuzzyScore(query, `${it.label} ${it.detail ?? ""}`) }))
      .filter((x): x is { it: Item; s: number } => x.s !== null)
      .sort((a, b) => b.s - a.s)
      .slice(0, 80)
      .map((x) => x.it);
  }, [items, query]);

  useEffect(() => setIndex(0), [query, mode]);
  useEffect(() => {
    listRef.current?.querySelector(`[data-idx="${index}"]`)?.scrollIntoView({ block: "nearest" });
  }, [index]);

  const choose = (it: Item | undefined) => {
    if (!it) return;
    close();
    it.run();
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setIndex((i) => Math.min(results.length - 1, i + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setIndex((i) => Math.max(0, i - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      choose(results[index]);
    } else if (e.key === "Escape") {
      e.preventDefault();
      close();
    }
  };

  return (
    <div className="fixed inset-0 z-40 flex items-start justify-center bg-black/15 pt-[12vh]" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div className="w-[620px] overflow-hidden rounded-xl border border-line bg-elev shadow-[var(--shadow)]" role="dialog" aria-label="Command palette">
        <input
          ref={inputRef}
          className="h-11 w-full border-b border-line bg-transparent px-4 text-[14px] outline-none placeholder:text-faint"
          placeholder={mode === "quickOpen" ? "Open table or view…" : "Type a command, connection or table…"}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={onKeyDown}
          role="combobox"
          aria-expanded
          aria-controls="palette-list"
        />
        <div ref={listRef} id="palette-list" role="listbox" className="max-h-[50vh] overflow-y-auto p-1.5">
          {results.length === 0 && (
            <div className="px-3 py-6 text-center text-[12px] text-faint">
              {mode === "quickOpen" && items.length === 0 ? "Connect to a database to search its tables." : "No matches"}
            </div>
          )}
          {results.map((it, i) => (
            <div
              key={it.id}
              data-idx={i}
              role="option"
              aria-selected={i === index}
              onMouseMove={() => setIndex(i)}
              onClick={() => choose(it)}
              className={`flex h-8 items-center gap-2.5 rounded-md px-2.5 text-[13px] ${i === index ? "bg-accent text-white" : ""}`}
            >
              <span className={i === index ? "text-white" : "text-muted"}>{it.icon}</span>
              <span className="truncate">{it.label}</span>
              {it.detail && <span className={`truncate text-[11.5px] ${i === index ? "text-white/75" : "text-faint"}`}>{it.detail}</span>}
              <span className="flex-1" />
              {it.hint && <span className={`text-[11.5px] ${i === index ? "text-white/85" : "text-faint"}`}>{it.hint}</span>}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
