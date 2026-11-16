// Curated Monaco bundle: core editor API + all editor features (find, folding,
// suggest, multi-cursor…) + only the SQL grammars — no TS/CSS/HTML/JSON language
// services. Bundled locally; no CDN (SPEC §4.1). Subpaths resolve via monaco-editor's
// `exports` map ("./*" → "./esm/vs/*.js").

import "monaco-editor/features/register.all";
import "monaco-editor/languages/definitions/sql/register";
import "monaco-editor/languages/definitions/pgsql/register";
import EditorWorker from "monaco-editor/editor/editor.worker?worker";
import * as monaco from "monaco-editor/editor/editor.api";

self.MonacoEnvironment = {
  getWorker: () => new EditorWorker(),
};

export { monaco };
