//! Cancellable task registry (SPEC §1.4).
//!
//! The UI generates a `task_id` before invoking a long-running command, so it can
//! cancel at any time — even before the backend has registered the task. Cancelling
//! fires the task's [`CancellationToken`] and, for PostgreSQL, also asks the server to
//! stop the backend process running the query.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use sqlx::postgres::PgPool;
use tokio_util::sync::CancellationToken;

/// How to stop the work on the database server, beyond dropping our side.
#[derive(Clone)]
pub enum ServerCancel {
    /// `pg_cancel_backend(pid)` issued over a separate pooled connection.
    Postgres { pool: PgPool, backend_pid: i32 },
}

impl ServerCancel {
    async fn fire(self) {
        match self {
            ServerCancel::Postgres { pool, backend_pid } => {
                if let Err(e) = sqlx::query("SELECT pg_cancel_backend($1)")
                    .bind(backend_pid)
                    .execute(&pool)
                    .await
                {
                    tracing::warn!(error = %e, backend_pid, "pg_cancel_backend failed");
                }
            }
        }
    }
}

struct Entry {
    token: CancellationToken,
    server: Option<ServerCancel>,
}

/// Remember this many cancel requests that arrived before their task registered.
const EARLY_CANCEL_MEMORY: usize = 64;

#[derive(Default)]
struct Inner {
    tasks: HashMap<String, Entry>,
    early_cancels: VecDeque<String>,
}

#[derive(Clone, Default)]
pub struct TaskRegistry {
    inner: Arc<Mutex<Inner>>,
}

/// Unregisters the task when dropped.
pub struct TaskGuard {
    registry: TaskRegistry,
    task_id: String,
    pub token: CancellationToken,
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        self.registry
            .inner
            .lock()
            .unwrap()
            .tasks
            .remove(&self.task_id);
    }
}

impl TaskRegistry {
    pub fn register(&self, task_id: &str, server: Option<ServerCancel>) -> TaskGuard {
        let token = CancellationToken::new();
        let mut inner = self.inner.lock().unwrap();
        let was_cancelled_early = match inner.early_cancels.iter().position(|id| id == task_id) {
            Some(pos) => {
                inner.early_cancels.remove(pos);
                true
            }
            None => false,
        };
        if was_cancelled_early {
            token.cancel();
        }
        inner.tasks.insert(
            task_id.to_owned(),
            Entry {
                token: token.clone(),
                server,
            },
        );
        TaskGuard {
            registry: self.clone(),
            task_id: task_id.to_owned(),
            token,
        }
    }

    /// Cancel a task. Returns `true` if the task was running.
    pub fn cancel(&self, task_id: &str) -> bool {
        let (token, server) = {
            let mut inner = self.inner.lock().unwrap();
            match inner.tasks.get(task_id) {
                Some(e) => (e.token.clone(), e.server.clone()),
                None => {
                    if inner.early_cancels.len() >= EARLY_CANCEL_MEMORY {
                        inner.early_cancels.pop_front();
                    }
                    inner.early_cancels.push_back(task_id.to_owned());
                    return false;
                }
            }
        };
        if token.is_cancelled() {
            return true;
        }
        token.cancel();
        if let Some(server) = server {
            tokio::spawn(server.fire());
        }
        true
    }

    pub fn running(&self) -> usize {
        self.inner.lock().unwrap().tasks.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancel_running_task() {
        let reg = TaskRegistry::default();
        let guard = reg.register("t1", None);
        assert!(!guard.token.is_cancelled());
        assert!(reg.cancel("t1"));
        assert!(guard.token.is_cancelled());
        drop(guard);
        assert_eq!(reg.running(), 0);
    }

    #[tokio::test]
    async fn cancel_before_register() {
        let reg = TaskRegistry::default();
        assert!(!reg.cancel("early"));
        let guard = reg.register("early", None);
        assert!(guard.token.is_cancelled());
        // The early-cancel marker is consumed.
        drop(guard);
        assert!(!reg.register("early", None).token.is_cancelled());
    }
}
