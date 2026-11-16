#![allow(dead_code)]

use serde_json::Value;
use snaptable_core::model::*;
use snaptable_core::query::{ColumnMeta, QueryEvent};
use snaptable_drivers::exec::ExecResult;

pub fn connection(engine: Engine) -> Connection {
    Connection {
        id: uuid::Uuid::new_v4().to_string(),
        group_id: None,
        name: "test".into(),
        engine,
        environment_id: None,
        host: None,
        port: None,
        database: None,
        username: None,
        has_password: false,
        sqlite_path: None,
        tls_mode: TlsMode::Disable,
        options: ConnectionOptions::default(),
        created_at: 0,
        updated_at: 0,
        last_connected_at: None,
    }
}

/// Collected results of one statement.
#[derive(Debug, Default)]
pub struct StatementData {
    pub columns: Vec<ColumnMeta>,
    /// Row-major cells reassembled from columnar batches.
    pub rows: Vec<Vec<Value>>,
    pub batches: usize,
    pub row_count: u64,
    pub rows_affected: Option<u64>,
    pub truncated: bool,
    pub error: Option<snaptable_core::AppError>,
}

#[derive(Debug, Default)]
pub struct Collected {
    pub statements: Vec<StatementData>,
    pub done: bool,
    pub cancelled: bool,
}

impl Collected {
    pub fn sink(&mut self) -> impl FnMut(QueryEvent) + Send + '_ {
        move |ev| match ev {
            QueryEvent::StatementStart { .. } => self.statements.push(StatementData::default()),
            QueryEvent::Columns { columns, .. } => {
                self.statements.last_mut().unwrap().columns = columns
            }
            QueryEvent::Rows {
                start_row, columns, ..
            } => {
                let s = self.statements.last_mut().unwrap();
                assert_eq!(
                    start_row as usize,
                    s.rows.len(),
                    "batches must be contiguous"
                );
                s.batches += 1;
                let n = columns.first().map_or(0, Vec::len);
                for r in 0..n {
                    s.rows.push(columns.iter().map(|c| c[r].clone()).collect());
                }
            }
            QueryEvent::StatementEnd {
                row_count,
                rows_affected,
                truncated,
                ..
            } => {
                let s = self.statements.last_mut().unwrap();
                s.row_count = row_count;
                s.rows_affected = rows_affected;
                s.truncated = truncated;
            }
            QueryEvent::StatementError { error, .. } => {
                self.statements.last_mut().unwrap().error = Some(error)
            }
            QueryEvent::Done { cancelled, .. } => {
                self.done = true;
                self.cancelled = cancelled;
            }
        }
    }
}

pub fn stmts(sql: &[&str]) -> Vec<String> {
    sql.iter().map(|s| s.to_string()).collect()
}

pub fn assert_ok(r: &ExecResult) {
    assert_eq!(r.summary.errors, 0, "{:?}", r.outcomes);
    assert!(!r.summary.cancelled);
}
