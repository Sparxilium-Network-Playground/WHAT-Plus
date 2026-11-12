-- 設定類資料表的修改一律經 Store::mutate（同一個 transaction 內 revision + 1）。
-- 執行期狀態（key_activity、schedule_run、destination_health、runtime_status）不 bump revision。

CREATE TABLE IF NOT EXISTS configuration_state (
    id       INTEGER PRIMARY KEY CHECK (id = 1),
    revision INTEGER NOT NULL
);
INSERT OR IGNORE INTO configuration_state (id, revision) VALUES (1, 0);

CREATE TABLE IF NOT EXISTS authorized_key (
    id            INTEGER PRIMARY KEY,
    name          TEXT    NOT NULL,
    role          TEXT    NOT NULL CHECK (role IN ('admin', 'agent')),
    public_key    TEXT    NOT NULL,
    fingerprint   TEXT    NOT NULL UNIQUE,
    allowed_cidrs TEXT    NOT NULL DEFAULT '[]',
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS allowed_network (
    cidr TEXT PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS key_activity (
    key_id       INTEGER PRIMARY KEY,
    last_seen_at TEXT,
    last_seen_ip TEXT
);

CREATE TABLE IF NOT EXISTS usage_record (
    agent_id        INTEGER NOT NULL,
    record_key      TEXT    NOT NULL,
    ts_ms           INTEGER NOT NULL,
    source          TEXT    NOT NULL,
    model           TEXT    NOT NULL,
    project         TEXT    NOT NULL,
    session_id      TEXT    NOT NULL,
    input           INTEGER NOT NULL,
    output          INTEGER NOT NULL,
    cache_creation  INTEGER NOT NULL,
    cache_read      INTEGER NOT NULL,
    reasoning       INTEGER NOT NULL,
    cost_usd        REAL    NOT NULL,
    pricing_missing INTEGER NOT NULL,
    PRIMARY KEY (agent_id, record_key)
);
CREATE INDEX IF NOT EXISTS usage_record_ts ON usage_record (ts_ms);

CREATE TABLE IF NOT EXISTS discord_destination (
    id           INTEGER PRIMARY KEY,
    name         TEXT    NOT NULL,
    guild_id     TEXT    NOT NULL,
    guild_name   TEXT    NOT NULL,
    channel_id   TEXT    NOT NULL,
    channel_name TEXT    NOT NULL,
    language     TEXT    NOT NULL,
    enabled      INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS destination_health (
    destination_id INTEGER PRIMARY KEY,
    last_result    TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS report_schedule (
    id              INTEGER PRIMARY KEY,
    name            TEXT    NOT NULL,
    kind_json       TEXT    NOT NULL,
    destination_ids TEXT    NOT NULL,
    content_json    TEXT    NOT NULL,
    enabled         INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS schedule_run (
    schedule_id INTEGER PRIMARY KEY,
    last_due_at TEXT NOT NULL,
    last_run_at TEXT NOT NULL,
    result      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS runtime_status (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
