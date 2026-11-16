//! Live connections and pinned editor sessions (SPEC §1.5, ADR 0005).
//!
//! Each live connection owns a small shared pool for browsing/introspection. Each SQL
//! editor tab gets a *session*: a dedicated connection that keeps transactions, `SET`
//! and temp tables alive between runs.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use snaptable_core::model::Engine;
use snaptable_core::{AppError, AppResult};
use sqlx::Connection as _;

use crate::error::map_sqlx;
use crate::exec::{ConnRef, ExecOptions, ExecResult, Sink, execute_statements};
use crate::pool::{ConnectParams, DbConn, DbPool};
use crate::tasks::{ServerCancel, TaskRegistry};

pub struct LiveConnection {
    pub connection_id: String,
    pub engine: Engine,
    pub pool: DbPool,
    pub read_only: bool,
}

pub struct Session {
    pub id: String,
    pub connection_id: String,
    conn: Option<DbConn>,
    backend_pid: Option<i32>,
}

#[derive(Default)]
pub struct ConnectionManager {
    live: RwLock<HashMap<String, Arc<LiveConnection>>>,
    sessions: Mutex<HashMap<String, Arc<tokio::sync::Mutex<Session>>>>,
    tasks: TaskRegistry,
}

impl ConnectionManager {
    pub fn tasks(&self) -> &TaskRegistry {
        &self.tasks
    }

    /// Connect (or return the existing live connection).
    pub async fn connect(&self, params: ConnectParams) -> AppResult<Arc<LiveConnection>> {
        let id = params.connection.id.clone();
        if let Some(existing) = self.live.read().unwrap().get(&id) {
            return Ok(existing.clone());
        }
        let pool = DbPool::connect(&params).await?;
        let live = Arc::new(LiveConnection {
            connection_id: id.clone(),
            engine: params.connection.engine,
            pool,
            read_only: params.read_only,
        });
        // Another task may have connected concurrently; keep the first one.
        let mut map = self.live.write().unwrap();
        if let Some(existing) = map.get(&id) {
            let dup = live.pool.clone();
            tokio::spawn(async move { dup.close().await });
            return Ok(existing.clone());
        }
        map.insert(id, live.clone());
        Ok(live)
    }

    pub async fn disconnect(&self, connection_id: &str) {
        let live = self.live.write().unwrap().remove(connection_id);
        let sessions: Vec<_> = {
            let mut map = self.sessions.lock().unwrap();
            let ids: Vec<String> = map
                .iter()
                .filter(|(_, s)| {
                    s.try_lock()
                        .map(|s| s.connection_id == connection_id)
                        .unwrap_or(true)
                })
                .map(|(id, _)| id.clone())
                .collect();
            ids.into_iter().filter_map(|id| map.remove(&id)).collect()
        };
        drop(sessions);
        if let Some(live) = live {
            live.pool.close().await;
        }
    }

    pub fn get(&self, connection_id: &str) -> AppResult<Arc<LiveConnection>> {
        self.live
            .read()
            .unwrap()
            .get(connection_id)
            .cloned()
            .ok_or_else(|| AppError::connection("Not connected"))
    }

    pub fn connected_ids(&self) -> Vec<String> {
        self.live.read().unwrap().keys().cloned().collect()
    }

    /// Open a dedicated session for an editor tab. Connects eagerly so errors surface now.
    pub async fn open_session(&self, connection_id: &str) -> AppResult<String> {
        let live = self.get(connection_id)?;
        let (conn, pid) = open_dedicated(&live).await?;
        let id = uuid::Uuid::now_v7().to_string();
        let session = Session {
            id: id.clone(),
            connection_id: connection_id.to_owned(),
            conn: Some(conn),
            backend_pid: pid,
        };
        self.sessions
            .lock()
            .unwrap()
            .insert(id.clone(), Arc::new(tokio::sync::Mutex::new(session)));
        Ok(id)
    }

    pub async fn close_session(&self, session_id: &str) {
        let session = self.sessions.lock().unwrap().remove(session_id);
        if let Some(session) = session {
            let mut s = session.lock().await;
            if let Some(conn) = s.conn.take() {
                // Closing the connection rolls back any open transaction server-side.
                let _ = match conn {
                    DbConn::Postgres(c) => c.close().await,
                    DbConn::Sqlite(c) => c.close().await,
                };
            }
        }
    }

    pub fn session_connection(&self, session_id: &str) -> AppResult<String> {
        let session = self.session(session_id)?;
        let s = session
            .try_lock()
            .map_err(|_| AppError::invalid("A query is already running in this tab"))?;
        Ok(s.connection_id.clone())
    }

    fn session(&self, session_id: &str) -> AppResult<Arc<tokio::sync::Mutex<Session>>> {
        self.sessions
            .lock()
            .unwrap()
            .get(session_id)
            .cloned()
            .ok_or_else(|| AppError::not_found("Session not found (was the connection closed?)"))
    }

    /// Run statements in a pinned session. Only one run per session at a time.
    pub async fn run_in_session(
        &self,
        task_id: &str,
        session_id: &str,
        statements: &[String],
        opts: &ExecOptions,
        sink: &mut Sink<'_>,
    ) -> AppResult<ExecResult> {
        let session = self.session(session_id)?;
        let mut s = session
            .try_lock()
            .map_err(|_| AppError::invalid("A query is already running in this tab"))?;
        let live = self.get(&s.connection_id)?;
        if s.conn.is_none() {
            let (conn, pid) = open_dedicated(&live).await?;
            s.conn = Some(conn);
            s.backend_pid = pid;
        }
        let server = match (&live.pool, s.backend_pid) {
            (DbPool::Postgres { pool, .. }, Some(pid)) => Some(ServerCancel::Postgres {
                pool: pool.clone(),
                backend_pid: pid,
            }),
            _ => None,
        };
        let guard = self.tasks.register(task_id, server);
        let conn_ref = match s.conn.as_mut().expect("connection opened above") {
            DbConn::Postgres(c) => ConnRef::Postgres(c),
            DbConn::Sqlite(c) => ConnRef::Sqlite(c),
        };
        let result = execute_statements(conn_ref, statements, opts, &guard.token, sink).await;
        if !result.connection_healthy {
            s.conn = None;
            s.backend_pid = None;
        }
        Ok(result)
    }

    /// Run statements on the shared pool (table browsing). Not for user transactions.
    pub async fn run_on_pool(
        &self,
        task_id: &str,
        connection_id: &str,
        statements: &[String],
        opts: &ExecOptions,
        sink: &mut Sink<'_>,
    ) -> AppResult<ExecResult> {
        let live = self.get(connection_id)?;
        match &live.pool {
            DbPool::Postgres { pool, .. } => {
                let mut conn = pool.acquire().await.map_err(map_sqlx)?;
                let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                    .fetch_one(&mut *conn)
                    .await
                    .map_err(map_sqlx)?;
                let guard = self.tasks.register(
                    task_id,
                    Some(ServerCancel::Postgres {
                        pool: pool.clone(),
                        backend_pid: pid,
                    }),
                );
                let result = execute_statements(
                    ConnRef::Postgres(&mut conn),
                    statements,
                    opts,
                    &guard.token,
                    sink,
                )
                .await;
                if !result.connection_healthy {
                    drop(conn.detach());
                }
                Ok(result)
            }
            DbPool::Sqlite { pool, .. } => {
                let mut conn = pool.acquire().await.map_err(map_sqlx)?;
                let guard = self.tasks.register(task_id, None);
                Ok(execute_statements(
                    ConnRef::Sqlite(&mut conn),
                    statements,
                    opts,
                    &guard.token,
                    sink,
                )
                .await)
            }
        }
    }
}

async fn open_dedicated(live: &LiveConnection) -> AppResult<(DbConn, Option<i32>)> {
    let mut conn = live.pool.open_dedicated().await?;
    let pid = match &mut conn {
        DbConn::Postgres(c) => Some(
            sqlx::query_scalar::<_, i32>("SELECT pg_backend_pid()")
                .fetch_one(&mut *c)
                .await
                .map_err(map_sqlx)?,
        ),
        DbConn::Sqlite(_) => None,
    };
    Ok((conn, pid))
}
