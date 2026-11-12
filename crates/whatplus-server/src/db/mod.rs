//! SQLite 存取。所有 SQL 集中在這個模組底下。
//!
//! daemon 與 admin TUI 是兩個 process，各自開一個連線；以 WAL、busy_timeout 與
//! `BEGIN IMMEDIATE` 避免寫入衝突。設定變更經 [`Store::mutate`]，daemon 輪詢 revision 重新載入。

mod keys;
mod network;
mod records;
mod reports;
pub mod runtime;
pub mod sql;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use anyhow::{Context, Result, bail};

pub use keys::{AuthKey, NewKey};
pub use records::StoredRecord;
use sql::Conn;

/// 資料庫 schema 版本（`PRAGMA user_version`）。變更 schema 時加一並寫升級步驟。
pub const SCHEMA_VERSION: i64 = 1;

pub struct Store {
    conn: Mutex<Conn>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                bail!("database directory {} does not exist", parent.display());
            }
        }
        let conn = Conn::open(path)
            .with_context(|| format!("cannot open database {} (is the directory writable?)", path.display()))?;
        let store = Self::init(conn)?;
        restrict_permissions(path);
        Ok(store)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Conn::open_in_memory()?)
    }

    fn init(mut conn: Conn) -> Result<Self> {
        conn.set_busy_timeout(5000)?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
        let version: i64 = conn
            .query_row("PRAGMA user_version", &[], |r| r.get(0))?
            .unwrap_or(0);
        if version > SCHEMA_VERSION {
            bail!(
                "database schema version {version} is newer than this build ({SCHEMA_VERSION}); \
                 upgrade the server instead of downgrading"
            );
        }
        conn.execute_batch(include_str!("schema.sql"))?;
        conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Conn> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// 修改設定：在單一 transaction 內執行 `f` 並把 revision 加一；失敗則全部 rollback。
    pub fn mutate<T>(&self, f: impl FnOnce(&Conn) -> Result<T>) -> Result<T> {
        self.lock().transaction(true, |tx| {
            let value = f(tx)?;
            tx.execute(
                "UPDATE configuration_state SET revision = revision + 1 WHERE id = 1",
                &[],
            )?;
            Ok(value)
        })
    }

    pub fn revision(&self) -> Result<i64> {
        Ok(self
            .lock()
            .query_row("SELECT revision FROM configuration_state WHERE id = 1", &[], |r| r.get(0))?
            .unwrap_or(0))
    }
}

pub(crate) fn now_rfc3339() -> String {
    let now = jiff::Timestamp::now();
    now.round(jiff::Unit::Second).unwrap_or(now).to_string()
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}
