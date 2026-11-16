// Translate shortcuts.json accelerators into Monaco keybindings so app shortcuts win
// over Monaco defaults (⌘↵ insert-line, ⌘K chord prefix, ⌘. quick-fix) — SPEC §3.3.

import type * as Monaco from "monaco-editor/editor/editor.api";

const NAMED: Record<string, string> = {
  Enter: "Enter",
  "[": "BracketLeft",
  "]": "BracketRight",
  ".": "Period",
  ",": "Comma",
  "/": "Slash",
  Backspace: "Backspace",
  Escape: "Escape",
};

export function toMonacoKeybinding(monaco: typeof Monaco, accelerator: string): number | null {
  let mods = 0;
  let key: number | undefined;
  for (const part of accelerator.split("+")) {
    if (part === "CmdOrCtrl" || part === "Cmd") mods |= monaco.KeyMod.CtrlCmd;
    else if (part === "Shift") mods |= monaco.KeyMod.Shift;
    else if (part === "Alt" || part === "Option") mods |= monaco.KeyMod.Alt;
    else if (part === "Ctrl") mods |= monaco.KeyMod.WinCtrl;
    else {
      const name = NAMED[part] ?? (/^[0-9]$/.test(part) ? `Digit${part}` : /^[A-Za-z]$/.test(part) ? `Key${part.toUpperCase()}` : part);
      key = (monaco.KeyCode as unknown as Record<string, number>)[name];
    }
  }
  return key === undefined ? null : mods | key;
}

/** Commands whose Monaco-native behaviour should be kept when the editor has focus. */
export const EDITOR_NATIVE = new Set(["data.filter"]);
