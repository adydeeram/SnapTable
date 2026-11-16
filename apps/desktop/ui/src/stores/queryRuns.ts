// Query execution state per tab, kept outside React (results can be huge). Components
// subscribe via useSyncExternalStore. Also owns editor-tab ⇄ backend session mapping.

import { useSyncExternalStore } from "react";
import { ResultSet } from "../grid/ResultSet";
import { api, errorMessage, IpcError, newTaskId, type QueryEvent } from "../ipc/api";
import type { ExecuteSummary } from "../ipc/bindings/ExecuteSummary";
import { useConnections } from "./connections";

export type StatementResult = {
  index: number;
  sql: string;
  result: ResultSet | null;
  error?: string;
  errorPosition?: number | null;
  cancelled?: boolean;
  rowsAffected: number | null;
  rowCount: number;
  truncated: boolean;
  elapsedMs: number;
  done: boolean;
};

export type RunStatus = "idle" | "running" | "done" | "error" | "cancelled";

export class QueryRun {
  status: RunStatus = "running";
  statements: StatementResult[] = [];
  elapsedMs = 0;
  readonly startedAt = performance.now();
  error?: string;
  private version = 0;
  private listeners = new Set<() => void>();

  constructor(readonly taskId: string) {}

  handle = (ev: QueryEvent) => {
    switch (ev.type) {
      case "statementStart":
        this.statements.push({ index: ev.index, sql: ev.sql, result: null, rowsAffected: null, rowCount: 0, truncated: false, elapsedMs: 0, done: false });
        break;
      case "columns": {
        const s = this.stmt(ev.index);
        if (s) {
          s.result = new ResultSet();
          s.result.setColumns(ev.columns);
        }
        break;
      }
      case "rows":
        this.stmt(ev.index)?.result?.append(ev.startRow, ev.columns);
        break;
      case "statementEnd": {
        const s = this.stmt(ev.index);
        if (s) Object.assign(s, { rowsAffected: ev.rowsAffected, rowCount: ev.rowCount, truncated: ev.truncated, elapsedMs: ev.elapsedMs, done: true });
        break;
      }
      case "statementError": {
        const s = this.stmt(ev.index);
        if (s) {
          s.done = true;
          s.elapsedMs = ev.elapsedMs;
          if (ev.error.kind === "cancelled") s.cancelled = true;
          else {
            s.error = errorMessage(new IpcError(ev.error));
            s.errorPosition = ev.error.kind === "query" ? ev.error.position : null;
          }
        }
        break;
      }
      case "done":
        this.elapsedMs = ev.elapsedMs;
        this.status = ev.cancelled ? "cancelled" : this.statements.some((s) => s.error) ? "error" : "done";
        break;
    }
    this.bump();
  };

  fail(message: string) {
    this.status = "error";
    this.error = message;
    this.bump();
  }

  finish(summary: ExecuteSummary) {
    if (this.status === "running") {
      this.status = summary.cancelled ? "cancelled" : summary.errors ? "error" : "done";
      this.elapsedMs = summary.elapsedMs;
      this.bump();
    }
  }

  private stmt(index: number) {
    return this.statements.find((s) => s.index === index);
  }

  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };
  getVersion = () => this.version;
  private bump() {
    this.version++;
    for (const l of this.listeners) l();
  }
}

// ---- registry of runs per tab -------------------------------------------------

const runs = new Map<string, QueryRun>();
const runListeners = new Set<() => void>();
let runsVersion = 0;

function setRun(tabId: string, run: QueryRun | undefined) {
  if (run) runs.set(tabId, run);
  else runs.delete(tabId);
  runsVersion++;
  for (const l of runListeners) l();
}

export function useRun(tabId: string): QueryRun | undefined {
  useSyncExternalStore(
    (fn) => {
      runListeners.add(fn);
      return () => runListeners.delete(fn);
    },
    () => runsVersion,
  );
  const run = runs.get(tabId);
  useSyncExternalStore(run?.subscribe ?? noopSubscribe, run?.getVersion ?? zero);
  return run;
}

const noopSubscribe = () => () => {};
const zero = () => 0;

export const getRun = (tabId: string) => runs.get(tabId);

export function cancelRun(tabId: string) {
  const run = runs.get(tabId);
  if (run?.status === "running") void api.queryCancel(run.taskId);
}

// ---- editor sessions ----------------------------------------------------------

const sessions = new Map<string, { connectionId: string; sessionId: string }>();

async function sessionFor(tabId: string, connectionId: string): Promise<string> {
  const existing = sessions.get(tabId);
  if (existing?.connectionId === connectionId) return existing.sessionId;
  if (existing) {
    sessions.delete(tabId);
    void api.sessionClose(existing.sessionId).catch(() => {});
  }
  const sessionId = await api.sessionOpen(connectionId);
  sessions.set(tabId, { connectionId, sessionId });
  return sessionId;
}

/** Release backend resources when a tab closes (rolls back open transactions). */
export function disposeTab(tabId: string) {
  cancelRun(tabId);
  setRun(tabId, undefined);
  const s = sessions.get(tabId);
  if (s) {
    sessions.delete(tabId);
    void api.sessionClose(s.sessionId).catch(() => {});
  }
}

/** Forget sessions for a connection that was disconnected. */
export function forgetConnectionSessions(connectionId: string) {
  for (const [tabId, s] of sessions) if (s.connectionId === connectionId) sessions.delete(tabId);
}

export async function runQuery(
  tabId: string,
  connectionId: string,
  sql: string,
  mode: "all" | "atCursor",
  cursorUtf16: number | null,
) {
  if (runs.get(tabId)?.status === "running") return;
  const run = new QueryRun(newTaskId());
  setRun(tabId, run);
  try {
    if (!(await useConnections.getState().connect(connectionId))) {
      run.fail("Not connected");
      return;
    }
    let sessionId: string;
    try {
      sessionId = await sessionFor(tabId, connectionId);
    } catch (e) {
      // The backend may have dropped the connection (e.g. after an edit); reconnect once.
      if (e instanceof IpcError && (e.kind === "connection" || e.kind === "notFound")) {
        sessions.delete(tabId);
        useConnections.setState((s) => ({ live: { ...s.live, [connectionId]: "disconnected" } }));
        if (!(await useConnections.getState().connect(connectionId))) throw e;
        sessionId = await sessionFor(tabId, connectionId);
      } else throw e;
    }
    const summary = await api.queryExecute(
      { taskId: run.taskId, sessionId, sql, mode, cursorUtf16, maxRows: null },
      run.handle,
    );
    run.finish(summary);
  } catch (e) {
    if (e instanceof IpcError && e.kind === "notFound") sessions.delete(tabId);
    run.fail(errorMessage(e));
  }
}

export async function browseTable(
  tabId: string,
  req: { connectionId: string; schema: string; table: string; offset: number; limit: number; orderBy: { column: string; descending: boolean } | null; filter: string | null },
) {
  cancelRun(tabId);
  const run = new QueryRun(newTaskId());
  setRun(tabId, run);
  try {
    if (!(await useConnections.getState().connect(req.connectionId))) {
      run.fail("Not connected");
      return;
    }
    const summary = await api.tableBrowse({ taskId: run.taskId, ...req }, run.handle);
    run.finish(summary);
  } catch (e) {
    run.fail(errorMessage(e));
  }
}
