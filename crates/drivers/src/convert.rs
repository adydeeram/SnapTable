//! Row → cell conversion for each engine.
//!
//! PostgreSQL statements run through the simple-query protocol, so every value arrives
//! in *text* format. That gives a uniform, lossless fallback for any type (PostGIS,
//! ranges, enums, domains…) without per-type decoders; only a few types are mapped to
//! native JSON (numbers, booleans) or special markers (bytea).

use serde_json::Value;
use snaptable_core::query::{ColumnKind, ColumnMeta, cell_bytes, cell_float, cell_int, cell_text};
use sqlx::postgres::{PgRow, PgValueFormat};
use sqlx::sqlite::SqliteRow;
use sqlx::{Column, Row, TypeInfo, ValueRef};

pub trait ConvertRow {
    fn column_meta(&self) -> Vec<ColumnMeta>;
    fn push_cells(&self, out: &mut [Vec<Value>]);
}

pub fn pg_kind(type_name: &str) -> ColumnKind {
    match type_name {
        "INT2" | "INT4" | "INT8" | "FLOAT4" | "FLOAT8" | "NUMERIC" | "OID" | "MONEY" => {
            ColumnKind::Number
        }
        "BOOL" => ColumnKind::Bool,
        "JSON" | "JSONB" => ColumnKind::Json,
        "BYTEA" => ColumnKind::Bytes,
        "DATE" | "TIME" | "TIMETZ" | "TIMESTAMP" | "TIMESTAMPTZ" | "INTERVAL" => {
            ColumnKind::Temporal
        }
        "TEXT" | "VARCHAR" | "CHAR" | "BPCHAR" | "NAME" | "UUID" | "CITEXT" | "XML" => {
            ColumnKind::Text
        }
        _ => ColumnKind::Other,
    }
}

/// SQLite column affinity from a declared type (https://sqlite.org/datatype3.html §3.1).
pub fn sqlite_kind(declared: &str) -> ColumnKind {
    let t = declared.to_ascii_uppercase();
    if t.contains("BOOL") {
        ColumnKind::Bool
    } else if t.contains("INT")
        || t.contains("REAL")
        || t.contains("FLOA")
        || t.contains("DOUB")
        || t.contains("NUMERIC")
        || t.contains("DECIMAL")
    {
        ColumnKind::Number
    } else if t.contains("JSON") {
        ColumnKind::Json
    } else if t.contains("DATE") || t.contains("TIME") {
        ColumnKind::Temporal
    } else if t.contains("CHAR") || t.contains("CLOB") || t.contains("TEXT") {
        ColumnKind::Text
    } else if t.contains("BLOB") {
        ColumnKind::Bytes
    } else {
        ColumnKind::Other
    }
}

impl ConvertRow for PgRow {
    fn column_meta(&self) -> Vec<ColumnMeta> {
        self.columns()
            .iter()
            .map(|c| {
                let type_name = c.type_info().name().to_owned();
                ColumnMeta {
                    name: c.name().to_owned(),
                    kind: pg_kind(&type_name),
                    type_name,
                }
            })
            .collect()
    }

    fn push_cells(&self, out: &mut [Vec<Value>]) {
        for (i, col) in out.iter_mut().enumerate() {
            col.push(pg_cell(self, i));
        }
    }
}

fn pg_cell(row: &PgRow, i: usize) -> Value {
    let Ok(raw) = row.try_get_raw(i) else {
        return Value::Null;
    };
    if raw.is_null() {
        return Value::Null;
    }
    if raw.format() == PgValueFormat::Binary {
        // Not expected for simple-query results; render bytes rather than guessing.
        return raw.as_bytes().map(cell_bytes).unwrap_or(Value::Null);
    }
    let type_name = raw.type_info().name().to_owned();
    let Ok(text) = raw.as_str() else {
        return raw.as_bytes().map(cell_bytes).unwrap_or(Value::Null);
    };
    match type_name.as_str() {
        "INT2" | "INT4" | "INT8" | "OID" => text
            .parse::<i64>()
            .map(cell_int)
            .unwrap_or_else(|_| cell_text(text)),
        "FLOAT4" | "FLOAT8" => match text.parse::<f64>() {
            Ok(f) if f.is_finite() => cell_float(f),
            _ => cell_text(text),
        },
        "BOOL" => Value::Bool(text == "t"),
        "BYTEA" => match text.strip_prefix("\\x").map(decode_hex) {
            Some(Some(bytes)) => cell_bytes(&bytes),
            _ => cell_text(text),
        },
        _ => cell_text(text),
    }
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

impl ConvertRow for SqliteRow {
    fn column_meta(&self) -> Vec<ColumnMeta> {
        self.columns()
            .iter()
            .map(|c| {
                let type_name = c.type_info().name().to_owned();
                ColumnMeta {
                    name: c.name().to_owned(),
                    kind: sqlite_kind(&type_name),
                    type_name,
                }
            })
            .collect()
    }

    fn push_cells(&self, out: &mut [Vec<Value>]) {
        for (i, col) in out.iter_mut().enumerate() {
            col.push(sqlite_cell(self, i));
        }
    }
}

fn sqlite_cell(row: &SqliteRow, i: usize) -> Value {
    let Ok(raw) = row.try_get_raw(i) else {
        return Value::Null;
    };
    if raw.is_null() {
        return Value::Null;
    }
    // SQLite is dynamically typed: dispatch on the value's storage class, not the column.
    let storage = raw.type_info().name().to_owned();
    match storage.as_str() {
        "INTEGER" | "BOOLEAN" => row
            .try_get_unchecked::<i64, _>(i)
            .map(cell_int)
            .unwrap_or(Value::Null),
        "REAL" | "NUMERIC" => row
            .try_get_unchecked::<f64, _>(i)
            .map(cell_float)
            .unwrap_or(Value::Null),
        "BLOB" => row
            .try_get_unchecked::<Vec<u8>, _>(i)
            .map(|b| cell_bytes(&b))
            .unwrap_or(Value::Null),
        _ => match row.try_get_unchecked::<String, _>(i) {
            Ok(s) => cell_text(&s),
            Err(_) => row
                .try_get_unchecked::<Vec<u8>, _>(i)
                .map(|b| cell_bytes(&b))
                .unwrap_or(Value::Null),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_decoding() {
        assert_eq!(decode_hex("00ff10"), Some(vec![0, 255, 16]));
        assert_eq!(decode_hex("0"), None);
        assert_eq!(decode_hex("zz"), None);
    }

    #[test]
    fn sqlite_affinity() {
        assert_eq!(sqlite_kind("INTEGER"), ColumnKind::Number);
        assert_eq!(sqlite_kind("varchar(20)"), ColumnKind::Text);
        assert_eq!(sqlite_kind("BOOLEAN"), ColumnKind::Bool);
        assert_eq!(sqlite_kind("DATETIME"), ColumnKind::Temporal);
        assert_eq!(sqlite_kind(""), ColumnKind::Other);
    }
}
