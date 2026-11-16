//! Persisted entities (connections, environments, history) and their IPC shapes.
//!
//! Hard rule: none of these types ever carries a secret value outward. Secrets flow
//! *in* through [`SecretChange`] and are written straight to the Keychain.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Engine {
    Postgres,
    Sqlite,
}

impl Engine {
    pub fn as_str(self) -> &'static str {
        match self {
            Engine::Postgres => "postgres",
            Engine::Sqlite => "sqlite",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "postgres" => Some(Engine::Postgres),
            "sqlite" => Some(Engine::Sqlite),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ProtectionLevel {
    None,
    ConfirmWrites,
    ReadOnly,
}

impl ProtectionLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            ProtectionLevel::None => "none",
            ProtectionLevel::ConfirmWrites => "confirm_writes",
            ProtectionLevel::ReadOnly => "read_only",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "confirm_writes" => ProtectionLevel::ConfirmWrites,
            "read_only" => ProtectionLevel::ReadOnly,
            _ => ProtectionLevel::None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Environment {
    pub id: String,
    pub name: String,
    /// CSS color (hex) used for banners, tab underlines and sidebar dots.
    pub color: String,
    pub protection_level: ProtectionLevel,
    #[ts(type = "number")]
    pub sort_order: i64,
    pub is_builtin: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TlsMode {
    Disable,
    #[default]
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

impl TlsMode {
    pub fn as_str(self) -> &'static str {
        match self {
            TlsMode::Disable => "disable",
            TlsMode::Prefer => "prefer",
            TlsMode::Require => "require",
            TlsMode::VerifyCa => "verify_ca",
            TlsMode::VerifyFull => "verify_full",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "disable" => TlsMode::Disable,
            "require" => TlsMode::Require,
            "verify_ca" => TlsMode::VerifyCa,
            "verify_full" => TlsMode::VerifyFull,
            _ => TlsMode::Prefer,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ConnectionOptions {
    /// Server-side statement timeout applied to every session (PostgreSQL).
    pub statement_timeout_ms: Option<u32>,
    /// `search_path` applied on connect (PostgreSQL).
    pub search_path: Option<String>,
}

/// A saved connection as stored in the metadata DB. Never contains secrets.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Connection {
    pub id: String,
    pub group_id: Option<String>,
    pub name: String,
    pub engine: Engine,
    pub environment_id: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub username: Option<String>,
    /// True when a password is stored in the Keychain for this connection.
    pub has_password: bool,
    pub sqlite_path: Option<String>,
    pub tls_mode: TlsMode,
    pub options: ConnectionOptions,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
    #[ts(type = "number | null")]
    pub last_connected_at: Option<i64>,
}

/// How a write-only secret field should change on save.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(tag = "action", rename_all = "camelCase")]
#[ts(export)]
pub enum SecretChange {
    /// Leave whatever is stored untouched.
    #[default]
    Keep,
    Set {
        value: String,
    },
    Clear,
}

/// Payload for creating/updating a connection (and for "Test connection").
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConnectionInput {
    /// `None` creates a new connection.
    pub id: Option<String>,
    pub group_id: Option<String>,
    pub name: String,
    pub engine: Engine,
    pub environment_id: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub username: Option<String>,
    pub sqlite_path: Option<String>,
    #[serde(default)]
    pub tls_mode: TlsMode,
    #[serde(default)]
    pub options: ConnectionOptions,
    #[serde(default)]
    pub password: SecretChange,
}

impl ConnectionInput {
    pub fn validate(&self) -> Result<(), crate::AppError> {
        if self.name.trim().is_empty() {
            return Err(crate::AppError::invalid("Connection name is required"));
        }
        match self.engine {
            Engine::Postgres => {
                if self.host.as_deref().unwrap_or("").trim().is_empty() {
                    return Err(crate::AppError::invalid("Host is required"));
                }
            }
            Engine::Sqlite => {
                if self.sqlite_path.as_deref().unwrap_or("").trim().is_empty() {
                    return Err(crate::AppError::invalid("Database file path is required"));
                }
            }
        }
        Ok(())
    }
}

/// A table-like object shown in the sidebar.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SchemaObject {
    pub schema: String,
    pub name: String,
    pub kind: SchemaObjectKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SchemaObjectKind {
    Table,
    View,
    MaterializedView,
}

/// Column metadata from schema introspection (used for autocomplete and browsing).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TableColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub is_primary_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryEntry {
    #[ts(type = "number")]
    pub id: i64,
    pub connection_id: Option<String>,
    pub sql: String,
    pub status: HistoryStatus,
    #[ts(type = "number | null")]
    pub rows_affected: Option<i64>,
    #[ts(type = "number")]
    pub elapsed_ms: i64,
    #[ts(type = "number")]
    pub executed_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum HistoryStatus {
    Success,
    Error,
    Cancelled,
}

impl HistoryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            HistoryStatus::Success => "success",
            HistoryStatus::Error => "error",
            HistoryStatus::Cancelled => "cancelled",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "error" => HistoryStatus::Error,
            "cancelled" => HistoryStatus::Cancelled,
            _ => HistoryStatus::Success,
        }
    }
}
