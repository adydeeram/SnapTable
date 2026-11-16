// Query history (⌘Y): full-text search across past statements; Enter opens one in a
// new query tab on its connection.

import { useEffect, useRef, useState } from "react";
import { api, errorMessage, type HistoryEntry } from "../../ipc/api";
import { nameOf, useConnections } from "../../stores/connections";
import { useWorkspace } from "../../stores/workspace";

export function HistoryPanel() {
  const [query, setQuery] = useState("");
  const [onlyCurrent, setOnlyCurrent] = useState(false);
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [index, setIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const connections = useConnections((s) => s.connections);
  const activeConn = useWorkspace((s) => s.tabs.find((t) => t.id === s.activeId)?.connectionId ?? null);
  const close = () => useWorkspace.getState().setOverlay(null);

  useEffect(() => inputRef.current?.focus(), []);

  useEffect(() => {
    let stale = false;
    const t = setTimeout(() => {
      api
        .historySearch(onlyCurrent ? activeConn : null, query, 300)
        .then((r) => !stale && (setEntries(r), setError(null), setIndex(0)))
        .catch((e) => !stale && setError(errorMessage(e)));
    }, 120);
    return () => {
      stale = true;
      clearTimeout(t);
    };
  }, [query, onlyCurrent, activeConn]);

  const open = (e: HistoryEntry | undefined) => {
    if (!e?.connectionId) return;
    close();
    useWorkspace.getState().openQuery(e.connectionId, e.sql);
  };

  return (
    <div className="fixed inset-0 z-40 flex items-start justify-center bg-black/15 pt-[10vh]" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div className="flex max-h-[70vh] w-[760px] flex-col overflow-hidden rounded-xl border border-line bg-elev shadow-[var(--shadow)]" role="dialog" aria-label="Query history">
        <div className="flex items-center gap-3 border-b border-line px-4">
          <input
            ref={inputRef}
            className="h-11 flex-1 bg-transparent text-[14px] outline-none placeholder:text-faint"
            placeholder="Search query history…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") (e.preventDefault(), setIndex((i) => Math.min(entries.length - 1, i + 1)));
              else if (e.key === "ArrowUp") (e.preventDefault(), setIndex((i) => Math.max(0, i - 1)));
              else if (e.key === "Enter") open(entries[index]);
              else if (e.key === "Escape") close();
            }}
          />
          {activeConn && (
            <label className="flex items-center gap-1.5 text-[12px] text-muted">
              <input type="checkbox" checked={onlyCurrent} onChange={(e) => setOnlyCurrent(e.target.checked)} />
              This connection only
            </label>
          )}
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-1.5">
          {error && <div className="p-3 text-[12px] text-danger">{error}</div>}
          {!error && entries.length === 0 && <div className="px-3 py-8 text-center text-[12px] text-faint">No history yet. Production-tagged connections aren't recorded.</div>}
          {entries.map((e, i) => (
            <div
              key={e.id}
              onMouseMove={() => setIndex(i)}
              onClick={() => open(e)}
              className={`rounded-md px-3 py-2 ${i === index ? "bg-selected" : ""}`}
            >
              <div className="truncate font-mono text-[12px]">{e.sql.replace(/\s+/g, " ")}</div>
              <div className="mt-0.5 flex gap-2 text-[11px] text-faint">
                <span className={e.status === "error" ? "text-danger" : e.status === "cancelled" ? "text-warning" : ""}>{e.status}</span>
                <span>· {nameOf(connections, e.connectionId ?? undefined)}</span>
                <span>· {e.elapsedMs} ms</span>
                {e.rowsAffected !== null && <span>· {e.rowsAffected} affected</span>}
                <span className="ml-auto">{timeAgo(e.executedAt)}</span>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function timeAgo(ms: number) {
  const s = Math.round((Date.now() - ms) / 1000);
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
  return new Date(ms).toLocaleDateString();
}
