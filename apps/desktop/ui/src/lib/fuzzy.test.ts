import { describe, expect, it } from "vitest";
import { fuzzyScore } from "./fuzzy";

describe("fuzzyScore", () => {
  it("matches subsequences and rejects others", () => {
    expect(fuzzyScore("usr", "users")).not.toBeNull();
    expect(fuzzyScore("xyz", "users")).toBeNull();
    expect(fuzzyScore("", "anything")).toBe(0);
  });

  it("ranks consecutive and word-start matches higher", () => {
    const exact = fuzzyScore("order", "public.orders")!;
    const scattered = fuzzyScore("order", "o_r_d_e_r_s_x")!;
    expect(exact).toBeGreaterThan(scattered);
    expect(fuzzyScore("nq", "New Query Tab")!).toBeGreaterThan(fuzzyScore("nq", "connection quality")!);
  });
});
