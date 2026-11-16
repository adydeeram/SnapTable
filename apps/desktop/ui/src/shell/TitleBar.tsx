import { runCommand, shortcutFor } from "../app/commands";
import { IconSearch, IconSidebar } from "../lib/icons";
import { useConnections, useEnvironment } from "../stores/connections";
import { useActiveTab, useWorkspace } from "../stores/workspace";

export function TitleBar() {
  const mode = useWorkspace((s) => s.mode);
  const tab = useActiveTab();
  const connection = useConnections((s) => s.connections.find((c) => c.id === tab?.connectionId));
  const env = useEnvironment(tab?.connectionId);

  return (
    // Leaves room for the traffic lights (overlay title bar).
    <header
      data-tauri-drag-region
      className="drag-region flex h-11 shrink-0 items-center gap-3 border-b border-line bg-sidebar pr-3 pl-[84px]"
    >
      <button className="btn-ghost" style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties} onClick={() => runCommand("view.toggleSidebar")} title={`Toggle sidebar ${shortcutFor("view.toggleSidebar")}`}>
        <IconSidebar />
      </button>
      <div
        role="tablist"
        aria-label="Mode"
        className="flex rounded-md border border-line bg-surface p-0.5"
        style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}
      >
        {(["db", "api"] as const).map((m) => (
          <button
            key={m}
            role="tab"
            aria-selected={mode === m}
            onClick={() => runCommand(m === "db" ? "mode.db" : "mode.api")}
            title={shortcutFor(m === "db" ? "mode.db" : "mode.api")}
            className={`h-6 rounded px-3 text-[12px] font-medium ${mode === m ? "bg-elev text-fg shadow-sm" : "text-muted hover:text-fg"}`}
          >
            {m === "db" ? "Database" : "API"}
          </button>
        ))}
      </div>

      <div data-tauri-drag-region className="flex min-w-0 flex-1 items-center justify-center gap-2 text-[12px] text-muted">
        {connection && mode === "db" && (
          <>
            <span className="h-2 w-2 shrink-0 rounded-full" style={{ background: env?.color ?? "var(--fg-faint)" }} />
            <span className="truncate font-medium text-fg">{connection.name}</span>
            {env && <span className="truncate">· {env.name}</span>}
          </>
        )}
      </div>

      <button
        className="flex h-7 w-56 items-center gap-2 rounded-md border border-line bg-surface px-2 text-[12px] text-faint hover:text-muted"
        style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}
        onClick={() => runCommand("app.palette")}
      >
        <IconSearch />
        <span className="flex-1 truncate text-left whitespace-nowrap">Search commands & tables</span>
        <span className="kbd">{shortcutFor("app.palette")}</span>
      </button>
    </header>
  );
}
