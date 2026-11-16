import { describe, expect, it } from "vitest";
import { parseConnectionUrl } from "./url";

describe("parseConnectionUrl", () => {
  it("parses a full URL", () => {
    expect(parseConnectionUrl("postgresql://al%40ice:p%3Ass@db.example.com:6543/app?sslmode=verify-full")).toEqual({
      host: "db.example.com",
      port: "6543",
      username: "al@ice",
      password: "p:ss",
      database: "app",
      tlsMode: "verifyFull",
    });
  });

  it("handles minimal URLs and rejects others", () => {
    expect(parseConnectionUrl("postgres://localhost")).toMatchObject({ host: "localhost", port: "", database: "" });
    expect(parseConnectionUrl("mysql://x")).toBeNull();
    expect(parseConnectionUrl("not a url")).toBeNull();
  });
});
