//! The app-owned metadata database (SPEC §2.2): connections, environments, history,
//! workspace state. SQLite in WAL mode with embedded, forward-only migrations.

use std::path::Path;
use std::str::FromStr;

use snaptable_core::model::*;
use snaptable_core::time::now_ms;
use snaptable_core::{AppError, AppResult};
use snaptable_secrets::{SecretKey, SecretStore};
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteRow,
};
use sqlx::{AssertSqlSafe, Row};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Per-connection cap on stored history rows; older rows are pruned at startup.
pub const HISTORY_CAP_PER_CONNECTION: i64 = 10_000;

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

fn db_err(e: sqlx::Error) -> AppError {
    AppError::store(e.to_string())
}

impl Store {
    /// Open (creating if needed) the metadata DB at `path`, back it up if migrations are
    /// pending, and migrate.
    pub async fn open(path: &Path) -> AppResult<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| AppError::store(e.to_string()))?;
        }
        let existed = path.exists();
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await
            .map_err(db_err)?;
        restrict_permissions(path);
        let store = Self { pool };
        if existed {
            store.backup_if_migrations_pending(path).await?;
        }
        MIGRATOR
            .run(&store.pool)
            .await
            .map_err(|e| AppError::store(e.to_string()))?;
        store.prune_history().await?;
        Ok(store)
    }

    /// In-memory database for tests.
    pub async fn open_in_memory() -> AppResult<Self> {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")
            .map_err(db_err)?
            .foreign_keys(true);
        // A single connection: every pooled connection to `:memory:` is a separate DB.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .map_err(db_err)?;
        MIGRATOR
            .run(&pool)
            .await
            .map_err(|e| AppError::store(e.to_string()))?;
        Ok(Self { pool })
    }

    async fn backup_if_migrations_pending(&self, path: &Path) -> AppResult<()> {
        let applied: i64 =
            sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE success = 1")
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0);
        let known = MIGRATOR.iter().count() as i64;
        if applied < known {
            let backup = path.with_extension(format!("db.bak-{applied}"));
            let sql = format!(
                "VACUUM INTO '{}'",
                backup.display().to_string().replace('\'', "''")
            );
            sqlx::query(AssertSqlSafe(sql))
                .execute(&self.pool)
                .await
                .map_err(db_err)?;
        }
        Ok(())
    }

    // ---- environments -----------------------------------------------------

    pub async fn environments(&self) -> AppResult<Vec<Environment>> {
        let rows = sqlx::query("SELECT * FROM environments ORDER BY sort_order, name")
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(rows
            .iter()
            .map(|r| Environment {
                id: r.get("id"),
                name: r.get("name"),
                color: r.get("color"),
                protection_level: ProtectionLevel::parse(r.get("protection_level")),
                sort_order: r.get("sort_order"),
                is_builtin: r.get("is_builtin"),
            })
            .collect())
    }

    pub async fn environment(&self, id: &str) -> AppResult<Option<Environment>> {
        Ok(self.environments().await?.into_iter().find(|e| e.id == id))
    }

    // ---- connections -------------------------------------------------------

    pub async fn connections(&self) -> AppResult<Vec<Connection>> {
        let rows =
            sqlx::query("SELECT * FROM connections ORDER BY sort_order, name COLLATE NOCASE")
                .fetch_all(&self.pool)
                .await
                .map_err(db_err)?;
        rows.iter().map(row_to_connection).collect()
    }

    pub async fn connection(&self, id: &str) -> AppResult<Connection> {
        let row = sqlx::query("SELECT * FROM connections WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?
            .ok_or_else(|| AppError::not_found("Connection not found"))?;
        row_to_connection(&row)
    }

    /// Create or update a connection, routing the password to `secrets`. The password
    /// never touches this database.
    pub async fn save_connection(
        &self,
        secrets: &dyn SecretStore,
        input: ConnectionInput,
    ) -> AppResult<Connection> {
        input.validate()?;
        let now = now_ms();
        let (id, existing) = match &input.id {
            Some(id) => (id.clone(), Some(self.connection(id).await?)),
            None => (uuid::Uuid::now_v7().to_string(), None),
        };
        let pw_key = SecretKey::DbPassword {
            connection_id: id.clone(),
        };
        let has_password = match &input.password {
            SecretChange::Keep => existing.as_ref().is_some_and(|c| c.has_password),
            SecretChange::Set { value } => {
                secrets.set(&pw_key, value)?;
                true
            }
            SecretChange::Clear => {
                secrets.delete(&pw_key)?;
                false
            }
        };
        let options_json =
            serde_json::to_string(&input.options).map_err(|e| AppError::internal(e.to_string()))?;
        sqlx::query(
            "INSERT INTO connections (id, group_id, name, engine, environment_id, host, port, database, username,
                has_password, sqlite_path, tls_mode, options_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (id) DO UPDATE SET
                group_id = excluded.group_id, name = excluded.name, engine = excluded.engine,
                environment_id = excluded.environment_id, host = excluded.host, port = excluded.port,
                database = excluded.database, username = excluded.username,
                has_password = excluded.has_password, sqlite_path = excluded.sqlite_path,
                tls_mode = excluded.tls_mode, options_json = excluded.options_json,
                updated_at = excluded.updated_at",
        )
        .bind(&id)
        .bind(&input.group_id)
        .bind(input.name.trim())
        .bind(input.engine.as_str())
        .bind(&input.environment_id)
        .bind(trimmed(&input.host))
        .bind(input.port.map(i64::from))
        .bind(trimmed(&input.database))
        .bind(trimmed(&input.username))
        .bind(has_password)
        .bind(trimmed(&input.sqlite_path))
        .bind(input.tls_mode.as_str())
        .bind(options_json)
        .bind(existing.as_ref().map_or(now, |c| c.created_at))
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        self.connection(&id).await
    }

    /// Delete a connection and every Keychain item that belongs to it.
    pub async fn delete_connection(&self, secrets: &dyn SecretStore, id: &str) -> AppResult<()> {
        for key in SecretKey::all_for_connection(id) {
            secrets.delete(&key)?;
        }
        sqlx::query("DELETE FROM connections WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn touch_connection(&self, id: &str) -> AppResult<()> {
        sqlx::query("UPDATE connections SET last_connected_at = ? WHERE id = ?")
            .bind(now_ms())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    // ---- history -----------------------------------------------------------

    pub async fn add_history(
        &self,
        connection_id: &str,
        sql: &str,
        status: HistoryStatus,
        rows_affected: Option<i64>,
        elapsed_ms: i64,
    ) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO query_history (connection_id, sql, status, rows_affected, elapsed_ms, executed_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(connection_id)
        .bind(sql)
        .bind(status.as_str())
        .bind(rows_affected)
        .bind(elapsed_ms)
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    /// Most recent first. Substring search uses the FTS5 trigram index for queries of 3+
    /// characters and falls back to `LIKE` for shorter ones.
    pub async fn search_history(
        &self,
        connection_id: Option<&str>,
        query: &str,
        limit: u32,
    ) -> AppResult<Vec<HistoryEntry>> {
        let q = query.trim();
        let rows = if q.chars().count() >= 3 {
            let fts = format!("\"{}\"", q.replace('"', "\"\""));
            sqlx::query(
                "SELECT h.* FROM query_history h JOIN query_history_fts f ON f.rowid = h.id
                 WHERE query_history_fts MATCH ? AND (? IS NULL OR h.connection_id = ?)
                 ORDER BY h.executed_at DESC, h.id DESC LIMIT ?",
            )
            .bind(fts)
            .bind(connection_id)
            .bind(connection_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
        } else {
            let like = format!(
                "%{}%",
                q.replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            );
            sqlx::query(
                "SELECT * FROM query_history
                 WHERE sql LIKE ? ESCAPE '\\' AND (? IS NULL OR connection_id = ?)
                 ORDER BY executed_at DESC, id DESC LIMIT ?",
            )
            .bind(like)
            .bind(connection_id)
            .bind(connection_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
        }
        .map_err(db_err)?;
        Ok(rows
            .iter()
            .map(|r| HistoryEntry {
                id: r.get("id"),
                connection_id: r.get("connection_id"),
                sql: r.get("sql"),
                status: HistoryStatus::parse(r.get("status")),
                rows_affected: r.get("rows_affected"),
                elapsed_ms: r.get("elapsed_ms"),
                executed_at: r.get("executed_at"),
            })
            .collect())
    }

    async fn prune_history(&self) -> AppResult<()> {
        sqlx::query(
            "DELETE FROM query_history WHERE id IN (
                SELECT id FROM (
                    SELECT id, row_number() OVER (PARTITION BY connection_id ORDER BY executed_at DESC, id DESC) AS rn
                    FROM query_history
                ) WHERE rn > ?
            )",
        )
        .bind(HISTORY_CAP_PER_CONNECTION)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    // ---- workspace / settings ----------------------------------------------

    pub async fn workspace_get(&self, key: &str) -> AppResult<Option<serde_json::Value>> {
        let row: Option<String> =
            sqlx::query_scalar("SELECT json FROM workspace_state WHERE key = ?")
                .bind(key)
                .fetch_optional(&self.pool)
                .await
                .map_err(db_err)?;
        Ok(row.and_then(|s| serde_json::from_str(&s).ok()))
    }

    pub async fn workspace_set(&self, key: &str, value: &serde_json::Value) -> AppResult<()> {
        sqlx::query("INSERT INTO workspace_state (key, json) VALUES (?, ?) ON CONFLICT (key) DO UPDATE SET json = excluded.json")
            .bind(key)
            .bind(value.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }
}

fn trimmed(v: &Option<String>) -> Option<String> {
    v.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn row_to_connection(r: &SqliteRow) -> AppResult<Connection> {
    let engine: String = r.get("engine");
    let options: String = r.get("options_json");
    Ok(Connection {
        id: r.get("id"),
        group_id: r.get("group_id"),
        name: r.get("name"),
        engine: Engine::parse(&engine)
            .ok_or_else(|| AppError::store(format!("Unknown engine '{engine}'")))?,
        environment_id: r.get("environment_id"),
        host: r.get("host"),
        port: r
            .get::<Option<i64>, _>("port")
            .and_then(|p| u16::try_from(p).ok()),
        database: r.get("database"),
        username: r.get("username"),
        has_password: r.get("has_password"),
        sqlite_path: r.get("sqlite_path"),
        tls_mode: TlsMode::parse(r.get("tls_mode")),
        options: serde_json::from_str(&options).unwrap_or_default(),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        last_connected_at: r.get("last_connected_at"),
    })
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    for p in [
        path.to_path_buf(),
        path.with_extension("db-wal"),
        path.with_extension("db-shm"),
    ] {
        if p.exists() {
            let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
        }
    }
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}

#[cfg(test)]
mod tests;
