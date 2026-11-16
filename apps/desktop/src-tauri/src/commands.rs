//! IPC commands. Every command is async (so it runs on the tokio runtime, never the
//! main thread) and returns `Result<T, AppError>`.

use secrecy::SecretString;
use snaptable_core::model::*;
use snaptable_core::query::{
    BrowseRequest, ExecuteMode, ExecuteRequest, ExecuteSummary, QueryEvent,
};
use snaptable_core::time::now_ms;
use snaptable_core::{AppError, AppResult};
use snaptable_drivers::exec::ExecOptions;
use snaptable_drivers::{ConnectParams, DbPool, introspect};
use snaptable_secrets::SecretKey;
use snaptable_sqlgen::{
    BrowseQuery, Dialect, browse_sql, split_statements, statement_at, utf16_to_byte_offset,
};
use tauri::State;
use tauri::ipc::Channel;

use crate::state::AppState;

type Cmd<T> = Result<T, AppError>;

// ---- environments & connections ----------------------------------------------

#[tauri::command]
pub async fn env_list(state: State<'_, AppState>) -> Cmd<Vec<Environment>> {
    state.store.environments().await
}

#[tauri::command]
pub async fn conn_list(state: State<'_, AppState>) -> Cmd<Vec<Connection>> {
    state.store.connections().await
}

#[tauri::command]
pub async fn conn_save(state: State<'_, AppState>, input: ConnectionInput) -> Cmd<Connection> {
    let saved = state
        .store
        .save_connection(state.secrets.as_ref(), input)
        .await?;
    // Settings changed; drop the live connection so the next use reconnects with them.
    state.connections.disconnect(&saved.id).await;
    Ok(saved)
}

#[tauri::command]
pub async fn conn_delete(state: State<'_, AppState>, id: String) -> Cmd<()> {
    state.connections.disconnect(&id).await;
    state
        .store
        .delete_connection(state.secrets.as_ref(), &id)
        .await
}

/// Test unsaved settings. `password: keep` uses the stored Keychain secret.
#[tauri::command]
pub async fn conn_test(state: State<'_, AppState>, input: ConnectionInput) -> Cmd<String> {
    input.validate()?;
    let password = match &input.password {
        SecretChange::Set { value } => Some(SecretString::from(value.clone())),
        SecretChange::Clear => None,
        SecretChange::Keep => match &input.id {
            Some(id) => state.secrets.get(&SecretKey::DbPassword {
                connection_id: id.clone(),
            })?,
            None => None,
        },
    };
    let connection = Connection {
        id: input.id.clone().unwrap_or_default(),
        group_id: None,
        name: input.name.clone(),
        engine: input.engine,
        environment_id: input.environment_id.clone(),
        host: input.host.clone(),
        port: input.port,
        database: input.database.clone(),
        username: input.username.clone(),
        has_password: password.is_some(),
        sqlite_path: input.sqlite_path.clone(),
        tls_mode: input.tls_mode,
        options: input.options.clone(),
        created_at: now_ms(),
        updated_at: now_ms(),
        last_connected_at: None,
    };
    DbPool::test(&ConnectParams {
        connection,
        password,
        read_only: false,
    })
    .await
}

#[tauri::command]
pub async fn conn_connect(state: State<'_, AppState>, id: String) -> Cmd<()> {
    let connection = state.store.connection(&id).await?;
    let password = if connection.has_password {
        state.secrets.get(&SecretKey::DbPassword {
            connection_id: id.clone(),
        })?
    } else {
        None
    };
    let read_only = protection_of(&state, &connection).await? == ProtectionLevel::ReadOnly;
    state
        .connections
        .connect(ConnectParams {
            connection,
            password,
            read_only,
        })
        .await?;
    state.store.touch_connection(&id).await?;
    Ok(())
}

#[tauri::command]
pub async fn conn_disconnect(state: State<'_, AppState>, id: String) -> Cmd<()> {
    state.connections.disconnect(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn conn_connected(state: State<'_, AppState>) -> Cmd<Vec<String>> {
    Ok(state.connections.connected_ids())
}

async fn protection_of(state: &AppState, connection: &Connection) -> AppResult<ProtectionLevel> {
    Ok(match &connection.environment_id {
        Some(env) => state
            .store
            .environment(env)
            .await?
            .map_or(ProtectionLevel::None, |e| e.protection_level),
        None => ProtectionLevel::None,
    })
}

// ---- schema & browsing ----------------------------------------------------------

#[tauri::command]
pub async fn schema_objects(
    state: State<'_, AppState>,
    connection_id: String,
) -> Cmd<Vec<SchemaObject>> {
    let live = state.connections.get(&connection_id)?;
    introspect::list_objects(&live.pool).await
}

#[tauri::command]
pub async fn table_columns(
    state: State<'_, AppState>,
    connection_id: String,
    schema: String,
    table: String,
) -> Cmd<Vec<TableColumn>> {
    let live = state.connections.get(&connection_id)?;
    introspect::table_columns(&live.pool, &schema, &table).await
}

#[tauri::command]
pub async fn table_count(
    state: State<'_, AppState>,
    connection_id: String,
    schema: String,
    table: String,
    filter: Option<String>,
) -> Cmd<i64> {
    let live = state.connections.get(&connection_id)?;
    introspect::count_rows(&live.pool, &schema, &table, filter.as_deref()).await
}

#[tauri::command]
pub async fn table_browse(
    state: State<'_, AppState>,
    req: BrowseRequest,
    on_event: Channel<QueryEvent>,
) -> Cmd<ExecuteSummary> {
    let live = state.connections.get(&req.connection_id)?;
    let pk = introspect::primary_key(&live.pool, &req.schema, &req.table).await?;
    let sql = browse_sql(
        Dialect::from(live.engine),
        &BrowseQuery {
            schema: &req.schema,
            table: &req.table,
            order_by: req
                .order_by
                .as_ref()
                .map(|o| (o.column.as_str(), o.descending)),
            stable_order: &pk,
            filter: req.filter.as_deref(),
            limit: req.limit.clamp(1, 50_000),
            offset: req.offset,
        },
    );
    let opts = ExecOptions {
        max_rows: u64::from(req.limit),
        ..Default::default()
    };
    let mut sink = |ev: QueryEvent| {
        let _ = on_event.send(ev);
    };
    let result = state
        .connections
        .run_on_pool(&req.task_id, &req.connection_id, &[sql], &opts, &mut sink)
        .await?;
    Ok(result.summary)
}

// ---- SQL editor sessions ---------------------------------------------------------

#[tauri::command]
pub async fn session_open(state: State<'_, AppState>, connection_id: String) -> Cmd<String> {
    state.connections.open_session(&connection_id).await
}

#[tauri::command]
pub async fn session_close(state: State<'_, AppState>, session_id: String) -> Cmd<()> {
    state.connections.close_session(&session_id).await;
    Ok(())
}

#[tauri::command]
pub async fn query_execute(
    state: State<'_, AppState>,
    req: ExecuteRequest,
    on_event: Channel<QueryEvent>,
) -> Cmd<ExecuteSummary> {
    let connection_id = state.connections.session_connection(&req.session_id)?;
    let live = state.connections.get(&connection_id)?;
    let dialect = Dialect::from(live.engine);

    let statements: Vec<String> = match req.mode {
        ExecuteMode::All => split_statements(&req.sql, dialect)
            .iter()
            .map(|s| s.text(&req.sql).to_owned())
            .collect(),
        ExecuteMode::AtCursor => {
            let offset = utf16_to_byte_offset(&req.sql, req.cursor_utf16.unwrap_or(0) as usize);
            statement_at(&req.sql, dialect, offset)
                .map(|s| vec![s.text(&req.sql).to_owned()])
                .unwrap_or_default()
        }
    };
    if statements.is_empty() {
        return Err(AppError::invalid("Nothing to run"));
    }

    let opts = ExecOptions {
        max_rows: req.max_rows.map_or(100_000, u64::from),
        ..Default::default()
    };
    let mut sink = |ev: QueryEvent| {
        let _ = on_event.send(ev);
    };
    let result = state
        .connections
        .run_in_session(&req.task_id, &req.session_id, &statements, &opts, &mut sink)
        .await?;

    // Protected environments don't record history by default (SPEC §2.2).
    let connection = state.store.connection(&connection_id).await?;
    if protection_of(&state, &connection).await? == ProtectionLevel::None {
        for o in &result.outcomes {
            let rows = o.rows_affected.and_then(|r| i64::try_from(r).ok());
            if let Err(e) = state
                .store
                .add_history(&connection_id, &o.sql, o.status, rows, o.elapsed_ms as i64)
                .await
            {
                tracing::warn!(error = %e, "failed to record history");
            }
        }
    }
    Ok(result.summary)
}

#[tauri::command]
pub async fn query_cancel(state: State<'_, AppState>, task_id: String) -> Cmd<bool> {
    Ok(state.connections.tasks().cancel(&task_id))
}

// ---- history & workspace -----------------------------------------------------------

#[tauri::command]
pub async fn history_search(
    state: State<'_, AppState>,
    connection_id: Option<String>,
    query: String,
    limit: Option<u32>,
) -> Cmd<Vec<HistoryEntry>> {
    state
        .store
        .search_history(
            connection_id.as_deref(),
            &query,
            limit.unwrap_or(200).min(1_000),
        )
        .await
}

#[tauri::command]
pub async fn workspace_get(
    state: State<'_, AppState>,
    key: String,
) -> Cmd<Option<serde_json::Value>> {
    state.store.workspace_get(&key).await
}

#[tauri::command]
pub async fn workspace_set(
    state: State<'_, AppState>,
    key: String,
    value: serde_json::Value,
) -> Cmd<()> {
    state.store.workspace_set(&key, &value).await
}
