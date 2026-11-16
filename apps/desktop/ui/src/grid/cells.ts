// Cell value formatting. Values arrive in the wire encoding described in
// crates/core/src/query.rs: JSON primitives, or {$bytes,len} / {$trunc,len} markers.

import type { ColumnKind } from "../ipc/api";

export type BytesCell = { $bytes: string; len: number };
export type TruncCell = { $trunc: string; len: number };

export const isBytes = (v: unknown): v is BytesCell => typeof v === "object" && v !== null && "$bytes" in v;
export const isTrunc = (v: unknown): v is TruncCell => typeof v === "object" && v !== null && "$trunc" in v;

const MAX_DISPLAY = 300;

export type CellDisplay = { text: string; tone: "null" | "number" | "bool" | "bytes" | "text" | "muted" };

export function formatCell(v: unknown, kind: ColumnKind | undefined): CellDisplay {
  if (v === null || v === undefined) return { text: "NULL", tone: "null" };
  if (typeof v === "boolean") return { text: v ? "true" : "false", tone: "bool" };
  if (typeof v === "number") return { text: String(v), tone: "number" };
  if (isBytes(v)) {
    const more = v.len * 2 > v.$bytes.length ? "…" : "";
    return { text: `0x${v.$bytes}${more}  (${formatBytes(v.len)})`, tone: "bytes" };
  }
  if (isTrunc(v)) return { text: oneLine(v.$trunc.slice(0, MAX_DISPLAY)) + "…", tone: "text" };
  if (typeof v === "string") {
    if (v === "") return { text: "", tone: "muted" };
    const text = oneLine(v.length > MAX_DISPLAY ? v.slice(0, MAX_DISPLAY) + "…" : v);
    return { text, tone: kind === "number" ? "number" : "text" };
  }
  return { text: oneLine(JSON.stringify(v)), tone: "text" };
}

/** Full value for the inspector / clipboard. */
export function fullText(v: unknown): string {
  if (v === null || v === undefined) return "";
  if (isBytes(v)) return `\\x${v.$bytes}${v.len * 2 > v.$bytes.length ? "…" : ""}`;
  if (isTrunc(v)) return `${v.$trunc}… [${formatBytes(v.len)} total, truncated]`;
  if (typeof v === "string") return v;
  return String(v);
}

/** Pretty-print JSON text when possible (for the inspector). */
export function prettyJson(s: string): string | null {
  const t = s.trimStart();
  if (!t.startsWith("{") && !t.startsWith("[")) return null;
  try {
    return JSON.stringify(JSON.parse(s), null, 2);
  } catch {
    return null;
  }
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

const oneLine = (s: string) => s.replace(/\r?\n/g, " ↵ ").replace(/\t/g, " ");

/** Tab-separated rendering of a cell for copying rows to spreadsheets. */
export function tsvCell(v: unknown): string {
  return fullText(v).replace(/[\t\n\r]/g, " ");
}
