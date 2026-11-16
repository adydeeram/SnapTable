// SQL editor tab: Monaco on top, result sets + messages below. Each tab owns a pinned
// backend session, so transactions and SET persist between runs (ADR 0005).

import { useEffect, useRef, useState } from "react";
import { registerTabCommands, shortcutFor } from "../../app/commands";
import { DataGrid } from "../../grid/DataGrid";
import { RowInspector } from "../../grid/RowInspector";
import { IconPlay, IconSidebarRight, IconStop } from "../../lib/icons";
import { useConnections } from "../../stores/connections";
import { cancelRun, runQuery, useRun, type QueryRun, type StatementResult } from "../../stores/queryRuns";
import type { QueryTab } from "../../stores/tabs";
import { useWorkspace } from "../../stores/workspace";
import { ErrorPanel } from "../table/TableView";
import { SqlEditor, type SqlEditorHandle } from "./SqlEditor";

export function QueryView({ tab, active }: { tab: QueryTab; active: boolean }) {
  const editor = useRef<SqlEditorHandle>(null);
  const run = useRun(tab.id);
  const connections = useConnections((s) => s.connections);
  const connection = connections.find((c) => c.id === tab.connectionId);
  const [split, setSplit] = useState(0.42);
  const container = useRef<HTMLDivElement>(null);

  // Persist editor text (debounced via the workspace store's own debounce).
  const onChange = (sql: string) => useWorkspace.getState().updateQuery(tab.id, { sql });

  const execute = (mode: "current" | "all") => {
    const ed = editor.current;
    if (!ed) return;
    ed.clearErrors();
    const selection = ed.getSelection();
    if (selection) return void runQuery(tab.id, tab.connectionId, selection, "all", null);
    void runQuery(tab.id, tab.connectionId, ed.getValue(), mode === "all" ? "all" : "atCursor", ed.getCursorOffset());
  };

  useEffect(
    () =>
      registerTabCommands(tab.id, {
        "query.runCurrent": () => execute("current"),
        "query.runAll": () => execute("all"),
        "query.cancel": () => cancelRun(tab.id),
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [tab.id, tab.connectionId],
  );

  // Surface the first statement error as an editor marker.
  useEffect(() => {
    if (!run || run.status !== "error") return;
    const failed = run.statements.find((s) => s.error);
    if (failed?.error) editor.current?.showError(failed.sql, failed.errorPosition, failed.error);
  }, [run, run?.status]);

  const startDrag = (e: React.MouseEvent) => {
    e.preventDefault();
    const rect = container.current!.getBoundingClientRect();
    const move = (ev: MouseEvent) => setSplit(Math.min(0.85, Math.max(0.12, (ev.clientY - rect.top) / rect.height)));
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  const running = run?.status === "running";

  return (
    <div ref={container} className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-10 shrink-0 items-center gap-2 border-b border-line px-2">
        <button className="btn btn-primary" disabled={running} onClick={() => execute("current")} title={`Run statement at cursor (or selection) ${shortcutFor("query.runCurrent")}`}>
          <IconPlay size={11} /> Run
        </button>
        <button className="btn" disabled={running} onClick={() => execute("all")} title={`Run all ${shortcutFor("query.runAll")}`}>
          Run All
        </button>
        <button className="btn" disabled={!running} onClick={() => cancelRun(tab.id)} title={`Cancel ${shortcutFor("query.cancel")} / Esc`}>
          <IconStop size={11} /> Cancel
        </button>
        <select
          className="field ml-2 h-7 w-56"
          value={tab.connectionId}
          onChange={(e) => useWorkspace.getState().updateQuery(tab.id, { connectionId: e.target.value })}
          disabled={running}
          aria-label="Connection"
        >
          {connections.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </select>
        <span className="flex-1" />
        <RunStatus run={run} />
      </div>

      <div className="min-h-0" style={{ height: `${split * 100}%` }}>
        <SqlEditor ref={editor} initialValue={tab.sql} language={connection?.engine === "sqlite" ? "sql" : "pgsql"} onChange={onChange} active={active} />
      </div>
      <div className="h-1 shrink-0 cursor-row-resize border-y border-line bg-surface hover:bg-accent/40" onMouseDown={startDrag} />
      <div className="flex min-h-0 flex-1 flex-col">
        <Results run={run} />
      </div>
    </div>
  );
}

function RunStatus({ run }: { run: QueryRun | undefined }) {
  const [now, setNow] = useState(performance.now());
  const running = run?.status === "running";
  useEffect(() => {
    if (!running) return;
    const t = setInterval(() => setNow(performance.now()), 100);
    return () => clearInterval(t);
  }, [running]);
  if (!run) return null;
  if (running) {
    return (
      <span className="flex items-center gap-2 text-[12px] text-muted">
        <span className="spinner" /> Running… {((now - run.startedAt) / 1000).toFixed(1)} s
      </span>
    );
  }
  const n = run.statements.length;
  const label = run.status === "cancelled" ? "Cancelled" : run.status === "error" ? "Failed" : "Done";
  const color = run.status === "error" ? "text-danger" : run.status === "cancelled" ? "text-warning" : "text-muted";
  return (
    <span className={`text-[12px] ${color}`}>
      {label} · {n} statement{n === 1 ? "" : "s"} · {run.elapsedMs} ms
    </span>
  );
}

function Results({ run }: { run: QueryRun | undefined }) {
  const [selected, setSelected] = useState<number | "messages">(0);
  const [inspector, setInspector] = useState(false);
  const [selectedRow, setSelectedRow] = useState<number | null>(null);
  const withResults = run?.statements.filter((s) => s.result) ?? [];

  // Jump to the newest result set when a run starts producing them.
  const lastWithResult = withResults[withResults.length - 1]?.index;
  useEffect(() => {
    if (run?.status === "error" || (run && run.statements.length > 0 && withResults.length === 0)) setSelected("messages");
    else if (lastWithResult !== undefined) setSelected(lastWithResult);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [run, lastWithResult, run?.status]);

  if (!run) {
    return <div className="flex flex-1 items-center justify-center text-[12px] text-faint">Run a query with {shortcutFor("query.runCurrent")}</div>;
  }
  if (run.error) return <ErrorPanel message={run.error} />;

  const current = typeof selected === "number" ? run.statements.find((s) => s.index === selected && s.result) : undefined;

  return (
    <>
      <div role="tablist" className="flex h-8 shrink-0 items-center gap-0.5 border-b border-line bg-surface px-1">
        {withResults.map((s, i) => (
          <ResultTabButton key={s.index} active={selected === s.index} onClick={() => setSelected(s.index)}>
            Result {withResults.length > 1 ? i + 1 : ""}
            <span className="text-faint">{s.result!.rowCount.toLocaleString()}</span>
          </ResultTabButton>
        ))}
        <ResultTabButton active={selected === "messages"} onClick={() => setSelected("messages")}>
          Messages
          {run.statements.some((s) => s.error) && <span className="h-1.5 w-1.5 rounded-full bg-danger" />}
        </ResultTabButton>
        <span className="flex-1" />
        {current && (
          <button className={`btn-ghost h-6 ${inspector ? "bg-hover text-fg" : ""}`} onClick={() => setInspector((v) => !v)} title="Row inspector">
            <IconSidebarRight />
          </button>
        )}
      </div>
      <div className="flex min-h-0 flex-1">
        <div className="relative min-w-0 flex-1">
          {current?.result ? (
            <DataGrid key={current.index} result={current.result} onSelectionChange={setSelectedRow} emptyText="Query returned no rows" />
          ) : (
            <Messages run={run} />
          )}
        </div>
        {inspector && current?.result && (
          <div className="w-80 shrink-0 overflow-y-auto border-l border-line">
            <RowInspector result={current.result} row={selectedRow} />
          </div>
        )}
      </div>
      {current && <ResultFooter s={current} />}
    </>
  );
}

function ResultTabButton({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      role="tab"
      aria-selected={active}
      onClick={onClick}
      className={`flex h-6 items-center gap-1.5 rounded px-2.5 text-[12px] ${active ? "bg-bg font-medium text-fg shadow-sm" : "text-muted hover:text-fg"}`}
    >
      {children}
    </button>
  );
}

function ResultFooter({ s }: { s: StatementResult }) {
  return (
    <div className="flex h-7 shrink-0 items-center gap-3 border-t border-line bg-surface px-3 text-[11.5px] text-muted">
      <span className="min-w-0 flex-1 truncate font-mono">{s.sql.replace(/\s+/g, " ")}</span>
      {s.truncated && (
        <span className="text-warning" title="Rows past the limit were not loaded. Narrow the query or add LIMIT.">
          Showing first {s.result!.rowCount.toLocaleString()} of {s.rowCount.toLocaleString()} rows
        </span>
      )}
      {!s.truncated && <span>{(s.result?.rowCount ?? 0).toLocaleString()} rows</span>}
      {s.done && <span>{s.elapsedMs} ms</span>}
    </div>
  );
}

function Messages({ run }: { run: QueryRun }) {
  return (
    <div className="selectable h-full overflow-auto p-2 font-mono text-[12px]">
      {run.statements.map((s) => (
        <div key={s.index} className="border-b border-line px-2 py-2 last:border-0">
          <div className="truncate text-muted">{s.sql.replace(/\s+/g, " ")}</div>
          <div className={`mt-0.5 whitespace-pre-wrap ${s.error ? "text-danger" : s.cancelled ? "text-warning" : ""}`}>
            {s.error
              ? s.error
              : s.cancelled
                ? "Cancelled"
                : !s.done
                  ? "Running…"
                  : s.result
                    ? `${s.rowCount.toLocaleString()} row${s.rowCount === 1 ? "" : "s"} returned`
                    : `OK${s.rowsAffected !== null ? ` · ${s.rowsAffected.toLocaleString()} row${s.rowsAffected === 1 ? "" : "s"} affected` : ""}`}
            {s.done && <span className="text-faint"> · {s.elapsedMs} ms</span>}
          </div>
        </div>
      ))}
      {run.status === "cancelled" && run.statements.length === 0 && <div className="p-2 text-warning">Cancelled</div>}
    </div>
  );
}
