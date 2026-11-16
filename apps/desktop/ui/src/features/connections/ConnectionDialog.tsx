// Create/edit a connection. Passwords are write-only: the dialog never receives the
// stored value, only `hasPassword`, and sends Keep / Set / Clear on save (SPEC §2.3).

import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, errorMessage, type ConnectionInput, type Engine, type TlsMode } from "../../ipc/api";
import type { SecretChange } from "../../ipc/bindings/SecretChange";
import { useConnections } from "../../stores/connections";
import { toast } from "../../stores/toasts";
import { useWorkspace } from "../../stores/workspace";
import { parseConnectionUrl } from "./url";

type Form = {
  name: string;
  engine: Engine;
  environmentId: string;
  host: string;
  port: string;
  username: string;
  password: string;
  clearPassword: boolean;
  database: string;
  sqlitePath: string;
  tlsMode: TlsMode;
  statementTimeoutMs: string;
  searchPath: string;
};

const TLS_MODES: Array<[TlsMode, string]> = [
  ["disable", "Disable"],
  ["prefer", "Prefer"],
  ["require", "Require"],
  ["verifyCa", "Verify CA"],
  ["verifyFull", "Verify full"],
];

export function ConnectionDialog({ editId }: { editId?: string }) {
  const close = useWorkspace((s) => s.closeConnectionDialog);
  const environments = useConnections((s) => s.environments);
  const existing = useConnections((s) => s.connections.find((c) => c.id === editId));
  const [form, setForm] = useState<Form>(() => ({
    name: existing?.name ?? "",
    engine: existing?.engine ?? "postgres",
    environmentId: existing?.environmentId ?? "local",
    host: existing?.host ?? "localhost",
    port: existing?.port ? String(existing.port) : "",
    username: existing?.username ?? "",
    password: "",
    clearPassword: false,
    database: existing?.database ?? "",
    sqlitePath: existing?.sqlitePath ?? "",
    tlsMode: existing?.tlsMode ?? "prefer",
    statementTimeoutMs: existing?.options.statementTimeoutMs ? String(existing.options.statementTimeoutMs) : "",
    searchPath: existing?.options.searchPath ?? "",
  }));
  const [busy, setBusy] = useState<null | "test" | "save">(null);
  const [testResult, setTestResult] = useState<{ ok: boolean; text: string } | null>(null);
  const nameRef = useRef<HTMLInputElement>(null);
  useEffect(() => nameRef.current?.focus(), []);

  const set = <K extends keyof Form>(k: K, v: Form[K]) => {
    setForm((f) => ({ ...f, [k]: v }));
    setTestResult(null);
  };

  const passwordChange: SecretChange = form.clearPassword
    ? { action: "clear" }
    : form.password
      ? { action: "set", value: form.password }
      : { action: "keep" };

  const input = useMemo<ConnectionInput>(() => {
    const port = form.port.trim() ? Number(form.port) : null;
    const timeout = form.statementTimeoutMs.trim() ? Number(form.statementTimeoutMs) : null;
    const isPg = form.engine === "postgres";
    return {
      id: editId ?? null,
      groupId: existing?.groupId ?? null,
      name: form.name.trim() || defaultName(form),
      engine: form.engine,
      environmentId: form.environmentId || null,
      host: isPg ? form.host : null,
      port: isPg && port && Number.isFinite(port) ? port : null,
      database: isPg ? form.database : null,
      username: isPg ? form.username : null,
      sqlitePath: isPg ? null : form.sqlitePath,
      tlsMode: form.tlsMode,
      options: {
        statementTimeoutMs: isPg && timeout && Number.isFinite(timeout) ? timeout : null,
        searchPath: isPg && form.searchPath.trim() ? form.searchPath.trim() : null,
      },
      password: isPg ? passwordChange : { action: "keep" },
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [form, editId, existing]);

  const test = async () => {
    setBusy("test");
    setTestResult(null);
    try {
      const version = await api.connTest(input);
      setTestResult({ ok: true, text: version.split(" on ")[0] });
    } catch (e) {
      setTestResult({ ok: false, text: errorMessage(e) });
    } finally {
      setBusy(null);
    }
  };

  const save = async (connectAfter: boolean) => {
    setBusy("save");
    try {
      const saved = await api.connSave(input);
      useConnections.getState().saved(saved);
      close();
      toast.success(editId ? "Connection updated" : "Connection saved", saved.name);
      if (connectAfter) void useConnections.getState().connect(saved.id);
    } catch (e) {
      toast.error("Could not save connection", errorMessage(e));
    } finally {
      setBusy(null);
    }
  };

  const chooseFile = async () => {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "SQLite database", extensions: ["db", "sqlite", "sqlite3", "db3"] }, { name: "All files", extensions: ["*"] }],
    });
    if (typeof path === "string") set("sqlitePath", path);
  };

  const applyUrl = (url: string) => {
    const parsed = parseConnectionUrl(url);
    if (!parsed) return;
    setForm((f) => ({ ...f, ...parsed, engine: "postgres" }));
    setTestResult(null);
  };

  const isPg = form.engine === "postgres";

  return (
    <div className="fixed inset-0 z-40 flex items-start justify-center bg-black/25 pt-[8vh]" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <form
        className="w-[560px] rounded-xl border border-line bg-elev shadow-[var(--shadow)]"
        onSubmit={(e) => {
          e.preventDefault();
          void save(true);
        }}
        onKeyDown={(e) => e.key === "Escape" && close()}
      >
        <div className="border-b border-line px-5 py-3 text-[14px] font-semibold">{editId ? "Edit Connection" : "New Connection"}</div>
        <div className="grid grid-cols-6 gap-3 px-5 py-4">
          <div className="col-span-4">
            <label className="label">Name</label>
            <input ref={nameRef} className="field" placeholder={defaultName(form)} value={form.name} onChange={(e) => set("name", e.target.value)} />
          </div>
          <div className="col-span-2">
            <label className="label">Environment</label>
            <select className="field" value={form.environmentId} onChange={(e) => set("environmentId", e.target.value)}>
              {environments.map((env) => (
                <option key={env.id} value={env.id}>
                  {env.name}
                  {env.protectionLevel === "readOnly" ? " (read-only)" : ""}
                </option>
              ))}
            </select>
          </div>

          <div className="col-span-6">
            <label className="label">Engine</label>
            <div className="flex gap-1.5">
              {(["postgres", "sqlite"] as const).map((e) => (
                <button
                  type="button"
                  key={e}
                  onClick={() => set("engine", e)}
                  className={`btn ${form.engine === e ? "border-accent ring-2 ring-accent/25" : ""}`}
                >
                  {e === "postgres" ? "PostgreSQL" : "SQLite"}
                </button>
              ))}
              <span className="self-center pl-2 text-[11.5px] text-faint">MySQL/MariaDB and Redis arrive in Phase 2</span>
            </div>
          </div>

          {isPg ? (
            <>
              <div className="col-span-6">
                <label className="label">Connection URL (optional, fills the fields below)</label>
                <input className="field font-mono" placeholder="postgres://user@host:5432/db" onChange={(e) => applyUrl(e.target.value)} />
              </div>
              <div className="col-span-4">
                <label className="label">Host</label>
                <input className="field" value={form.host} onChange={(e) => set("host", e.target.value)} />
              </div>
              <div className="col-span-2">
                <label className="label">Port</label>
                <input className="field" inputMode="numeric" placeholder="5432" value={form.port} onChange={(e) => set("port", e.target.value.replace(/\D/g, ""))} />
              </div>
              <div className="col-span-3">
                <label className="label">User</label>
                <input className="field" value={form.username} onChange={(e) => set("username", e.target.value)} autoComplete="off" />
              </div>
              <div className="col-span-3">
                <label className="label">
                  Password <span className="font-normal text-faint">· stored in macOS Keychain</span>
                </label>
                <input
                  className="field"
                  type="password"
                  autoComplete="new-password"
                  disabled={form.clearPassword}
                  placeholder={existing?.hasPassword ? "•••••••• (unchanged)" : ""}
                  value={form.password}
                  onChange={(e) => set("password", e.target.value)}
                />
                {existing?.hasPassword && (
                  <label className="mt-1 flex items-center gap-1.5 text-[11.5px] text-muted">
                    <input type="checkbox" checked={form.clearPassword} onChange={(e) => set("clearPassword", e.target.checked)} />
                    Remove saved password
                  </label>
                )}
              </div>
              <div className="col-span-3">
                <label className="label">Database</label>
                <input className="field" placeholder="postgres" value={form.database} onChange={(e) => set("database", e.target.value)} />
              </div>
              <div className="col-span-3">
                <label className="label">SSL/TLS</label>
                <select className="field" value={form.tlsMode} onChange={(e) => set("tlsMode", e.target.value as TlsMode)}>
                  {TLS_MODES.map(([v, l]) => (
                    <option key={v} value={v}>
                      {l}
                    </option>
                  ))}
                </select>
              </div>
              <details className="col-span-6">
                <summary className="cursor-default text-[12px] text-muted">Advanced</summary>
                <div className="mt-2 grid grid-cols-6 gap-3">
                  <div className="col-span-3">
                    <label className="label">Statement timeout (ms)</label>
                    <input className="field" inputMode="numeric" value={form.statementTimeoutMs} onChange={(e) => set("statementTimeoutMs", e.target.value.replace(/\D/g, ""))} />
                  </div>
                  <div className="col-span-3">
                    <label className="label">search_path</label>
                    <input className="field" placeholder="public" value={form.searchPath} onChange={(e) => set("searchPath", e.target.value)} />
                  </div>
                </div>
                <p className="mt-2 text-[11.5px] text-faint">SSH tunnels and client certificates arrive in Phase 2.</p>
              </details>
            </>
          ) : (
            <div className="col-span-6">
              <label className="label">Database file</label>
              <div className="flex gap-2">
                <input className="field font-mono" value={form.sqlitePath} onChange={(e) => set("sqlitePath", e.target.value)} placeholder="/path/to/database.db" />
                <button type="button" className="btn shrink-0" onClick={() => void chooseFile()}>
                  Choose…
                </button>
              </div>
            </div>
          )}
        </div>

        {testResult && (
          <div className={`selectable mx-5 mb-3 rounded-md px-3 py-2 text-[12px] whitespace-pre-wrap ${testResult.ok ? "bg-success/10 text-success" : "bg-danger/10 text-danger"}`}>
            {testResult.ok ? `✓ Connected — ${testResult.text}` : testResult.text}
          </div>
        )}

        <div className="flex items-center gap-2 border-t border-line px-5 py-3">
          <button type="button" className="btn" disabled={busy !== null} onClick={() => void test()}>
            {busy === "test" && <span className="spinner" />} Test
          </button>
          <div className="flex-1" />
          <button type="button" className="btn" onClick={close}>
            Cancel
          </button>
          <button type="button" className="btn" disabled={busy !== null} onClick={() => void save(false)}>
            Save
          </button>
          <button type="submit" className="btn btn-primary" disabled={busy !== null}>
            Save & Connect
          </button>
        </div>
      </form>
    </div>
  );
}

function defaultName(f: Pick<Form, "engine" | "host" | "database" | "sqlitePath">) {
  if (f.engine === "sqlite") return f.sqlitePath.split("/").pop() || "SQLite";
  return [f.database || "postgres", f.host].filter(Boolean).join(" @ ");
}
