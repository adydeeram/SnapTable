// Typed wrappers around Tauri commands. Request/response types come from the
// generated bindings in ./bindings (source of truth: crates/core, via ts-rs).

import { Channel, invoke } from "@tauri-apps/api/core";
import type { AppError } from "./bindings/AppError";
import type { BrowseRequest } from "./bindings/BrowseRequest";
import type { Connection } from "./bindings/Connection";
import type { ConnectionInput } from "./bindings/ConnectionInput";
import type { Environment } from "./bindings/Environment";
import type { ExecuteRequest } from "./bindings/ExecuteRequest";
import type { ExecuteSummary } from "./bindings/ExecuteSummary";
import type { HistoryEntry } from "./bindings/HistoryEntry";
import type { QueryEvent } from "./bindings/QueryEvent";
import type { SchemaObject } from "./bindings/SchemaObject";
import type { TableColumn } from "./bindings/TableColumn";

export type * from "./bindings/AppError";
export type * from "./bindings/ColumnKind";
export type * from "./bindings/ColumnMeta";
export type * from "./bindings/Connection";
export type * from "./bindings/ConnectionInput";
export type * from "./bindings/Engine";
export type * from "./bindings/Environment";
export type * from "./bindings/HistoryEntry";
export type * from "./bindings/ProtectionLevel";
export type * from "./bindings/QueryEvent";
export type * from "./bindings/SchemaObject";
export type * from "./bindings/TableColumn";
export type * from "./bindings/TlsMode";

/** Error thrown by every wrapper; `error.kind` drives UI handling. */
export class IpcError extends Error {
  constructor(public readonly error: AppError) {
    super("message" in error ? error.message : error.kind);
  }
  get kind() {
    return this.error.kind;
  }
}

function isAppError(e: unknown): e is AppError {
  return typeof e === "object" && e !== null && "kind" in e;
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    if (isAppError(e)) throw new IpcError(e);
    throw new IpcError({ kind: "internal", message: String(e) });
  }
}

export const newTaskId = () => crypto.randomUUID();

export const api = {
  envList: () => call<Environment[]>("env_list"),
  connList: () => call<Connection[]>("conn_list"),
  connSave: (input: ConnectionInput) => call<Connection>("conn_save", { input }),
  connDelete: (id: string) => call<void>("conn_delete", { id }),
  connTest: (input: ConnectionInput) => call<string>("conn_test", { input }),
  connConnect: (id: string) => call<void>("conn_connect", { id }),
  connDisconnect: (id: string) => call<void>("conn_disconnect", { id }),
  connConnected: () => call<string[]>("conn_connected"),

  schemaObjects: (connectionId: string) => call<SchemaObject[]>("schema_objects", { connectionId }),
  tableColumns: (connectionId: string, schema: string, table: string) =>
    call<TableColumn[]>("table_columns", { connectionId, schema, table }),
  tableCount: (connectionId: string, schema: string, table: string, filter: string | null) =>
    call<number>("table_count", { connectionId, schema, table, filter }),
  tableBrowse: (req: BrowseRequest, onEvent: (e: QueryEvent) => void) => {
    const channel = new Channel<QueryEvent>();
    channel.onmessage = onEvent;
    return call<ExecuteSummary>("table_browse", { req, onEvent: channel });
  },

  sessionOpen: (connectionId: string) => call<string>("session_open", { connectionId }),
  sessionClose: (sessionId: string) => call<void>("session_close", { sessionId }),
  queryExecute: (req: ExecuteRequest, onEvent: (e: QueryEvent) => void) => {
    const channel = new Channel<QueryEvent>();
    channel.onmessage = onEvent;
    return call<ExecuteSummary>("query_execute", { req, onEvent: channel });
  },
  queryCancel: (taskId: string) => call<boolean>("query_cancel", { taskId }),

  historySearch: (connectionId: string | null, query: string, limit = 200) =>
    call<HistoryEntry[]>("history_search", { connectionId, query, limit }),
  workspaceGet: <T>(key: string) => call<T | null>("workspace_get", { key }),
  workspaceSet: (key: string, value: unknown) => call<void>("workspace_set", { key, value }),
};

export function errorMessage(e: unknown): string {
  if (e instanceof IpcError) {
    const err = e.error;
    if (err.kind === "cancelled") return "Cancelled";
    if (err.kind === "query") {
      return [err.message, err.detail && `Detail: ${err.detail}`, err.hint && `Hint: ${err.hint}`]
        .filter(Boolean)
        .join("\n");
    }
    return err.message;
  }
  return e instanceof Error ? e.message : String(e);
}
