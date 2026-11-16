use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use snaptable_core::model::{Connection, Engine, TlsMode};
use snaptable_core::{AppError, AppResult};
use sqlx::Connection as _;
use sqlx::postgres::{PgConnectOptions, PgConnection, PgPool, PgPoolOptions, PgSslMode};
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection, SqlitePool, SqlitePoolOptions};

use crate::error::map_sqlx;

const BROWSE_POOL_SIZE: u32 = 3;
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(15);

/// Everything needed to open connections. Holds the password in memory only for the
/// lifetime of the live connection (sqlx needs it to open new pool members).
pub struct ConnectParams {
    pub connection: Connection,
    pub password: Option<SecretString>,
    /// Open sessions read-only at the database level (Production protection, SPEC §4.5).
    pub read_only: bool,
}

#[derive(Clone)]
pub enum DbPool {
    Postgres {
        pool: PgPool,
        opts: PgConnectOptions,
    },
    Sqlite {
        pool: SqlitePool,
        opts: SqliteConnectOptions,
    },
}

/// A dedicated (non-pooled) connection, pinned to one editor tab.
pub enum DbConn {
    Postgres(PgConnection),
    Sqlite(SqliteConnection),
}

impl DbPool {
    pub async fn connect(params: &ConnectParams) -> AppResult<Self> {
        match params.connection.engine {
            Engine::Postgres => {
                let opts = pg_options(params)?;
                let pool = PgPoolOptions::new()
                    .max_connections(BROWSE_POOL_SIZE)
                    .min_connections(0)
                    .acquire_timeout(ACQUIRE_TIMEOUT)
                    .idle_timeout(Duration::from_secs(300))
                    .test_before_acquire(true)
                    .connect_with(opts.clone())
                    .await
                    .map_err(map_connect_err)?;
                Ok(Self::Postgres { pool, opts })
            }
            Engine::Sqlite => {
                let opts = sqlite_options(params)?;
                let pool = SqlitePoolOptions::new()
                    .max_connections(BROWSE_POOL_SIZE)
                    .acquire_timeout(ACQUIRE_TIMEOUT)
                    .connect_with(opts.clone())
                    .await
                    .map_err(map_connect_err)?;
                Ok(Self::Sqlite { pool, opts })
            }
        }
    }

    /// Connect once and run a trivial query; used by "Test connection".
    pub async fn test(params: &ConnectParams) -> AppResult<String> {
        match params.connection.engine {
            Engine::Postgres => {
                let mut c = PgConnection::connect_with(&pg_options(params)?)
                    .await
                    .map_err(map_connect_err)?;
                let v: String = sqlx::query_scalar("SELECT version()")
                    .fetch_one(&mut c)
                    .await
                    .map_err(map_sqlx)?;
                let _ = c.close().await;
                Ok(v)
            }
            Engine::Sqlite => {
                let mut c = SqliteConnection::connect_with(&sqlite_options(params)?)
                    .await
                    .map_err(map_connect_err)?;
                let v: String = sqlx::query_scalar("SELECT sqlite_version()")
                    .fetch_one(&mut c)
                    .await
                    .map_err(map_sqlx)?;
                let _ = c.close().await;
                Ok(format!("SQLite {v}"))
            }
        }
    }

    pub async fn open_dedicated(&self) -> AppResult<DbConn> {
        match self {
            DbPool::Postgres { opts, .. } => Ok(DbConn::Postgres(
                PgConnection::connect_with(opts)
                    .await
                    .map_err(map_connect_err)?,
            )),
            DbPool::Sqlite { opts, .. } => Ok(DbConn::Sqlite(
                SqliteConnection::connect_with(opts)
                    .await
                    .map_err(map_connect_err)?,
            )),
        }
    }

    pub async fn close(&self) {
        match self {
            DbPool::Postgres { pool, .. } => pool.close().await,
            DbPool::Sqlite { pool, .. } => pool.close().await,
        }
    }
}

fn pg_options(params: &ConnectParams) -> AppResult<PgConnectOptions> {
    let c = &params.connection;
    let host = c
        .host
        .as_deref()
        .filter(|h| !h.is_empty())
        .ok_or_else(|| AppError::invalid("Host is required"))?;
    let mut opts = PgConnectOptions::new_without_pgpass()
        .host(host)
        .port(c.port.unwrap_or(5432))
        .application_name("SnapTable")
        .ssl_mode(match c.tls_mode {
            TlsMode::Disable => PgSslMode::Disable,
            TlsMode::Prefer => PgSslMode::Prefer,
            TlsMode::Require => PgSslMode::Require,
            TlsMode::VerifyCa => PgSslMode::VerifyCa,
            TlsMode::VerifyFull => PgSslMode::VerifyFull,
        });
    if let Some(u) = c.username.as_deref().filter(|s| !s.is_empty()) {
        opts = opts.username(u);
    }
    if let Some(db) = c.database.as_deref().filter(|s| !s.is_empty()) {
        opts = opts.database(db);
    }
    if let Some(pw) = &params.password {
        opts = opts.password(pw.expose_secret());
    }
    let mut server_opts: Vec<(&str, String)> = Vec::new();
    if let Some(ms) = c.options.statement_timeout_ms {
        server_opts.push(("statement_timeout", ms.to_string()));
    }
    if let Some(sp) = c
        .options
        .search_path
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        server_opts.push(("search_path", sp.trim().to_owned()));
    }
    if params.read_only {
        server_opts.push(("default_transaction_read_only", "on".into()));
    }
    if !server_opts.is_empty() {
        opts = opts.options(server_opts);
    }
    Ok(opts)
}

fn sqlite_options(params: &ConnectParams) -> AppResult<SqliteConnectOptions> {
    let path = params
        .connection
        .sqlite_path
        .as_deref()
        .filter(|p| !p.is_empty())
        .ok_or_else(|| AppError::invalid("Database file path is required"))?;
    if !std::path::Path::new(path).exists() {
        return Err(AppError::connection(format!(
            "Database file not found: {path}"
        )));
    }
    Ok(SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .read_only(params.read_only)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5)))
}

fn map_connect_err(e: sqlx::Error) -> AppError {
    match &e {
        sqlx::Error::Database(db) if matches!(db.code().as_deref(), Some("28P01" | "28000")) => {
            AppError::Auth {
                message: db.message().to_owned(),
                keychain_denied: false,
            }
        }
        sqlx::Error::Database(_) => map_sqlx(e),
        _ => match map_sqlx(e) {
            AppError::Query { message, .. } => AppError::connection(message),
            other => other,
        },
    }
}
