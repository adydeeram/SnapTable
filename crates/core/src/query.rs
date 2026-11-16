//! Query execution requests and the streamed event protocol (SPEC §1.3).
//!
//! Cells travel as JSON values:
//! - `null`, `true/false`, numbers (only when exactly representable in JS), strings
//! - `{"$bytes": "<hex preview>", "len": n}` for binary values
//! - `{"$trunc": "<text preview>", "len": n}` for text longer than [`MAX_INLINE_TEXT`]

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use crate::AppError;

/// Largest text value sent inline; longer values are truncated with a marker.
pub const MAX_INLINE_TEXT: usize = 64 * 1024;
/// Number of leading bytes of a binary value rendered as a hex preview.
pub const BYTES_PREVIEW: usize = 64;
/// Largest integer that survives a round-trip through a JS `number`.
pub const MAX_SAFE_INT: i64 = (1 << 53) - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ColumnKind {
    Number,
    Bool,
    Text,
    Json,
    Bytes,
    Temporal,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ColumnMeta {
    pub name: String,
    pub type_name: String,
    pub kind: ColumnKind,
}

/// Run SQL inside a pinned editor session.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExecuteRequest {
    pub task_id: String,
    pub session_id: String,
    pub sql: String,
    pub mode: ExecuteMode,
    /// Cursor position as a UTF-16 offset (what the editor reports). Used by `atCursor`.
    pub cursor_utf16: Option<u32>,
    pub max_rows: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ExecuteMode {
    /// Execute every statement in `sql` (also used for an explicit selection).
    All,
    /// Execute only the statement containing `cursor_utf16`.
    AtCursor,
}

/// Browse a table page (read-only, uses the shared pool).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BrowseRequest {
    pub task_id: String,
    pub connection_id: String,
    pub schema: String,
    pub table: String,
    pub offset: u32,
    pub limit: u32,
    pub order_by: Option<OrderBy>,
    /// Raw SQL boolean expression appended as `WHERE (...)`.
    pub filter: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OrderBy {
    pub column: String,
    pub descending: bool,
}

/// Events streamed over a Tauri `Channel` while a query runs.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum QueryEvent {
    StatementStart {
        index: u32,
        sql: String,
    },
    Columns {
        index: u32,
        columns: Vec<ColumnMeta>,
    },
    /// Columnar batch: `columns[c][r]` is the cell at row `start_row + r`, column `c`.
    Rows {
        index: u32,
        #[ts(type = "number")]
        start_row: u64,
        #[ts(type = "Array<Array<unknown>>")]
        columns: Vec<Vec<Value>>,
    },
    StatementEnd {
        index: u32,
        #[ts(type = "number | null")]
        rows_affected: Option<u64>,
        #[ts(type = "number")]
        row_count: u64,
        /// True when rows past `max_rows` were discarded.
        truncated: bool,
        #[ts(type = "number")]
        elapsed_ms: u64,
    },
    StatementError {
        index: u32,
        error: AppError,
        #[ts(type = "number")]
        elapsed_ms: u64,
    },
    Done {
        #[ts(type = "number")]
        elapsed_ms: u64,
        cancelled: bool,
    },
}

/// Summary returned once the stream completes (in addition to the `Done` event).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExecuteSummary {
    pub statements: u32,
    pub errors: u32,
    pub cancelled: bool,
    #[ts(type = "number")]
    pub elapsed_ms: u64,
}

// ---- cell encoding helpers -------------------------------------------------

pub fn cell_int(v: i64) -> Value {
    if (-MAX_SAFE_INT..=MAX_SAFE_INT).contains(&v) {
        Value::from(v)
    } else {
        Value::String(v.to_string())
    }
}

pub fn cell_float(v: f64) -> Value {
    serde_json::Number::from_f64(v)
        .map(Value::Number)
        .unwrap_or_else(|| Value::String(v.to_string()))
}

pub fn cell_text(s: &str) -> Value {
    if s.len() <= MAX_INLINE_TEXT {
        return Value::String(s.to_owned());
    }
    let mut cut = MAX_INLINE_TEXT;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    json!({ "$trunc": &s[..cut], "len": s.len() })
}

pub fn cell_bytes(b: &[u8]) -> Value {
    let preview: String = b
        .iter()
        .take(BYTES_PREVIEW)
        .map(|x| format!("{x:02x}"))
        .collect();
    json!({ "$bytes": preview, "len": b.len() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_ints_become_strings() {
        assert_eq!(cell_int(42), json!(42));
        assert_eq!(cell_int(MAX_SAFE_INT), json!(MAX_SAFE_INT));
        assert_eq!(cell_int(MAX_SAFE_INT + 1), json!("9007199254740992"));
        assert_eq!(cell_int(i64::MIN), json!(i64::MIN.to_string()));
    }

    #[test]
    fn non_finite_floats_become_strings() {
        assert_eq!(cell_float(1.5), json!(1.5));
        assert_eq!(cell_float(f64::NAN), json!("NaN"));
    }

    #[test]
    fn long_text_is_truncated_on_char_boundary() {
        let s = "é".repeat(MAX_INLINE_TEXT); // 2 bytes each
        let v = cell_text(&s);
        let preview = v["$trunc"].as_str().unwrap();
        assert!(preview.len() <= MAX_INLINE_TEXT);
        assert_eq!(v["len"], json!(s.len()));
    }

    #[test]
    fn bytes_preview() {
        let v = cell_bytes(&[0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(v, json!({"$bytes": "deadbeef", "len": 4}));
    }
}
