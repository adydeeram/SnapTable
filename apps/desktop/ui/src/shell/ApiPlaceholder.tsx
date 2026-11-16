import { shortcutFor } from "../app/commands";

export function ApiPlaceholder() {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 text-center">
      <div className="text-[15px] font-semibold">API client</div>
      <p className="max-w-sm text-muted">
        The request builder, environments and response inspector arrive in Phase 3 of the roadmap. Press{" "}
        <span className="kbd">{shortcutFor("mode.db")}</span> to return to Database mode.
      </p>
    </div>
  );
}
