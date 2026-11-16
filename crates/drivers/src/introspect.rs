use snaptable_core::AppResult;
use snaptable_core::model::{SchemaObject, SchemaObjectKind, TableColumn};
use sqlx::Row;

use crate::error::map_sqlx;
use crate::pool::DbPool;

const PG_OBJECTS: &str = "
    SELECT n.nspname AS schema, c.relname AS name, c.relkind::text AS kind
    FROM pg_catalog.pg_class c
    JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
    WHERE c.relkind IN ('r', 'p', 'v', 'm', 'f')
      AND n.nspname NOT IN ('pg_catalog', 'information_schema')
      AND n.nspname NOT LIKE 'pg\\_toast%'
      AND n.nspname NOT LIKE 'pg\\_temp\\_%'
      AND NOT c.relispartition
    ORDER BY 1, 2";

const PG_COLUMNS: &str = "
    SELECT a.attname AS name,
           pg_catalog.format_type(a.atttypid, a.atttypmod) AS data_type,
           NOT a.attnotnull AS nullable,
           pg_catalog.pg_get_expr(d.adbin, d.adrelid) AS default_value,
           COALESCE(a.attnum = ANY (i.indkey), false) AS is_pk
    FROM pg_catalog.pg_attribute a
    JOIN pg_catalog.pg_class c ON c.oid = a.attrelid
    JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
    LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
    LEFT JOIN pg_catalog.pg_index i ON i.indrelid = c.oid AND i.indisprimary
    WHERE n.nspname = $1 AND c.relname = $2 AND a.attnum > 0 AND NOT a.attisdropped
    ORDER BY a.attnum";

const SQLITE_OBJECTS: &str = "
    SELECT 'main' AS schema, name, type AS kind FROM sqlite_master
    WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'
    ORDER BY name";

const SQLITE_COLUMNS: &str = "SELECT name, type AS data_type, \"notnull\", dflt_value, pk FROM pragma_table_info(?1, ?2) ORDER BY cid";

pub async fn list_objects(pool: &DbPool) -> AppResult<Vec<SchemaObject>> {
    let objects = match pool {
        DbPool::Postgres { pool, .. } => sqlx::query(PG_OBJECTS)
            .fetch_all(pool)
            .await
            .map_err(map_sqlx)?
            .iter()
            .map(|r| SchemaObject {
                schema: r.get("schema"),
                name: r.get("name"),
                kind: match r.get::<String, _>("kind").as_str() {
                    "v" => SchemaObjectKind::View,
                    "m" => SchemaObjectKind::MaterializedView,
                    _ => SchemaObjectKind::Table,
                },
            })
            .collect(),
        DbPool::Sqlite { pool, .. } => sqlx::query(SQLITE_OBJECTS)
            .fetch_all(pool)
            .await
            .map_err(map_sqlx)?
            .iter()
            .map(|r| SchemaObject {
                schema: r.get("schema"),
                name: r.get("name"),
                kind: if r.get::<String, _>("kind") == "view" {
                    SchemaObjectKind::View
                } else {
                    SchemaObjectKind::Table
                },
            })
            .collect(),
    };
    Ok(objects)
}

pub async fn table_columns(
    pool: &DbPool,
    schema: &str,
    table: &str,
) -> AppResult<Vec<TableColumn>> {
    let columns = match pool {
        DbPool::Postgres { pool, .. } => sqlx::query(PG_COLUMNS)
            .bind(schema)
            .bind(table)
            .fetch_all(pool)
            .await
            .map_err(map_sqlx)?
            .iter()
            .map(|r| TableColumn {
                name: r.get("name"),
                data_type: r.get("data_type"),
                nullable: r.get("nullable"),
                default_value: r.get("default_value"),
                is_primary_key: r.get("is_pk"),
            })
            .collect(),
        DbPool::Sqlite { pool, .. } => sqlx::query(SQLITE_COLUMNS)
            .bind(table)
            .bind(schema)
            .fetch_all(pool)
            .await
            .map_err(map_sqlx)?
            .iter()
            .map(|r| TableColumn {
                name: r.get("name"),
                data_type: r.get("data_type"),
                nullable: r.get::<i64, _>("notnull") == 0,
                default_value: r.get("dflt_value"),
                is_primary_key: r.get::<i64, _>("pk") > 0,
            })
            .collect(),
    };
    Ok(columns)
}

/// Primary-key column names in key order (empty when the table has no PK).
pub async fn primary_key(pool: &DbPool, schema: &str, table: &str) -> AppResult<Vec<String>> {
    Ok(table_columns(pool, schema, table)
        .await?
        .into_iter()
        .filter(|c| c.is_primary_key)
        .map(|c| c.name)
        .collect())
}

/// Exact row count for a table (+ optional raw filter). Used by the browser footer.
pub async fn count_rows(
    pool: &DbPool,
    schema: &str,
    table: &str,
    filter: Option<&str>,
) -> AppResult<i64> {
    match pool {
        DbPool::Postgres { pool, .. } => {
            let sql = snaptable_sqlgen::count_sql(
                snaptable_sqlgen::Dialect::Postgres,
                schema,
                table,
                filter,
            );
            sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
                .fetch_one(pool)
                .await
                .map_err(map_sqlx)
        }
        DbPool::Sqlite { pool, .. } => {
            let sql = snaptable_sqlgen::count_sql(
                snaptable_sqlgen::Dialect::Sqlite,
                schema,
                table,
                filter,
            );
            sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
                .fetch_one(pool)
                .await
                .map_err(map_sqlx)
        }
    }
}
