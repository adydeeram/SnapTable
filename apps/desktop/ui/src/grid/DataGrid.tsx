// Virtualized data grid (SPEC §3.4): 2-D virtualization over a ResultSet, sticky
// header and row-number gutter, keyboard cell navigation, column resizing, copy.

import { useVirtualizer } from "@tanstack/react-virtual";
import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import type { ResultSet } from "./ResultSet";
import { formatCell, fullText, tsvCell } from "./cells";

const ROW_H = 26;
const HEADER_H = 30;
const MIN_COL = 60;
const MAX_AUTO_COL = 420;

export type SortState = { column: string; descending: boolean } | null;

type Props = {
  result: ResultSet;
  sort?: SortState;
  onSort?: (column: string) => void;
  /** Row offset added to the gutter numbers (paged table browsing). */
  rowNumberOffset?: number;
  onSelectionChange?: (row: number | null) => void;
  emptyText?: string;
};

export function DataGrid({ result, sort, onSort, rowNumberOffset = 0, onSelectionChange, emptyText }: Props) {
  useSyncExternalStore(result.subscribe, result.getVersion);
  const columns = result.columns;
  const rowCount = result.rowCount;
  const scrollRef = useRef<HTMLDivElement>(null);
  const [sel, setSel] = useState<{ row: number; col: number } | null>(null);
  const [widths, setWidths] = useState<number[]>([]);

  // Auto-size columns once per column set, from the header and the first rows.
  const columnKey = columns.map((c) => c.name).join("\u0000");
  const sampled = rowCount > 0;
  useEffect(() => {
    setWidths(autoWidths(result));
    setSel(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [columnKey, sampled]);

  const gutterW = Math.max(44, String(rowNumberOffset + rowCount).length * 8 + 20);
  const colW = (i: number) => widths[i] ?? 140;

  const rowV = useVirtualizer({
    count: rowCount,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_H,
    overscan: 16,
    paddingStart: HEADER_H,
  });
  const colV = useVirtualizer({
    horizontal: true,
    count: columns.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: colW,
    overscan: 4,
    paddingStart: gutterW,
  });
  useEffect(() => colV.measure(), [widths, gutterW, colV]);

  useEffect(() => onSelectionChange?.(sel?.row ?? null), [sel?.row, onSelectionChange]);

  const select = useCallback(
    (row: number, col: number) => {
      const r = Math.max(0, Math.min(rowCount - 1, row));
      const c = Math.max(0, Math.min(columns.length - 1, col));
      setSel({ row: r, col: c });
      rowV.scrollToIndex(r, { align: "auto" });
      colV.scrollToIndex(c, { align: "auto" });
    },
    [rowCount, columns.length, rowV, colV],
  );

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (!sel || rowCount === 0) return;
    const page = Math.max(1, Math.floor((scrollRef.current?.clientHeight ?? 400) / ROW_H) - 2);
    const moves: Record<string, [number, number]> = {
      ArrowUp: [-1, 0],
      ArrowDown: [1, 0],
      ArrowLeft: [0, -1],
      ArrowRight: [0, 1],
      PageUp: [-page, 0],
      PageDown: [page, 0],
      Tab: [0, e.shiftKey ? -1 : 1],
    };
    if (e.key in moves) {
      e.preventDefault();
      const [dr, dc] = moves[e.key];
      if (e.metaKey && e.key === "ArrowUp") return select(0, sel.col);
      if (e.metaKey && e.key === "ArrowDown") return select(rowCount - 1, sel.col);
      if (e.metaKey && e.key === "ArrowLeft") return select(sel.row, 0);
      if (e.metaKey && e.key === "ArrowRight") return select(sel.row, columns.length - 1);
      select(sel.row + dr, sel.col + dc);
    } else if (e.key === "Home") {
      e.preventDefault();
      select(sel.row, 0);
    } else if (e.key === "End") {
      e.preventDefault();
      select(sel.row, columns.length - 1);
    }
  };

  const onCopy = (e: React.ClipboardEvent) => {
    if (!sel) return;
    e.preventDefault();
    // ⌘C copies the cell; ⌘⇧C is not available here, so ⌥-click row number copies rows.
    e.clipboardData.setData("text/plain", fullText(result.cell(sel.row, sel.col)));
  };

  const startResize = (e: React.MouseEvent, i: number) => {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const startW = colW(i);
    const move = (ev: MouseEvent) => {
      const w = Math.max(MIN_COL, startW + ev.clientX - startX);
      setWidths((prev) => {
        const next = [...prev];
        next[i] = w;
        return next;
      });
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  const copyRow = async (row: number) => {
    const header = columns.map((c) => c.name).join("\t");
    const line = columns.map((_, c) => tsvCell(result.cell(row, c))).join("\t");
    await navigator.clipboard.writeText(`${header}\n${line}`);
  };

  const totalW = colV.getTotalSize();
  const totalH = rowV.getTotalSize();
  const vRows = rowV.getVirtualItems();
  const vCols = colV.getVirtualItems();

  if (columns.length === 0) {
    return <div className="grid-empty">{emptyText ?? "No results"}</div>;
  }

  return (
    <div
      ref={scrollRef}
      className="data-grid"
      tabIndex={0}
      role="grid"
      aria-rowcount={rowCount}
      aria-colcount={columns.length}
      onKeyDown={onKeyDown}
      onCopy={onCopy}
    >
      <div style={{ width: totalW, height: totalH, position: "relative" }}>
        {/* Header */}
        <div className="dg-header" style={{ width: totalW, height: HEADER_H }} role="row">
          <div className="dg-corner" style={{ width: gutterW, height: HEADER_H }} />
          {vCols.map((vc) => {
            const col = columns[vc.index];
            const sorted = sort?.column === col.name ? (sort.descending ? "▼" : "▲") : "";
            return (
              <div
                key={vc.key}
                role="columnheader"
                className={`dg-hcell ${onSort ? "sortable" : ""}`}
                style={{ left: vc.start, width: vc.size, height: HEADER_H }}
                title={`${col.name} · ${col.typeName}`}
                onClick={() => onSort?.(col.name)}
              >
                <span className="dg-hname">{col.name}</span>
                <span className="dg-htype">{col.typeName.toLowerCase()}</span>
                {sorted && <span className="dg-sort">{sorted}</span>}
                <span className="dg-resize" onMouseDown={(e) => startResize(e, vc.index)} onClick={(e) => e.stopPropagation()} />
              </div>
            );
          })}
        </div>
        {/* Rows */}
        {vRows.map((vr) => {
          const rowSelected = sel?.row === vr.index;
          return (
            <div
              key={vr.key}
              role="row"
              className={`dg-row ${vr.index % 2 ? "odd" : ""} ${rowSelected ? "selected" : ""}`}
              style={{ top: vr.start, height: ROW_H, width: totalW }}
            >
              <div
                className="dg-gutter"
                style={{ width: gutterW, height: ROW_H }}
                title="Click to select · ⌥-click to copy row"
                onClick={(e) => (e.altKey ? void copyRow(vr.index) : select(vr.index, sel?.col ?? 0))}
              >
                {rowNumberOffset + vr.index + 1}
              </div>
              {vCols.map((vc) => {
                const kind = columns[vc.index].kind;
                const d = formatCell(result.cell(vr.index, vc.index), kind);
                const active = rowSelected && sel?.col === vc.index;
                return (
                  <div
                    key={vc.key}
                    role="gridcell"
                    className={`dg-cell tone-${d.tone} ${active ? "active" : ""}`}
                    style={{ left: vc.start, width: vc.size, height: ROW_H }}
                    onMouseDown={() => select(vr.index, vc.index)}
                  >
                    {d.text}
                  </div>
                );
              })}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function autoWidths(result: ResultSet): number[] {
  const sample = Math.min(result.rowCount, 200);
  return result.columns.map((c, ci) => {
    // Header shows "name type" (type in a smaller font).
    let chars = c.name.length + c.typeName.length * 0.75 + 2;
    for (let r = 0; r < sample; r++) {
      const t = formatCell(result.cell(r, ci), c.kind).text;
      if (t.length > chars) chars = t.length;
    }
    return Math.round(Math.max(MIN_COL + 10, Math.min(MAX_AUTO_COL, chars * 7.4 + 26)));
  });
}
