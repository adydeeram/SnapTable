-- SnapTable metadata schema v1 (SPEC §2.2).
-- Hard rule: no secret values in this database. `has_*` columns only flag that a
-- Keychain item exists; the Keychain account name is derived from the row id.

CREATE TABLE environments (
    id               TEXT PRIMARY KEY,
    name             TEXT NOT NULL,
    color            TEXT NOT NULL,
    protection_level TEXT NOT NULL DEFAULT 'none'
                     CHECK (protection_level IN ('none', 'confirm_writes', 'read_only')),
    sort_order       INTEGER NOT NULL DEFAULT 0,
    is_builtin       INTEGER NOT NULL DEFAULT 0
);

INSERT INTO environments (id, name, color, protection_level, sort_order, is_builtin) VALUES
    ('local',      'Local',      '#22c55e', 'none',           0, 1),
    ('staging',    'Staging',    '#f59e0b', 'confirm_writes', 1, 1),
    ('production', 'Production', '#ef4444', 'read_only',      2, 1);

CREATE TABLE connection_groups (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    parent_id  TEXT REFERENCES connection_groups(id) ON DELETE CASCADE,
    sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE connections (
    id                TEXT PRIMARY KEY,
    group_id          TEXT REFERENCES connection_groups(id) ON DELETE SET NULL,
    name              TEXT NOT NULL,
    engine            TEXT NOT NULL,
    environment_id    TEXT REFERENCES environments(id) ON DELETE SET NULL,
    host              TEXT,
    port              INTEGER,
    database          TEXT,
    username          TEXT,
    has_password      INTEGER NOT NULL DEFAULT 0,
    sqlite_path       TEXT,
    tls_mode          TEXT NOT NULL DEFAULT 'prefer',
    options_json      TEXT NOT NULL DEFAULT '{}',
    sort_order        INTEGER NOT NULL DEFAULT 0,
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL,
    last_connected_at INTEGER
);

CREATE TABLE connection_ssh (
    connection_id          TEXT PRIMARY KEY REFERENCES connections(id) ON DELETE CASCADE,
    host                   TEXT NOT NULL,
    port                   INTEGER NOT NULL DEFAULT 22,
    username               TEXT NOT NULL,
    auth_method            TEXT NOT NULL CHECK (auth_method IN ('password', 'key', 'agent')),
    key_path               TEXT,
    has_password           INTEGER NOT NULL DEFAULT 0,
    has_passphrase         INTEGER NOT NULL DEFAULT 0,
    known_host_fingerprint TEXT,
    keepalive_secs         INTEGER NOT NULL DEFAULT 30
);

CREATE TABLE connection_tls (
    connection_id             TEXT PRIMARY KEY REFERENCES connections(id) ON DELETE CASCADE,
    ca_cert_path              TEXT,
    client_cert_path          TEXT,
    client_key_path           TEXT,
    has_client_key_passphrase INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE saved_queries (
    id            TEXT PRIMARY KEY,
    connection_id TEXT REFERENCES connections(id) ON DELETE SET NULL,
    name          TEXT NOT NULL,
    sql           TEXT NOT NULL,
    folder        TEXT,
    updated_at    INTEGER NOT NULL
);

CREATE TABLE query_history (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_id TEXT REFERENCES connections(id) ON DELETE CASCADE,
    database      TEXT,
    sql           TEXT NOT NULL,
    status        TEXT NOT NULL,
    rows_affected INTEGER,
    elapsed_ms    INTEGER NOT NULL,
    executed_at   INTEGER NOT NULL
);
CREATE INDEX query_history_conn_time ON query_history (connection_id, executed_at DESC);

CREATE VIRTUAL TABLE query_history_fts USING fts5 (
    sql, content = 'query_history', content_rowid = 'id', tokenize = 'trigram'
);
CREATE TRIGGER query_history_ai AFTER INSERT ON query_history BEGIN
    INSERT INTO query_history_fts (rowid, sql) VALUES (new.id, new.sql);
END;
CREATE TRIGGER query_history_ad AFTER DELETE ON query_history BEGIN
    INSERT INTO query_history_fts (query_history_fts, rowid, sql) VALUES ('delete', old.id, old.sql);
END;

CREATE TABLE workspace_state (
    key  TEXT PRIMARY KEY,
    json TEXT NOT NULL
);

CREATE TABLE app_settings (
    key        TEXT PRIMARY KEY,
    value_json TEXT NOT NULL
);
