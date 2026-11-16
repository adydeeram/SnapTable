//! PostgreSQL integration tests. Run against a live server:
//!
//! `SNAPTABLE_PG_HOST=127.0.0.1 SNAPTABLE_PG_PORT=5432 SNAPTABLE_PG_USER=postgres cargo test -p snaptable-drivers`
//!
//! Skipped (pass trivially) when `SNAPTABLE_PG_PORT` is unset. CI provides the server.

mod common;

use std::time::{Duration, Instant};

use common::*;
use secrecy::SecretString;
use serde_json::json;
use snaptable_core::AppError;
use snaptable_core::model::Engine;
use snaptable_core::query::ColumnKind;
use snaptable_drivers::exec::ExecOptions;
use snaptable_drivers::{ConnectParams, ConnectionManager, DbPool, introspect};

fn params(read_only: bool) -> Option<ConnectParams> {
    let port: u16 = std::env::var("SNAPTABLE_PG_PORT").ok()?.parse().ok()?;
    let mut c = connection(Engine::Postgres);
    c.host = Some(std::env::var("SNAPTABLE_PG_HOST").unwrap_or_else(|_| "127.0.0.1".into()));
    c.port = Some(port);
    c.username = Some(std::env::var("SNAPTABLE_PG_USER").unwrap_or_else(|_| "postgres".into()));
    c.database = Some(std::env::var("SNAPTABLE_PG_DB").unwrap_or_else(|_| "postgres".into()));
    let password = std::env::var("SNAPTABLE_PG_PASSWORD")
        .ok()
        .map(SecretString::from);
    Some(ConnectParams {
        connection: c,
        password,
        read_only,
    })
}

macro_rules! pg_or_skip {
    ($read_only:expr) => {
        match params($read_only) {
            Some(p) => p,
            None => {
                eprintln!("SNAPTABLE_PG_PORT not set; skipping");
                return;
            }
        }
    };
}

async fn setup(read_only: bool) -> Option<(ConnectionManager, String, String)> {
    let p = params(read_only)?;
    let id = p.connection.id.clone();
    let mgr = ConnectionManager::default();
    mgr.connect(p).await.unwrap();
    let session = mgr.open_session(&id).await.unwrap();
    Some((mgr, id, session))
}

#[tokio::test]
async fn test_connection_reports_version() {
    let p = pg_or_skip!(false);
    let v = DbPool::test(&p).await.unwrap();
    assert!(v.starts_with("PostgreSQL"), "{v}");
}

#[tokio::test]
async fn bad_credentials_and_hosts_map_to_typed_errors() {
    let mut p = pg_or_skip!(false);
    p.connection.port = Some(1); // nothing listens on port 1
    assert!(matches!(
        DbPool::test(&p).await,
        Err(AppError::Connection { .. })
    ));
}

#[tokio::test]
async fn decodes_common_types() {
    let Some((mgr, _, session)) = setup(false).await else {
        return;
    };
    let mut c = Collected::default();
    let r = mgr
        .run_in_session(
            "types",
            &session,
            &stmts(&[
                "CREATE TYPE pg_temp.mood AS ENUM ('ok', 'sad')",
                "SELECT 42::int4 AS i, 9007199254740993::int8 AS big, 1.5::float8 AS f, 'NaN'::float8 AS nan,
                        12.345::numeric AS n, true AS b, '\\xdeadbeef'::bytea AS bin, '{\"a\": 1}'::jsonb AS j,
                        NULL::text AS nul, ''::text AS empty, '2026-01-02T03:04:05Z'::timestamptz AS ts,
                        ARRAY[1,2] AS arr, 'ok'::pg_temp.mood AS e, gen_random_uuid() IS NOT NULL AS has_uuid",
            ]),
            &ExecOptions::default(),
            &mut c.sink(),
        )
        .await
        .unwrap();
    assert_ok(&r);
    let s = &c.statements[1];
    let kinds: Vec<_> = s
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c.kind))
        .collect();
    assert_eq!(kinds[0], ("i", ColumnKind::Number));
    assert_eq!(kinds[5], ("b", ColumnKind::Bool));
    assert_eq!(kinds[7], ("j", ColumnKind::Json));
    assert_eq!(kinds[10], ("ts", ColumnKind::Temporal));
    let row = &s.rows[0];
    assert_eq!(row[0], json!(42));
    assert_eq!(row[1], json!("9007199254740993"), "unsafe ints are strings");
    assert_eq!(row[2], json!(1.5));
    assert_eq!(row[3], json!("NaN"));
    assert_eq!(row[4], json!("12.345"), "numeric keeps exact text");
    assert_eq!(row[5], json!(true));
    assert_eq!(row[6], json!({"$bytes": "deadbeef", "len": 4}));
    assert_eq!(row[7], json!("{\"a\": 1}"));
    assert_eq!(row[8], json!(null));
    assert_eq!(row[9], json!(""), "empty string is distinct from NULL");
    assert!(row[10].as_str().unwrap().starts_with("2026-01-02"));
    assert_eq!(row[11], json!("{1,2}"));
    assert_eq!(row[12], json!("ok"), "custom enum falls back to text");
}

#[tokio::test]
async fn streams_100k_rows_in_batches() {
    let Some((mgr, _, session)) = setup(false).await else {
        return;
    };
    let mut c = Collected::default();
    let started = Instant::now();
    let r = mgr
        .run_in_session(
            "stream",
            &session,
            &stmts(&["SELECT g AS id, md5(g::text) AS hash, g % 7 = 0 AS flag FROM generate_series(1, 100000) g"]),
            &ExecOptions::default(),
            &mut c.sink(),
        )
        .await
        .unwrap();
    assert_ok(&r);
    let s = &c.statements[0];
    assert_eq!(s.rows.len(), 100_000);
    assert!(
        s.batches >= 100,
        "expected ~1000-row batches, got {}",
        s.batches
    );
    assert_eq!(s.rows[99_999][0], json!(100_000));
    assert!(!s.truncated);
    eprintln!(
        "100k rows streamed in {:?} ({} batches)",
        started.elapsed(),
        s.batches
    );
}

#[tokio::test]
async fn truncates_past_max_rows_but_drains() {
    let Some((mgr, _, session)) = setup(false).await else {
        return;
    };
    let opts = ExecOptions {
        max_rows: 1_000,
        ..Default::default()
    };
    let mut c = Collected::default();
    let r = mgr
        .run_in_session(
            "trunc",
            &session,
            &stmts(&["SELECT g FROM generate_series(1, 5000) g", "SELECT 'next'"]),
            &opts,
            &mut c.sink(),
        )
        .await
        .unwrap();
    assert_ok(&r);
    assert_eq!(c.statements[0].rows.len(), 1_000);
    assert_eq!(c.statements[0].row_count, 5_000);
    assert!(c.statements[0].truncated);
    assert_eq!(
        c.statements[1].rows[0][0],
        json!("next"),
        "connection stays usable"
    );
}

#[tokio::test]
async fn cancel_stops_the_server_query() {
    let Some((mgr, conn_id, session)) = setup(false).await else {
        return;
    };
    let mgr = std::sync::Arc::new(mgr);
    let canceller = {
        let mgr = mgr.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            assert!(mgr.tasks().cancel("sleepy"));
        })
    };
    let started = Instant::now();
    let mut c = Collected::default();
    let r = mgr
        .run_in_session(
            "sleepy",
            &session,
            &stmts(&["SELECT pg_sleep(30)", "SELECT 'never runs'"]),
            &ExecOptions::default(),
            &mut c.sink(),
        )
        .await
        .unwrap();
    canceller.await.unwrap();
    assert!(r.summary.cancelled);
    assert!(c.cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "took {:?}",
        started.elapsed()
    );
    assert_eq!(c.statements.len(), 1, "later statements are skipped");
    assert!(matches!(c.statements[0].error, Some(AppError::Cancelled)));
    assert!(r.connection_healthy);

    // The backend is no longer running pg_sleep, and the session still works.
    let live = mgr.get(&conn_id).unwrap();
    if let DbPool::Postgres { pool, .. } = &live.pool {
        let active: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE query = 'SELECT pg_sleep(30)' AND state = 'active'")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(active, 0);
    }
    let mut c = Collected::default();
    let r = mgr
        .run_in_session(
            "after",
            &session,
            &stmts(&["SELECT 1"]),
            &ExecOptions::default(),
            &mut c.sink(),
        )
        .await
        .unwrap();
    assert_ok(&r);
}

#[tokio::test]
async fn errors_carry_code_and_position_and_stop_execution() {
    let Some((mgr, _, session)) = setup(false).await else {
        return;
    };
    let mut c = Collected::default();
    let r = mgr
        .run_in_session(
            "err",
            &session,
            &stmts(&["SELECT 1", "SELECT * FROM does_not_exist", "SELECT 3"]),
            &ExecOptions::default(),
            &mut c.sink(),
        )
        .await
        .unwrap();
    assert_eq!(r.summary.errors, 1);
    assert_eq!(c.statements.len(), 2);
    match &c.statements[1].error {
        Some(AppError::Query { code, position, .. }) => {
            assert_eq!(code.as_deref(), Some("42P01"));
            assert_eq!(*position, Some(15));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn empty_results_still_have_columns_and_dml_reports_affected() {
    let Some((mgr, _, session)) = setup(false).await else {
        return;
    };
    let mut c = Collected::default();
    let r = mgr
        .run_in_session(
            "empty",
            &session,
            &stmts(&[
                "CREATE TEMP TABLE t (id int PRIMARY KEY, name text)",
                "INSERT INTO t VALUES (1, 'a'), (2, 'b')",
                "SELECT id, name FROM t WHERE false",
            ]),
            &ExecOptions::default(),
            &mut c.sink(),
        )
        .await
        .unwrap();
    assert_ok(&r);
    assert_eq!(c.statements[1].rows_affected, Some(2));
    let names: Vec<_> = c.statements[2]
        .columns
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["id", "name"]);
    assert!(c.statements[2].rows.is_empty());
    assert_eq!(c.statements[2].rows_affected, None);
}

#[tokio::test]
async fn session_state_persists_between_runs() {
    let Some((mgr, _, session)) = setup(false).await else {
        return;
    };
    let opts = ExecOptions::default();
    let mut c = Collected::default();
    mgr.run_in_session(
        "a",
        &session,
        &stmts(&[
            "BEGIN",
            "CREATE TEMP TABLE keep (x int)",
            "INSERT INTO keep VALUES (7)",
        ]),
        &opts,
        &mut c.sink(),
    )
    .await
    .unwrap();
    let mut c = Collected::default();
    let r = mgr
        .run_in_session(
            "b",
            &session,
            &stmts(&["SELECT x FROM keep", "ROLLBACK"]),
            &opts,
            &mut c.sink(),
        )
        .await
        .unwrap();
    assert_ok(&r);
    assert_eq!(c.statements[0].rows[0][0], json!(7));
}

#[tokio::test]
async fn read_only_connections_reject_writes() {
    let p = pg_or_skip!(true);
    let id = p.connection.id.clone();
    let mgr = ConnectionManager::default();
    mgr.connect(p).await.unwrap();
    let session = mgr.open_session(&id).await.unwrap();
    let mut c = Collected::default();
    let r = mgr
        .run_in_session(
            "ro",
            &session,
            &stmts(&["CREATE TABLE should_fail (x int)"]),
            &ExecOptions::default(),
            &mut c.sink(),
        )
        .await
        .unwrap();
    assert_eq!(r.summary.errors, 1);
    match &c.statements[0].error {
        Some(AppError::Query { code, .. }) => assert_eq!(code.as_deref(), Some("25006")),
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn introspection_and_browse() {
    let Some((mgr, id, session)) = setup(false).await else {
        return;
    };
    let table = format!("it_{}", uuid::Uuid::new_v4().simple());
    let opts = ExecOptions::default();
    let mut c = Collected::default();
    mgr.run_in_session(
        "mk",
        &session,
        &stmts(&[
            &format!("CREATE TABLE public.{table} (id serial PRIMARY KEY, name text NOT NULL DEFAULT 'x')"),
            &format!("INSERT INTO public.{table} (name) SELECT 'n' || g FROM generate_series(1, 30) g"),
        ]),
        &opts,
        &mut c.sink(),
    )
    .await
    .unwrap();

    let live = mgr.get(&id).unwrap();
    let objects = introspect::list_objects(&live.pool).await.unwrap();
    assert!(
        objects
            .iter()
            .any(|o| o.schema == "public" && o.name == table)
    );
    let cols = introspect::table_columns(&live.pool, "public", &table)
        .await
        .unwrap();
    assert_eq!(cols.len(), 2);
    assert!(cols[0].is_primary_key && !cols[1].is_primary_key);
    assert!(!cols[1].nullable);
    assert_eq!(cols[1].default_value.as_deref(), Some("'x'::text"));
    let pk = introspect::primary_key(&live.pool, "public", &table)
        .await
        .unwrap();

    let sql = snaptable_sqlgen::browse_sql(
        snaptable_sqlgen::Dialect::Postgres,
        &snaptable_sqlgen::BrowseQuery {
            schema: "public",
            table: &table,
            order_by: Some(("id", true)),
            stable_order: &pk,
            filter: Some("id > 5"),
            limit: 10,
            offset: 0,
        },
    );
    let mut c = Collected::default();
    let r = mgr
        .run_on_pool("browse", &id, &[sql], &opts, &mut c.sink())
        .await
        .unwrap();
    assert_ok(&r);
    assert_eq!(c.statements[0].rows.len(), 10);
    assert_eq!(c.statements[0].rows[0][0], json!(30));

    let mut c = Collected::default();
    mgr.run_in_session(
        "drop",
        &session,
        &stmts(&[&format!("DROP TABLE public.{table}")]),
        &opts,
        &mut c.sink(),
    )
    .await
    .unwrap();
}
