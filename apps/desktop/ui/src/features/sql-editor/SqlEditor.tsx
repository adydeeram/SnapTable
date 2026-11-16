// Monaco-based SQL editor, loaded lazily (Monaco is the largest dependency).

import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from "react";
import type * as Monaco from "monaco-editor/editor/editor.api";
import { commandDefs, runCommand } from "../../app/commands";
import { EDITOR_NATIVE, toMonacoKeybinding } from "./keybindings";

export type SqlEditorHandle = {
  getValue: () => string;
  /** Selected text, or null when the selection is empty. */
  getSelection: () => string | null;
  /** Cursor position as a UTF-16 offset into the full text. */
  getCursorOffset: () => number;
  focus: () => void;
  showError: (statementSql: string, position: number | null | undefined, message: string) => void;
  clearErrors: () => void;
};

type Props = { initialValue: string; language: "pgsql" | "sql"; onChange: (value: string) => void; active: boolean };

let monacoPromise: Promise<typeof Monaco> | null = null;
const loadMonaco = () => (monacoPromise ??= import("./monaco").then((m) => m.monaco));

export const SqlEditor = forwardRef<SqlEditorHandle, Props>(function SqlEditor({ initialValue, language, onChange, active }, ref) {
  const host = useRef<HTMLDivElement>(null);
  const editorRef = useRef<Monaco.editor.IStandaloneCodeEditor | null>(null);
  const monacoRef = useRef<typeof Monaco | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;
    void loadMonaco().then((monaco) => {
      if (disposed || !host.current) return;
      monacoRef.current = monaco;
      const dark = window.matchMedia("(prefers-color-scheme: dark)");
      const editor = monaco.editor.create(host.current, {
        value: initialValue,
        language,
        theme: dark.matches ? "vs-dark" : "vs",
        automaticLayout: true,
        minimap: { enabled: false },
        fontFamily: 'ui-monospace, "SF Mono", Menlo, monospace',
        fontSize: 13,
        lineHeight: 20,
        scrollBeyondLastLine: false,
        renderLineHighlight: "line",
        tabSize: 2,
        wordWrap: "off",
        padding: { top: 8 },
        fixedOverflowWidgets: true,
        quickSuggestions: false,
        contextmenu: true,
      });
      editorRef.current = editor;

      // App shortcuts take precedence inside the editor.
      for (const cmd of commandDefs) {
        if (!cmd.accelerator || EDITOR_NATIVE.has(cmd.id)) continue;
        const kb = toMonacoKeybinding(monaco, cmd.accelerator);
        if (kb !== null) editor.addCommand(kb, () => runCommand(cmd.id));
      }

      const sub = editor.onDidChangeModelContent(() => onChangeRef.current(editor.getValue()));
      const onScheme = (e: MediaQueryListEvent) => monaco.editor.setTheme(e.matches ? "vs-dark" : "vs");
      dark.addEventListener("change", onScheme);
      setLoading(false);
      cleanup = () => {
        sub.dispose();
        dark.removeEventListener("change", onScheme);
        editor.getModel()?.dispose();
        editor.dispose();
      };
    });
    return () => {
      disposed = true;
      cleanup?.();
    };
    // The editor owns its content after mount.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const m = editorRef.current?.getModel();
    if (m && monacoRef.current) monacoRef.current.editor.setModelLanguage(m, language);
  }, [language]);

  useEffect(() => {
    if (active) requestAnimationFrame(() => editorRef.current?.focus());
  }, [active, loading]);

  useImperativeHandle(ref, () => ({
    getValue: () => editorRef.current?.getValue() ?? initialValue,
    getSelection: () => {
      const ed = editorRef.current;
      const sel = ed?.getSelection();
      if (!ed || !sel || sel.isEmpty()) return null;
      return ed.getModel()?.getValueInRange(sel) ?? null;
    },
    getCursorOffset: () => {
      const ed = editorRef.current;
      const pos = ed?.getPosition();
      return ed && pos ? (ed.getModel()?.getOffsetAt(pos) ?? 0) : 0;
    },
    focus: () => editorRef.current?.focus(),
    showError: (statementSql, position, message) => {
      const ed = editorRef.current;
      const monaco = monacoRef.current;
      const model = ed?.getModel();
      if (!ed || !monaco || !model) return;
      const text = model.getValue();
      const start = text.indexOf(statementSql);
      if (start < 0) return;
      const at = start + (position ? position - 1 : 0);
      const p = model.getPositionAt(at);
      const end = position ? model.getPositionAt(at + 1) : model.getPositionAt(start + statementSql.length);
      monaco.editor.setModelMarkers(model, "snaptable", [
        {
          severity: monaco.MarkerSeverity.Error,
          message,
          startLineNumber: p.lineNumber,
          startColumn: p.column,
          endLineNumber: end.lineNumber,
          endColumn: Math.max(end.column, p.column + 1),
        },
      ]);
      ed.revealPositionInCenterIfOutsideViewport(p);
    },
    clearErrors: () => {
      const model = editorRef.current?.getModel();
      if (model && monacoRef.current) monacoRef.current.editor.setModelMarkers(model, "snaptable", []);
    },
  }));

  return (
    <div className="relative h-full w-full">
      <div ref={host} className="absolute inset-0" />
      {loading && <div className="absolute inset-0 flex items-center justify-center text-faint">Loading editor…</div>}
    </div>
  );
});
