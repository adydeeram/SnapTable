// Columnar, chunked row storage that lives outside React state (ADR 0003).
//
// Batches arrive as `columns[c][r]` arrays and are kept as-is (no row objects are
// materialised). The grid reads cells by (row, col) and re-renders only when the
// version counter changes; notifications are coalesced to one per animation frame.

import type { ColumnMeta } from "../ipc/api";

type Chunk = { start: number; length: number; columns: unknown[][] };

export class ResultSet {
  columns: ColumnMeta[] = [];
  rowCount = 0;
  private chunks: Chunk[] = [];
  private version = 0;
  private listeners = new Set<() => void>();
  private scheduled = false;

  constructor(private readonly schedule: (cb: () => void) => void = defaultSchedule) {}

  setColumns(columns: ColumnMeta[]) {
    this.columns = columns;
    this.bump();
  }

  /** Append a columnar batch. Batches must be contiguous (startRow === rowCount). */
  append(startRow: number, columns: unknown[][]) {
    const length = columns[0]?.length ?? 0;
    if (length === 0) return;
    if (startRow !== this.rowCount) {
      throw new Error(`non-contiguous batch: expected row ${this.rowCount}, got ${startRow}`);
    }
    this.chunks.push({ start: startRow, length, columns });
    this.rowCount += length;
    this.bump();
  }

  cell(row: number, col: number): unknown {
    const chunk = this.chunkFor(row);
    return chunk ? chunk.columns[col]?.[row - chunk.start] : undefined;
  }

  row(row: number): unknown[] {
    const chunk = this.chunkFor(row);
    if (!chunk) return [];
    return chunk.columns.map((c) => c[row - chunk.start]);
  }

  private chunkFor(row: number): Chunk | undefined {
    if (row < 0 || row >= this.rowCount) return undefined;
    // Binary search; chunks are sorted by start and contiguous.
    let lo = 0;
    let hi = this.chunks.length - 1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      const c = this.chunks[mid];
      if (row < c.start) hi = mid - 1;
      else if (row >= c.start + c.length) lo = mid + 1;
      else return c;
    }
    return undefined;
  }

  // ---- useSyncExternalStore protocol ----
  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };
  getVersion = () => this.version;

  private bump() {
    this.version++;
    if (this.scheduled) return;
    this.scheduled = true;
    this.schedule(() => {
      this.scheduled = false;
      for (const l of this.listeners) l();
    });
  }
}

function defaultSchedule(cb: () => void) {
  if (typeof requestAnimationFrame === "function") requestAnimationFrame(() => cb());
  else setTimeout(cb, 0);
}
