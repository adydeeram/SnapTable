import { runCommand, shortcutFor } from "../app/commands";
import { useConnections } from "../stores/connections";

export function Welcome() {
  const hasConnections = useConnections((s) => s.connections.length > 0);
  const rows: Array<[string, string]> = [
    ["Command palette", "app.palette"],
    ["New connection", "conn.new"],
    ["New query tab", "tab.newQuery"],
    ["Quick open table", "app.quickOpen"],
    ["Query history", "app.history"],
  ];
  return (
    <div className="flex h-full flex-col items-center justify-center gap-6">
      <div className="text-center">
        <div className="text-[20px] font-semibold tracking-tight">SnapTable</div>
        <div className="mt-1 text-muted">
          {hasConnections ? "Pick a table in the sidebar, or open a query tab." : "Add a connection to get started."}
        </div>
      </div>
      {!hasConnections && (
        <button className="btn btn-primary" onClick={() => runCommand("conn.new")}>
          New Connection
        </button>
      )}
      <dl className="grid grid-cols-[auto_auto] gap-x-6 gap-y-1.5 text-[12px]">
        {rows.map(([label, id]) => (
          <div key={id} className="contents">
            <dt className="text-muted">{label}</dt>
            <dd className="text-right">
              <span className="kbd">{shortcutFor(id)}</span>
            </dd>
          </div>
        ))}
      </dl>
    </div>
  );
}
