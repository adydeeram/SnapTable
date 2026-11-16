import { describe, expect, it } from "vitest";
import { ResultSet } from "./ResultSet";

const sync = (cb: () => void) => cb();

describe("ResultSet", () => {
  it("reads cells across chunks", () => {
    const rs = new ResultSet(sync);
    rs.append(0, [[1, 2], ["a", "b"]]);
    rs.append(2, [[3], ["c"]]);
    expect(rs.rowCount).toBe(3);
    expect(rs.cell(0, 0)).toBe(1);
    expect(rs.cell(2, 1)).toBe("c");
    expect(rs.row(1)).toEqual([2, "b"]);
    expect(rs.cell(3, 0)).toBeUndefined();
    expect(rs.cell(-1, 0)).toBeUndefined();
  });

  it("rejects non-contiguous batches", () => {
    const rs = new ResultSet(sync);
    rs.append(0, [[1]]);
    expect(() => rs.append(5, [[2]])).toThrow(/non-contiguous/);
  });

  it("coalesces notifications until the scheduled flush", () => {
    const pending: Array<() => void> = [];
    const rs = new ResultSet((cb) => pending.push(cb));
    let calls = 0;
    rs.subscribe(() => calls++);
    rs.append(0, [[1]]);
    rs.append(1, [[2]]);
    expect(pending).toHaveLength(1);
    pending[0]();
    expect(calls).toBe(1);
    expect(rs.getVersion()).toBe(2);
  });

  it("handles 100k rows in 1k batches quickly", () => {
    const rs = new ResultSet(sync);
    const t0 = performance.now();
    for (let b = 0; b < 100; b++) {
      const ids = Array.from({ length: 1000 }, (_, i) => b * 1000 + i);
      rs.append(b * 1000, [ids, ids.map(String)]);
    }
    let sum = 0;
    for (let r = 0; r < rs.rowCount; r += 7) sum += rs.cell(r, 0) as number;
    expect(rs.rowCount).toBe(100_000);
    expect(sum).toBeGreaterThan(0);
    expect(performance.now() - t0).toBeLessThan(500);
  });
});
