import { useSyncExternalStore } from "react";
import { fullText, prettyJson } from "./cells";
import type { ResultSet } from "./ResultSet";

/** Right-hand panel showing every field of the selected row with full values. */
export function RowInspector({ result, row }: { result: ResultSet; row: number | null }) {
  useSyncExternalStore(result.subscribe, result.getVersion);
  if (row === null || row >= result.rowCount) {
    return <div className="p-4 text-[12px] text-faint">Select a row to inspect it.</div>;
  }
  const values = result.row(row);
  return (
    <div className="flex flex-col gap-3 p-3">
      {result.columns.map((c, i) => {
        const v = values[i];
        const text = fullText(v);
        const pretty = c.kind === "json" && typeof v === "string" ? prettyJson(v) : null;
        return (
          <div key={`${c.name}-${i}`}>
            <div className="mb-0.5 flex items-baseline gap-2">
              <span className="text-[11.5px] font-semibold">{c.name}</span>
              <span className="text-[10.5px] text-faint">{c.typeName.toLowerCase()}</span>
              <button className="ml-auto text-[10.5px] text-faint hover:text-accent" onClick={() => void navigator.clipboard.writeText(text)}>
                Copy
              </button>
            </div>
            {v === null || v === undefined ? (
              <div className="text-[11.5px] text-[var(--null)] italic">NULL</div>
            ) : (
              <pre className="selectable max-h-60 overflow-auto rounded border border-line bg-surface px-2 py-1 font-mono text-[11.5px] break-all whitespace-pre-wrap">
                {pretty ?? (text === "" ? " " : text)}
              </pre>
            )}
          </div>
        );
      })}
    </div>
  );
}
