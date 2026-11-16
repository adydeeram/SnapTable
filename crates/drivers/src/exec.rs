//! Statement execution with columnar streaming and cooperative cancellation.

use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde_json::Value;
use snaptable_core::AppError;
use snaptable_core::model::HistoryStatus;
use snaptable_core::query::{ColumnKind, ColumnMeta, ExecuteSummary, QueryEvent};
use snaptable_sqlgen::looks_like_query;
use sqlx::postgres::{PgConnection, PgQueryResult};
use sqlx::sqlite::{SqliteConnection, SqliteQueryResult};
use sqlx::{AssertSqlSafe, Column, Database, Either, Executor, SqlSafeStr, Statement, TypeInfo};
use tokio_util::sync::CancellationToken;

use crate::convert::{ConvertRow, pg_kind, sqlite_kind};
use crate::error::map_sqlx;

#[derive(Debug, Clone)]
pub struct ExecOptions {
    /// Rows past this count are drained from the server but not sent to the UI.
    pub max_rows: u64,
    pub batch_rows: usize,
    pub batch_interval: Duration,
    /// After a server-side cancel, how long to wait for the server to acknowledge
    /// before abandoning the connection.
    pub cancel_grace: Duration,
}

impl Default for ExecOptions {
    fn default() -> Self {
        Self {
            max_rows: 100_000,
            batch_rows: 1_000,
            batch_interval: Duration::from_millis(50),
            cancel_grace: Duration::from_secs(5),
        }
    }
}

/// How a running statement reacts to cancellation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelStyle {
    /// The task registry asks the server to cancel (PostgreSQL `pg_cancel_backend`);
    /// keep reading until the server reports the cancellation so the connection stays
    /// in a clean protocol state.
    ServerSide,
    /// Drop the result stream immediately (SQLite: stops at the next row boundary).
    DropStream,
}

/// A borrowed connection of either engine (pooled or dedicated).
pub enum ConnRef<'a> {
    Postgres(&'a mut PgConnection),
    Sqlite(&'a mut SqliteConnection),
}

#[derive(Debug, Clone)]
pub struct StatementOutcome {
    pub sql: String,
    pub status: HistoryStatus,
    pub rows_affected: Option<u64>,
    pub elapsed_ms: u64,
}

pub struct ExecResult {
    pub summary: ExecuteSummary,
    pub outcomes: Vec<StatementOutcome>,
    /// False when the connection had to be abandoned mid-protocol and must be discarded.
    pub connection_healthy: bool,
}

pub type Sink<'a> = dyn FnMut(QueryEvent) + Send + 'a;

/// Execute statements in order, stopping at the first error or on cancellation.
pub async fn execute_statements(
    conn: ConnRef<'_>,
    statements: &[String],
    opts: &ExecOptions,
    cancel: &CancellationToken,
    sink: &mut Sink<'_>,
) -> ExecResult {
    let started = Instant::now();
    let mut outcomes = Vec::new();
    let mut errors = 0;
    let mut cancelled = false;
    let mut healthy = true;

    let (style, mut conn) = match conn {
        ConnRef::Postgres(c) => (CancelStyle::ServerSide, ConnRef::Postgres(c)),
        ConnRef::Sqlite(c) => (CancelStyle::DropStream, ConnRef::Sqlite(c)),
    };

    for (idx, sql) in statements.iter().enumerate() {
        if cancel.is_cancelled() {
            cancelled = true;
            break;
        }
        let idx = idx as u32;
        sink(QueryEvent::StatementStart {
            index: idx,
            sql: sql.clone(),
        });
        let ctx = StatementCtx {
            idx,
            sql,
            opts,
            cancel,
            style,
        };
        let out = match &mut conn {
            ConnRef::Postgres(c) => run_statement::<sqlx::Postgres>(c, &ctx, pg_kind, sink).await,
            ConnRef::Sqlite(c) => run_statement::<sqlx::Sqlite>(c, &ctx, sqlite_kind, sink).await,
        };
        healthy &= out.healthy;
        let elapsed_ms = out.elapsed.as_millis() as u64;
        let status = match &out.result {
            Ok(_) => HistoryStatus::Success,
            Err(AppError::Cancelled) => HistoryStatus::Cancelled,
            Err(_) => HistoryStatus::Error,
        };
        outcomes.push(StatementOutcome {
            sql: sql.clone(),
            status,
            rows_affected: out.result.as_ref().ok().copied().flatten(),
            elapsed_ms,
        });
        match out.result {
            Ok(_) => {}
            Err(AppError::Cancelled) => {
                sink(QueryEvent::StatementError {
                    index: idx,
                    error: AppError::Cancelled,
                    elapsed_ms,
                });
                cancelled = true;
                break;
            }
            Err(error) => {
                sink(QueryEvent::StatementError {
                    index: idx,
                    error,
                    elapsed_ms,
                });
                errors += 1;
                break;
            }
        }
        if !healthy {
            break;
        }
    }

    let elapsed_ms = started.elapsed().as_millis() as u64;
    sink(QueryEvent::Done {
        elapsed_ms,
        cancelled,
    });
    ExecResult {
        summary: ExecuteSummary {
            statements: outcomes.len() as u32,
            errors,
            cancelled,
            elapsed_ms,
        },
        outcomes,
        connection_healthy: healthy,
    }
}

struct StatementCtx<'a> {
    idx: u32,
    sql: &'a str,
    opts: &'a ExecOptions,
    cancel: &'a CancellationToken,
    style: CancelStyle,
}

struct RunOutcome {
    /// `Ok(rows_affected)` — `None` for statements that returned rows.
    result: Result<Option<u64>, AppError>,
    elapsed: Duration,
    healthy: bool,
}

trait RowsAffected {
    fn affected(&self) -> u64;
}
impl RowsAffected for PgQueryResult {
    fn affected(&self) -> u64 {
        self.rows_affected()
    }
}
impl RowsAffected for SqliteQueryResult {
    fn affected(&self) -> u64 {
        self.rows_affected()
    }
}

struct Batcher {
    buffers: Vec<Vec<Value>>,
    buffered: usize,
    next_start_row: u64,
    last_flush: Instant,
}

impl Batcher {
    fn flush(&mut self, idx: u32, sink: &mut Sink<'_>) {
        if self.buffered == 0 {
            return;
        }
        let width = self.buffers.len();
        let columns =
            std::mem::replace(&mut self.buffers, (0..width).map(|_| Vec::new()).collect());
        sink(QueryEvent::Rows {
            index: idx,
            start_row: self.next_start_row,
            columns,
        });
        self.next_start_row += self.buffered as u64;
        self.buffered = 0;
        self.last_flush = Instant::now();
    }
}

async fn run_statement<DB>(
    conn: &mut DB::Connection,
    ctx: &StatementCtx<'_>,
    kind_of: fn(&str) -> ColumnKind,
    sink: &mut Sink<'_>,
) -> RunOutcome
where
    DB: Database,
    DB::Row: ConvertRow,
    DB::QueryResult: RowsAffected,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
{
    let started = Instant::now();
    let mut batcher: Option<Batcher> = None;
    let mut row_count: u64 = 0;
    let mut rows_affected: u64 = 0;
    let mut cancel_deadline: Option<tokio::time::Instant> = None;
    let mut healthy = true;

    let result: Result<(), AppError> = {
        let mut stream = sqlx::raw_sql(AssertSqlSafe(ctx.sql.to_owned())).fetch_many(&mut *conn);
        loop {
            let next = match cancel_deadline {
                None => {
                    tokio::select! {
                        biased;
                        _ = ctx.cancel.cancelled() => {
                            if ctx.style == CancelStyle::DropStream {
                                break Err(AppError::Cancelled);
                            }
                            cancel_deadline = Some(tokio::time::Instant::now() + ctx.opts.cancel_grace);
                            continue;
                        }
                        n = stream.next() => n,
                    }
                }
                Some(deadline) => match tokio::time::timeout_at(deadline, stream.next()).await {
                    Ok(n) => n,
                    Err(_) => {
                        // Server never acknowledged the cancel; the connection is mid-protocol.
                        healthy = false;
                        break Err(AppError::Cancelled);
                    }
                },
            };
            match next {
                None => {
                    break if cancel_deadline.is_some() {
                        Err(AppError::Cancelled)
                    } else {
                        Ok(())
                    };
                }
                Some(Ok(Either::Left(res))) => rows_affected += res.affected(),
                Some(Ok(Either::Right(row))) => {
                    let b = batcher.get_or_insert_with(|| {
                        let columns = row.column_meta();
                        let width = columns.len();
                        sink(QueryEvent::Columns {
                            index: ctx.idx,
                            columns,
                        });
                        Batcher {
                            buffers: (0..width)
                                .map(|_| Vec::with_capacity(ctx.opts.batch_rows))
                                .collect(),
                            buffered: 0,
                            next_start_row: 0,
                            last_flush: Instant::now(),
                        }
                    });
                    if row_count < ctx.opts.max_rows {
                        row.push_cells(&mut b.buffers);
                        b.buffered += 1;
                        if b.buffered >= ctx.opts.batch_rows
                            || b.last_flush.elapsed() >= ctx.opts.batch_interval
                        {
                            b.flush(ctx.idx, sink);
                        }
                    }
                    row_count += 1;
                }
                Some(Err(e)) => {
                    let err = map_sqlx(e);
                    break if cancel_deadline.is_some() {
                        Err(AppError::Cancelled)
                    } else {
                        Err(err)
                    };
                }
            }
        }
    };

    if let Err(error) = result {
        return RunOutcome {
            result: Err(error),
            elapsed: started.elapsed(),
            healthy,
        };
    }

    let returned_rows = match batcher.as_mut() {
        Some(b) => {
            b.flush(ctx.idx, sink);
            true
        }
        None => {
            // No rows: still show column headers for queries (e.g. `SELECT … WHERE false`).
            if looks_like_query(ctx.sql)
                && let Ok(stmt) = (&mut *conn)
                    .prepare(AssertSqlSafe(ctx.sql.to_owned()).into_sql_str())
                    .await
                && !stmt.columns().is_empty()
            {
                let columns = stmt
                    .columns()
                    .iter()
                    .map(|c| {
                        let type_name = c.type_info().name().to_owned();
                        ColumnMeta {
                            name: c.name().to_owned(),
                            kind: kind_of(&type_name),
                            type_name,
                        }
                    })
                    .collect();
                sink(QueryEvent::Columns {
                    index: ctx.idx,
                    columns,
                });
                true
            } else {
                false
            }
        }
    };

    let elapsed = started.elapsed();
    sink(QueryEvent::StatementEnd {
        index: ctx.idx,
        rows_affected: (!returned_rows).then_some(rows_affected),
        row_count,
        truncated: row_count > ctx.opts.max_rows,
        elapsed_ms: elapsed.as_millis() as u64,
    });
    RunOutcome {
        result: Ok((!returned_rows).then_some(rows_affected)),
        elapsed,
        healthy,
    }
}
