import { describe, expect, it } from "vitest";
import { closeTab, cycle, openQuery, openTable, pin, pruneMissing, reopenClosed, type Tabs } from "./tabs";

const empty: Tabs = { tabs: [], activeId: null, closed: [] };
let n = 0;
const newId = () => `t${++n}`;
const users = { connectionId: "c1", schema: "public", table: "users" };
const orders = { connectionId: "c1", schema: "public", table: "orders" };

describe("tab lifecycle", () => {
  it("preview tabs are replaced by the next preview open", () => {
    let s = openTable(empty, users, { preview: true, newId });
    s = openTable(s, orders, { preview: true, newId });
    expect(s.tabs).toHaveLength(1);
    expect(s.tabs[0]).toMatchObject({ table: "orders", preview: true });
  });

  it("pinned tabs are kept and opening the same table dedupes", () => {
    let s = openTable(empty, users, { preview: false, newId });
    s = openTable(s, orders, { preview: true, newId });
    s = openTable(s, users, { preview: true, newId });
    expect(s.tabs.map((t) => (t.kind === "table" ? t.table : ""))).toEqual(["users", "orders"]);
    expect(s.activeId).toBe(s.tabs[0].id);
  });

  it("non-preview open pins an existing preview tab", () => {
    let s = openTable(empty, users, { preview: true, newId });
    s = openTable(s, users, { preview: false, newId });
    expect(s.tabs).toHaveLength(1);
    expect(s.tabs[0].preview).toBe(false);
  });

  it("pin() pins, and a later preview opens a new tab", () => {
    let s = openTable(empty, users, { preview: true, newId });
    s = pin(s, s.tabs[0].id);
    s = openTable(s, orders, { preview: true, newId });
    expect(s.tabs).toHaveLength(2);
  });

  it("query tabs get the lowest free number", () => {
    let s = openQuery(empty, "c1", "", newId);
    s = openQuery(s, "c1", "", newId);
    s = closeTab(s, s.tabs[0].id);
    s = openQuery(s, "c1", "", newId);
    expect(s.tabs.map((t) => (t.kind === "query" ? t.title : ""))).toEqual(["Query 2", "Query 1"]);
  });

  it("closing activates the right neighbour, then left; reopen restores", () => {
    let s = openQuery(empty, "c1", "a", newId);
    s = openQuery(s, "c1", "b", newId);
    s = openQuery(s, "c1", "c", newId);
    const [a, b, c] = s.tabs;
    s = { ...s, activeId: b.id };
    s = closeTab(s, b.id);
    expect(s.activeId).toBe(c.id);
    s = closeTab(s, c.id);
    expect(s.activeId).toBe(a.id);
    s = reopenClosed(s);
    expect(s.activeId).toBe(c.id);
    expect(s.tabs).toHaveLength(2);
  });

  it("cycles with wrap-around", () => {
    let s = openQuery(empty, "c1", "", newId);
    s = openQuery(s, "c1", "", newId);
    s = cycle(s, 1);
    expect(s.activeId).toBe(s.tabs[0].id);
    s = cycle(s, -1);
    expect(s.activeId).toBe(s.tabs[1].id);
  });

  it("prunes tabs of deleted connections", () => {
    let s = openTable(empty, users, { preview: false, newId });
    s = openQuery(s, "gone", "", newId);
    s = pruneMissing(s, new Set(["c1"]));
    expect(s.tabs).toHaveLength(1);
    expect(s.activeId).toBe(s.tabs[0].id);
  });
});
