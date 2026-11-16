//! SnapTable desktop shell: wires the core crates into Tauri commands.
//!
//! This crate is intentionally thin (SPEC §1.1): validation, state lookup and
//! translation to/from IPC types. Logic lives in the `crates/*` workspace members.

mod commands;
mod menu;
mod state;

use std::sync::Arc;

use snaptable_secrets::{KeychainStore, MemoryStore, SecretStore};
use tauri::Manager;

use crate::state::AppState;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // SNAPTABLE_DATA_DIR isolates dev/test runs from the real app data.
            let data_dir = match std::env::var_os("SNAPTABLE_DATA_DIR") {
                Some(dir) => std::path::PathBuf::from(dir),
                None => app.path().app_data_dir()?,
            };
            let db_path = data_dir.join("snaptable.db");
            let store = tauri::async_runtime::block_on(snaptable_store::Store::open(&db_path))?;
            let secrets: Arc<dyn SecretStore> = match std::env::var("SNAPTABLE_SECRETS").as_deref()
            {
                Ok("memory") => {
                    tracing::warn!("using in-memory secret store (SNAPTABLE_SECRETS=memory)");
                    Arc::new(MemoryStore::default())
                }
                _ => Arc::new(KeychainStore::init()?),
            };
            tracing::info!(path = %db_path.display(), "metadata store ready");
            app.manage(AppState::new(store, secrets));
            menu::install(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::env_list,
            commands::conn_list,
            commands::conn_save,
            commands::conn_delete,
            commands::conn_test,
            commands::conn_connect,
            commands::conn_disconnect,
            commands::conn_connected,
            commands::schema_objects,
            commands::table_columns,
            commands::table_count,
            commands::table_browse,
            commands::session_open,
            commands::session_close,
            commands::query_execute,
            commands::query_cancel,
            commands::history_search,
            commands::workspace_get,
            commands::workspace_set,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SnapTable");
}
