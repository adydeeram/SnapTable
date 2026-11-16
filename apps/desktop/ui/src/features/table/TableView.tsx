// Read-only table browser (Phase 1): server-side paging, sort and raw WHERE filter.

import { useCallback, useEffect, useRef, useState } from "react";
import { registerTabCommands, shortcutFor } from "../../app/commands";
import { DataGrid, type SortState } from "../../grid/DataGrid";
import { RowInspector } from "../../grid/RowInspector";
import { api } from "../../ipc/api";
import { IconChevronLeft, IconChevronRight, IconFilter, IconRefresh, IconSidebarRight } from "../../lib/icons";
import { useConnections } from "../../stores/connections";
import { browseTable, cancelRun, useRun } from "../../stores/queryRuns";
import type { TableTab } from "../../stores/tabs";
import { useWorkspace } from "../../stores/workspace";

const PAGE = 1000;

export function TableView({ tab, active }: { tab: TableTab; active: boolean }) {
  const run = useRun(tab.id);
  const live = useConnections((s) => s.live[tab.connectionId]);
  const [offset, setOffset] = useState(0);
  const [sort, setSort] = useState<SortState>(null);
  const [filterDraft, setFilterDraft] = useState("");
  const [filter, setFilter] = useState<string | null>(null);
  const [total, setTotal] = useState<number | null | "error">(null);
  const [inspector, setInspector] = useState(false);
  const [selectedRow, setSelectedRow] = useState<number | null>(null);
  const loadedOnce = useRef(false);
  const filterRef = useRef<HTMLInputElement>(null);

  const load = useCallback(() => {
    loadedOnce.current = true;
    void browseTable(tab.id, {
      connectionId: tab.connectionId,
      schema: tab.schema,
      table: tab.table,
      offset,
      limit: PAGE,
      orderBy: sort,
      filter,
    });
    setTotal(null);
    // The count needs the live connection that browseTable establishes lazily.
    void useConnections
      .getState()
      .connect(tab.connectionId)
      .then((ok) => (ok ? api.tableCount(tab.connectionId, tab.schema, tab.table, filter) : Promise.reject()))
      .then(setTotal)
      .catch(() => setTotal("error"));
  }, [tab.id, tab.connectionId, tab.schema, tab.table, offset, sort, filter]);

  // Load lazily: the first time the tab is visible, then whenever paging/sort/filter change.
  // (Restored background tabs therefore don't connect until focused.)
  useEffect(() => {
    if (active || loadedOnce.current) load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [load]);
  useEffect(() => {
    if (active && !loadedOnce.current) load();
  }, [active, load]);

  // A reconnect (e.g. after editing the connection) re-runs a failed page.
  const prevLive = useRef(live);
  useEffect(() => {
    if (prevLive.current !== "connected" && live === "connected" && loadedOnce.current && run?.status === "error") load();
    prevLive.current = live;
  }, [live, run?.status, load]);

  const pin = () => useWorkspace.getState().pin(tab.id);

  useEffect(
    () =>
      registerTabCommands(tab.id, {
        "data.refresh": load,
        "data.filter": () => filterRef.current?.focus(),
        "query.cancel": () => cancelRun(tab.id),
      }),
    [tab.id, load],
  );

  const onSort = (column: string) => {
    pin();
    setOffset(0);
    setSort((s) => (s?.column !== column ? { column, descending: false } : s.descending ? null : { column, descending: true }));
  };

  const applyFilter = () => {
    pin();
    setOffset(0);
    setFilter(filterDraft.trim() || null);
  };

  const stmt = run?.statements[0];
  const result = stmt?.result ?? null;
  const running = run?.status === "running";
  const error = run?.error ?? stmt?.error;
  const shown = result?.rowCount ?? 0;
  const totalNum = typeof total === "number" ? total : null;
  const hasNext = totalNum !== null ? offset + PAGE < totalNum : shown === PAGE;

  return (
    <>
      <div className="flex h-10 shrink-0 items-center gap-2 border-b border-line px-2">
        <div className="flex min-w-0 flex-1 items-center gap-1.5 rounded-md border border-line bg-bg px-2 focus-within:border-accent">
          <IconFilter className="shrink-0 text-faint" />
          <span className="font-mono text-[11.5px] text-faint">WHERE</span>
          <input
            ref={filterRef}
            className="h-7 min-w-0 flex-1 bg-transparent font-mono text-[12px] outline-none placeholder:text-faint"
            placeholder={`e.g. id > 100 AND name ILIKE '%a%'   (${shortcutFor("data.filter")})`}
            value={filterDraft}
            onChange={(e) => setFilterDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") applyFilter();
              if (e.key === "Escape" && filterDraft) {
                e.stopPropagation();
                setFilterDraft("");
                if (filter) {
                  setOffset(0);
                  setFilter(null);
                }
              }
            }}
            spellCheck={false}
          />
          {filter && <span className="rounded bg-selected px-1.5 text-[10.5px] text-accent">filtered</span>}
        </div>
        <button className="btn-ghost" onClick={load} title={`Refresh ${shortcutFor("data.refresh")}`} disabled={running}>
          {running ? <span className="spinner" /> : <IconRefresh />}
        </button>
        <button className={`btn-ghost ${inspector ? "bg-hover text-fg" : ""}`} onClick={() => setInspector((v) => !v)} title="Row inspector">
          <IconSidebarRight />
        </button>
      </div>

      <div className="flex min-h-0 flex-1">
        <div className="relative min-w-0 flex-1">
          {error ? (
            <ErrorPanel message={error} onRetry={load} />
          ) : result ? (
            <DataGrid result={result} sort={sort} onSort={onSort} rowNumberOffset={offset} onSelectionChange={setSelectedRow} emptyText="No rows" />
          ) : (
            <div className="flex h-full items-center justify-center text-faint">{running ? <span className="spinner" /> : "Not loaded"}</div>
          )}
        </div>
        {inspector && result && (
          <div className="w-80 shrink-0 overflow-y-auto border-l border-line bg-bg">
            <RowInspector result={result} row={selectedRow} />
          </div>
        )}
      </div>

      <div className="flex h-8 shrink-0 items-center gap-3 border-t border-line bg-surface px-3 text-[11.5px] text-muted">
        <span className="truncate font-mono">
          {tab.schema}.{tab.table}
        </span>
        <span className="flex-1" />
        {stmt?.done && <span>{stmt.elapsedMs} ms</span>}
        <span>
          {shown === 0 ? "0 rows" : `${(offset + 1).toLocaleString()}–${(offset + shown).toLocaleString()}`}
          {" of "}
          {totalNum !== null ? totalNum.toLocaleString() : total === "error" ? "?" : "…"}
        </span>
        <button className="btn-ghost h-6 px-1" disabled={offset === 0 || running} onClick={() => (pin(), setOffset(Math.max(0, offset - PAGE)))} aria-label="Previous page">
          <IconChevronLeft />
        </button>
        <button className="btn-ghost h-6 px-1" disabled={!hasNext || running} onClick={() => (pin(), setOffset(offset + PAGE))} aria-label="Next page">
          <IconChevronRight />
        </button>
      </div>
    </>
  );
}

export function ErrorPanel({ message, onRetry }: { message: string; onRetry?: () => void }) {
  return (
    <div className="flex h-full items-start justify-center overflow-auto p-8">
      <div className="max-w-xl rounded-lg border border-danger/30 bg-danger/5 p-4">
        <div className="mb-1 text-[12.5px] font-semibold text-danger">Query failed</div>
        <pre className="selectable font-mono text-[12px] whitespace-pre-wrap">{message}</pre>
        {onRetry && (
          <button className="btn mt-3" onClick={onRetry}>
            Retry
          </button>
        )}
      </div>
    </div>
  );
}
