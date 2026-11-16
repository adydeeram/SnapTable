use super::*;
use secrecy::ExposeSecret;
use snaptable_secrets::MemoryStore;

const SENTINEL: &str = "SENTINEL-p4ssw0rd-7f3a9c";

fn pg_input(password: SecretChange) -> ConnectionInput {
    ConnectionInput {
        id: None,
        group_id: None,
        name: "Local PG".into(),
        engine: Engine::Postgres,
        environment_id: Some("local".into()),
        host: Some(" localhost ".into()),
        port: Some(5432),
        database: Some("app".into()),
        username: Some("postgres".into()),
        sqlite_path: None,
        tls_mode: TlsMode::Prefer,
        options: ConnectionOptions {
            statement_timeout_ms: Some(30_000),
            search_path: None,
        },
        password,
    }
}

#[tokio::test]
async fn seeds_builtin_environments() {
    let store = Store::open_in_memory().await.unwrap();
    let envs = store.environments().await.unwrap();
    let names: Vec<_> = envs.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["Local", "Staging", "Production"]);
    assert_eq!(envs[2].protection_level, ProtectionLevel::ReadOnly);
}

#[tokio::test]
async fn connection_crud_routes_password_to_secret_store() {
    let store = Store::open_in_memory().await.unwrap();
    let secrets = MemoryStore::default();

    let saved = store
        .save_connection(
            &secrets,
            pg_input(SecretChange::Set {
                value: SENTINEL.into(),
            }),
        )
        .await
        .unwrap();
    assert!(saved.has_password);
    assert_eq!(
        saved.host.as_deref(),
        Some("localhost"),
        "inputs are trimmed"
    );
    assert_eq!(saved.options.statement_timeout_ms, Some(30_000));
    let key = SecretKey::DbPassword {
        connection_id: saved.id.clone(),
    };
    assert_eq!(
        secrets.get(&key).unwrap().unwrap().expose_secret(),
        SENTINEL
    );

    // Keep leaves the secret alone; Clear removes it.
    let mut update = pg_input(SecretChange::Keep);
    update.id = Some(saved.id.clone());
    update.name = "Renamed".into();
    let updated = store
        .save_connection(&secrets, update.clone())
        .await
        .unwrap();
    assert!(updated.has_password);
    assert_eq!(updated.created_at, saved.created_at);
    assert_eq!(updated.name, "Renamed");

    update.password = SecretChange::Clear;
    let cleared = store.save_connection(&secrets, update).await.unwrap();
    assert!(!cleared.has_password);
    assert!(secrets.get(&key).unwrap().is_none());

    store.delete_connection(&secrets, &saved.id).await.unwrap();
    assert!(store.connections().await.unwrap().is_empty());
}

#[tokio::test]
async fn delete_removes_keychain_items() {
    let store = Store::open_in_memory().await.unwrap();
    let secrets = MemoryStore::default();
    let saved = store
        .save_connection(
            &secrets,
            pg_input(SecretChange::Set {
                value: SENTINEL.into(),
            }),
        )
        .await
        .unwrap();
    store.delete_connection(&secrets, &saved.id).await.unwrap();
    let key = SecretKey::DbPassword {
        connection_id: saved.id,
    };
    assert!(secrets.get(&key).unwrap().is_none());
}

/// Phase 1 DoD: no secret ever lands in the metadata DB file (including WAL).
#[tokio::test]
async fn sentinel_password_never_written_to_disk() {
    let dir = std::env::temp_dir().join(format!("snaptable-test-{}", uuid::Uuid::new_v4()));
    let path = dir.join("snaptable.db");
    {
        let store = Store::open(&path).await.unwrap();
        let secrets = MemoryStore::default();
        let saved = store
            .save_connection(
                &secrets,
                pg_input(SecretChange::Set {
                    value: SENTINEL.into(),
                }),
            )
            .await
            .unwrap();
        store
            .add_history(&saved.id, "select 1", HistoryStatus::Success, Some(1), 3)
            .await
            .unwrap();
        store.pool.close().await;
    }
    for entry in std::fs::read_dir(&dir).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        assert!(
            !bytes
                .windows(SENTINEL.len())
                .any(|w| w == SENTINEL.as_bytes()),
            "sentinel found in metadata files"
        );
    }
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777
    };
    assert_eq!(mode, 0o600);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn validation_rejects_missing_fields() {
    let store = Store::open_in_memory().await.unwrap();
    let secrets = MemoryStore::default();
    let mut input = pg_input(SecretChange::Keep);
    input.host = Some("  ".into());
    assert!(matches!(
        store.save_connection(&secrets, input).await,
        Err(AppError::Invalid { .. })
    ));
}

#[tokio::test]
async fn history_search_fts_and_like() {
    let store = Store::open_in_memory().await.unwrap();
    let secrets = MemoryStore::default();
    let c = store
        .save_connection(&secrets, pg_input(SecretChange::Keep))
        .await
        .unwrap();
    store
        .add_history(
            &c.id,
            "SELECT * FROM users",
            HistoryStatus::Success,
            None,
            5,
        )
        .await
        .unwrap();
    store
        .add_history(&c.id, "select * from orders", HistoryStatus::Error, None, 7)
        .await
        .unwrap();
    store
        .add_history(
            &c.id,
            "update t set a = 1",
            HistoryStatus::Success,
            Some(3),
            2,
        )
        .await
        .unwrap();

    let hits = store.search_history(Some(&c.id), "user", 10).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].sql, "SELECT * FROM users");

    // Trigram matching is case-insensitive.
    assert_eq!(
        store.search_history(None, "FROM", 10).await.unwrap().len(),
        2
    );
    // Short queries use LIKE.
    assert_eq!(store.search_history(None, "t", 10).await.unwrap().len(), 3);
    // Empty query lists everything, newest first.
    let all = store.search_history(None, "", 10).await.unwrap();
    assert_eq!(all[0].sql, "update t set a = 1");
    // Quotes in the search don't break FTS syntax.
    assert!(
        store
            .search_history(None, "a \"quoted\" term", 10)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn workspace_roundtrip() {
    let store = Store::open_in_memory().await.unwrap();
    assert!(store.workspace_get("tabs").await.unwrap().is_none());
    let v = serde_json::json!({"tabs": [1, 2]});
    store.workspace_set("tabs", &v).await.unwrap();
    assert_eq!(store.workspace_get("tabs").await.unwrap(), Some(v));
}
