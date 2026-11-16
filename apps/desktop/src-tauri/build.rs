//! Declares the app command set so each command needs an explicit capability grant
//! (SPEC §4.1). Keep in sync with `generate_handler!` in src/lib.rs.

const COMMANDS: &[&str] = &[
    "env_list",
    "conn_list",
    "conn_save",
    "conn_delete",
    "conn_test",
    "conn_connect",
    "conn_disconnect",
    "conn_connected",
    "schema_objects",
    "table_columns",
    "table_count",
    "table_browse",
    "session_open",
    "session_close",
    "query_execute",
    "query_cancel",
    "history_search",
    "workspace_get",
    "workspace_set",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
