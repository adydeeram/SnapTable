import { IconClose } from "../lib/icons";
import { useToasts } from "../stores/toasts";

export function Toasts() {
  const toasts = useToasts((s) => s.toasts);
  const dismiss = useToasts((s) => s.dismiss);
  return (
    <div className="pointer-events-none fixed right-4 bottom-4 z-50 flex w-96 flex-col gap-2" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className="pointer-events-auto rounded-lg border border-line bg-elev p-3 shadow-[var(--shadow)]">
          <div className="flex items-start gap-2">
            <span
              className="mt-1 h-2 w-2 shrink-0 rounded-full"
              style={{ background: t.tone === "error" ? "var(--danger)" : t.tone === "success" ? "var(--success)" : "var(--accent)" }}
            />
            <div className="min-w-0 flex-1">
              <div className="text-[12.5px] font-semibold">{t.title}</div>
              {t.body && <div className="selectable mt-0.5 text-[12px] whitespace-pre-wrap text-muted">{t.body}</div>}
            </div>
            <button className="text-faint hover:text-fg" onClick={() => dismiss(t.id)} aria-label="Dismiss">
              <IconClose size={12} />
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}
