mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::*;
use serde_json::json;
use snaptable_core::AppError;
use snaptable_core::model::Engine;
use snaptable_core::query::ColumnKind;
use snaptable_drivers::exec::ExecOptions;
use snaptable_drivers::{ConnectParams, ConnectionManager, DbPool, introspect};

struct TempDb(std::path::PathBuf);
impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn temp_db() -> TempDb {
    let path = std::env::temp_dir().join(format!("snaptable-it-{}.db", uuid::Uuid::new_v4()));
    // Create the file with a schema using a throwaway connection via the CLI-free route.
    std::fs::File::create(&path).unwrap();
    TempDb(path)
}

fn params(db: &TempDb, read_only: bool) -> ConnectParams {
    let mut c = connection(Engine::Sqlite);
    c.sqlite_path = Some(db.0.display().to_string());
    ConnectParams {
        connection: c,
        password: None,
        read_only,
    }
}

async fn setup(db: &TempDb) -> (ConnectionManager, String, String) {
    let p = params(db, false);
    let id = p.connection.id.clone();
    let mgr = ConnectionManager::default();
    mgr.connect(p).await.unwrap();
    let session = mgr.open_session(&id).await.unwrap();
    (mgr, id, session)
}

#[tokio::test]
async fn missing_file_is_a_connection_error() {
    let mut c = connection(Engine::Sqlite);
    c.sqlite_path = Some("/definitely/not/here.db".into());
    let r = DbPool::test(&ConnectParams {
        connection: c,
        password: None,
        read_only: false,
    })
    .await;
    assert!(matches!(r, Err(AppError::Connection { .. })));
}

#[tokio::test]
async fn decodes_dynamic_types_and_introspects() {
    let db = temp_db();
    let (mgr, id, session) = setup(&db).await;
    let mut c = Collected::default();
    let r = mgr
        .run_in_session(
            "t",
            &session,
            &stmts(&[
                "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL, price REAL, data BLOB, active BOOLEAN, meta JSON)",
                "INSERT INTO items (name, price, data, active, meta) VALUES ('a', 1.25, x'cafe', 1, '{\"k\":1}'), ('b', NULL, NULL, 0, NULL)",
                "SELECT * FROM items ORDER BY id",
                "SELECT 9007199254740993 AS big, 'mixed' AS m",
            ]),
            &ExecOptions::default(),
            &mut c.sink(),
        )
        .await
        .unwrap();
    assert_ok(&r);
    assert_eq!(c.statements[1].rows_affected, Some(2));
    let s = &c.statements[2];
    let kinds: Vec<_> = s.columns.iter().map(|c| c.kind).collect();
    // sqlx normalises unknown SQLite declared types (e.g. `JSON`), so the last column is not asserted.
    assert_eq!(
        kinds[..5],
        [
            ColumnKind::Number,
            ColumnKind::Text,
            ColumnKind::Number,
            ColumnKind::Bytes,
            ColumnKind::Bool
        ]
    );
    assert_eq!(
        s.rows[0],
        vec![
            json!(1),
            json!("a"),
            json!(1.25),
            json!({"$bytes": "cafe", "len": 2}),
            json!(1),
            json!("{\"k\":1}")
        ]
    );
    assert_eq!(s.rows[1][2], json!(null));
    assert_eq!(c.statements[3].rows[0][0], json!("9007199254740993"));

    let live = mgr.get(&id).unwrap();
    let objects = introspect::list_objects(&live.pool).await.unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(
        (objects[0].schema.as_str(), objects[0].name.as_str()),
        ("main", "items")
    );
    let pk = introspect::primary_key(&live.pool, "main", "items")
        .await
        .unwrap();
    assert_eq!(pk, ["id"]);
}

#[tokio::test]
async fn cancel_drops_long_running_stream() {
    let db = temp_db();
    let (mgr, _, session) = setup(&db).await;
    let mgr = Arc::new(mgr);
    let m2 = mgr.clone();
    let canceller = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        m2.tasks().cancel("long");
    });
    let started = Instant::now();
    let mut c = Collected::default();
    // Produces rows continuously, so cancellation lands between rows.
    let r = mgr
        .run_in_session(
            "long",
            &session,
            &stmts(&[
                "WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM n) SELECT x FROM n",
            ]),
            &ExecOptions {
                max_rows: 1_000,
                ..Default::default()
            },
            &mut c.sink(),
        )
        .await
        .unwrap();
    canceller.await.unwrap();
    assert!(r.summary.cancelled);
    assert!(started.elapsed() < Duration::from_secs(3));

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
async fn read_only_rejects_writes() {
    let db = temp_db();
    {
        let (mgr, _, session) = setup(&db).await;
        let mut c = Collected::default();
        mgr.run_in_session(
            "mk",
            &session,
            &stmts(&["CREATE TABLE t (x)"]),
            &ExecOptions::default(),
            &mut c.sink(),
        )
        .await
        .unwrap();
    }
    let p = params(&db, true);
    let id = p.connection.id.clone();
    let mgr = ConnectionManager::default();
    mgr.connect(p).await.unwrap();
    let session = mgr.open_session(&id).await.unwrap();
    let mut c = Collected::default();
    let r = mgr
        .run_in_session(
            "ro",
            &session,
            &stmts(&["INSERT INTO t VALUES (1)"]),
            &ExecOptions::default(),
            &mut c.sink(),
        )
        .await
        .unwrap();
    assert_eq!(r.summary.errors, 1);
    assert!(matches!(
        c.statements[0].error,
        Some(AppError::Query { .. })
    ));
}

#[tokio::test]
async fn concurrent_runs_in_one_session_are_rejected() {
    let db = temp_db();
    let (mgr, _, session) = setup(&db).await;
    let mgr = Arc::new(mgr);
    let (m2, s2) = (mgr.clone(), session.clone());
    let first = tokio::spawn(async move {
        let mut c = Collected::default();
        m2.run_in_session(
            "first",
            &s2,
            &stmts(&["WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM n LIMIT 3000000) SELECT count(*) FROM n"]),
            &ExecOptions::default(),
            &mut c.sink(),
        )
        .await
        .map(|_| ())
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    let mut c = Collected::default();
    let second = mgr
        .run_in_session(
            "second",
            &session,
            &stmts(&["SELECT 1"]),
            &ExecOptions::default(),
            &mut c.sink(),
        )
        .await;
    assert!(matches!(second, Err(AppError::Invalid { .. })));
    first.await.unwrap().unwrap();
}
