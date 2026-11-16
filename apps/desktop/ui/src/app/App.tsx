import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { ConnectionDialog } from "../features/connections/ConnectionDialog";
import { HistoryPanel } from "../features/history/HistoryPanel";
import { CommandPalette } from "../features/palette/CommandPalette";
import { QueryView } from "../features/sql-editor/QueryView";
import { TableView } from "../features/table/TableView";
import { ApiPlaceholder } from "../shell/ApiPlaceholder";
import { EnvBanner } from "../shell/EnvBanner";
import { Sidebar } from "../shell/Sidebar";
import { TabStrip } from "../shell/TabStrip";
import { TitleBar } from "../shell/TitleBar";
import { Toasts } from "../shell/Toasts";
import { Welcome } from "../shell/Welcome";
import { useConnections } from "../stores/connections";
import { toast } from "../stores/toasts";
import { useWorkspace } from "../stores/workspace";
import { errorMessage } from "../ipc/api";
import { runCommand } from "./commands";

export function App() {
  const mode = useWorkspace((s) => s.mode);
  const tabs = useWorkspace((s) => s.tabs);
  const activeId = useWorkspace((s) => s.activeId);
  const sidebarVisible = useWorkspace((s) => s.sidebarVisible);
  const overlay = useWorkspace((s) => s.overlay);
  const dialog = useWorkspace((s) => s.connectionDialog);

  useEffect(() => {
    (async () => {
      try {
        await useConnections.getState().load();
        const ids = new Set(useConnections.getState().connections.map((c) => c.id));
        await useWorkspace.getState().restore(ids);
      } catch (e) {
        toast.error("Failed to load workspace", errorMessage(e));
      }
    })();
    const unlisten = listen<string>("menu", (e) => runCommand(e.payload));
    return () => {
      void unlisten.then((f) => f());
    };
  }, []);

  // Escape closes overlays; otherwise it cancels a running query in the active tab.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.isComposing) return;
      const ws = useWorkspace.getState();
      if (ws.overlay) ws.setOverlay(null);
      else if (!ws.connectionDialog.open) runCommand("query.cancel");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="flex h-full flex-col bg-bg">
      <TitleBar />
      <EnvBanner />
      <div className="flex min-h-0 flex-1">
        {sidebarVisible && <Sidebar />}
        <main className="flex min-w-0 flex-1 flex-col">
          {mode === "api" ? (
            <ApiPlaceholder />
          ) : (
            <>
              <TabStrip />
              <div className="relative min-h-0 flex-1">
                {tabs.length === 0 && <Welcome />}
                {tabs.map((t) => (
                  <div key={t.id} className="absolute inset-0 flex flex-col" hidden={t.id !== activeId}>
                    {t.kind === "table" ? (
                      <TableView tab={t} active={t.id === activeId} />
                    ) : (
                      <QueryView tab={t} active={t.id === activeId} />
                    )}
                  </div>
                ))}
              </div>
            </>
          )}
        </main>
      </div>
      {(overlay === "palette" || overlay === "quickOpen") && <CommandPalette mode={overlay} />}
      {overlay === "history" && <HistoryPanel />}
      {dialog.open && <ConnectionDialog editId={dialog.editId} />}
      <Toasts />
    </div>
  );
}
