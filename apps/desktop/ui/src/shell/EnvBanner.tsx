import { useEnvironment } from "../stores/connections";
import { useActiveTab, useWorkspace } from "../stores/workspace";

/** Full-width safety banner for protected environments (SPEC §3.1, §4.5). */
export function EnvBanner() {
  const tab = useActiveTab();
  const mode = useWorkspace((s) => s.mode);
  const env = useEnvironment(tab?.connectionId);
  if (mode !== "db" || !env || env.protectionLevel === "none") return null;
  const text =
    env.protectionLevel === "readOnly" ? "read-only session · writes are blocked by the database" : "writes require care";
  return (
    <div
      role="status"
      className="flex h-6 shrink-0 items-center justify-center gap-2 text-[11px] font-semibold tracking-wide text-white uppercase"
      style={{ background: env.color }}
    >
      {env.name} <span className="font-normal normal-case opacity-90">· {text}</span>
    </div>
  );
}
