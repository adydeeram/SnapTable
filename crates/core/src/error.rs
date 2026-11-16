use serde::Serialize;
use ts_rs::TS;

/// The single error type returned by every IPC command.
///
/// Serialized as `{ "kind": "...", "message": "...", ...extra }`; the UI switches on
/// `kind` and never parses `message`.
#[derive(Debug, Clone, thiserror::Error, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum AppError {
    #[error("{message}")]
    Connection { message: String },
    #[error("{message}")]
    Auth {
        message: String,
        keychain_denied: bool,
    },
    #[error("{message}")]
    Query {
        message: String,
        code: Option<String>,
        /// 1-based character position inside the statement, when the server reports one.
        position: Option<u32>,
        detail: Option<String>,
        hint: Option<String>,
    },
    #[error("Cancelled")]
    Cancelled,
    #[error("{message}")]
    NotFound { message: String },
    #[error("{message}")]
    Invalid { message: String },
    #[error("{message}")]
    Protected { message: String },
    #[error("{message}")]
    Store { message: String },
    #[error("{message}")]
    Internal { message: String },
}

impl AppError {
    pub fn connection(message: impl Into<String>) -> Self {
        Self::Connection {
            message: message.into(),
        }
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound {
            message: message.into(),
        }
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid {
            message: message.into(),
        }
    }
    pub fn store(message: impl Into<String>) -> Self {
        Self::Store {
            message: message.into(),
        }
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
