use snaptable_core::AppError;
use sqlx::postgres::{PgDatabaseError, PgErrorPosition};

/// SQLSTATE for `query_canceled`.
const PG_QUERY_CANCELED: &str = "57014";

/// Map a sqlx error to an [`AppError`] the UI can render.
pub fn map_sqlx(e: sqlx::Error) -> AppError {
    match e {
        sqlx::Error::Database(db) => {
            if db.code().as_deref() == Some(PG_QUERY_CANCELED) {
                return AppError::Cancelled;
            }
            let (detail, hint, position) = match db.try_downcast_ref::<PgDatabaseError>() {
                Some(pg) => (
                    pg.detail().map(str::to_owned),
                    pg.hint().map(str::to_owned),
                    match pg.position() {
                        Some(PgErrorPosition::Original(p)) => u32::try_from(p).ok(),
                        _ => None,
                    },
                ),
                None => (None, None, None),
            };
            AppError::Query {
                message: db.message().to_owned(),
                code: db.code().map(|c| c.into_owned()),
                position,
                detail,
                hint,
            }
        }
        sqlx::Error::PoolTimedOut => {
            AppError::connection("Timed out waiting for a database connection")
        }
        sqlx::Error::Io(io) => AppError::connection(format!("Network error: {io}")),
        sqlx::Error::Tls(tls) => AppError::connection(format!("TLS error: {tls}")),
        sqlx::Error::Configuration(c) => AppError::invalid(c.to_string()),
        sqlx::Error::PoolClosed => AppError::connection("Connection was closed"),
        other => AppError::Query {
            message: other.to_string(),
            code: None,
            position: None,
            detail: None,
            hint: None,
        },
    }
}
