import type { TlsMode } from "../../ipc/api";

const SSL_MODES: Record<string, TlsMode> = {
  disable: "disable",
  allow: "prefer",
  prefer: "prefer",
  require: "require",
  "verify-ca": "verifyCa",
  "verify-full": "verifyFull",
};

/** Parse `postgres[ql]://user:pass@host:port/db?sslmode=…` into form fields. */
export function parseConnectionUrl(raw: string) {
  const s = raw.trim();
  if (!/^postgres(ql)?:\/\//i.test(s)) return null;
  let url: URL;
  try {
    // URL only parses userinfo/host for "special" schemes; swap to http for parsing.
    url = new URL(s.replace(/^postgres(ql)?:/i, "http:"));
  } catch {
    return null;
  }
  const sslmode = url.searchParams.get("sslmode");
  return {
    host: decodeURIComponent(url.hostname),
    port: url.port,
    username: decodeURIComponent(url.username),
    password: decodeURIComponent(url.password),
    database: decodeURIComponent(url.pathname.replace(/^\//, "")),
    ...(sslmode && SSL_MODES[sslmode] ? { tlsMode: SSL_MODES[sslmode] } : {}),
  };
}
