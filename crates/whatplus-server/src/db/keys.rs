//! 已註冊的公鑰（authorized_key）與連線紀錄（key_activity）。

use anyhow::{Result, bail};
use whatplus_proto::{KeyInfo, Role};

use super::{Store, now_rfc3339};
use crate::params;

/// daemon 驗證用的公鑰快照。
#[derive(Debug, Clone)]
pub struct AuthKey {
    pub id: i64,
    pub name: String,
    pub role: Role,
    pub fingerprint: String,
    pub allowed_cidrs: Vec<String>,
    pub enabled: bool,
}

pub struct NewKey<'a> {
    pub name: &'a str,
    pub role: Role,
    pub public_key: &'a str,
    pub fingerprint: &'a str,
    pub allowed_cidrs: &'a [String],
}

fn parse_role(raw: String) -> Role {
    // schema 的 CHECK 只允許 admin / agent；其他值一律當成權限最小的 agent。
    Role::parse(&raw).unwrap_or(Role::Agent)
}

fn parse_cidrs(raw: String) -> Vec<String> {
    serde_json::from_str(&raw).unwrap_or_default()
}

impl Store {
    pub fn auth_keys(&self) -> Result<Vec<AuthKey>> {
        self.lock().query_map(
            "SELECT id, name, role, fingerprint, allowed_cidrs, enabled FROM authorized_key",
            &[],
            |r| {
                Ok(AuthKey {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    role: parse_role(r.get(2)?),
                    fingerprint: r.get(3)?,
                    allowed_cidrs: parse_cidrs(r.get(4)?),
                    enabled: r.get_bool(5)?,
                })
            },
        )
    }

    pub fn list_keys(&self) -> Result<Vec<KeyInfo>> {
        self.lock().query_map(
            "SELECT k.id, k.name, k.role, k.fingerprint, k.public_key, k.allowed_cidrs, k.enabled,
                    a.last_seen_at, a.last_seen_ip,
                    (SELECT COUNT(*) FROM usage_record u WHERE u.agent_id = k.id)
             FROM authorized_key k LEFT JOIN key_activity a ON a.key_id = k.id
             ORDER BY k.role, k.name",
            &[],
            |r| {
                Ok(KeyInfo {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    role: parse_role(r.get(2)?),
                    fingerprint: r.get(3)?,
                    public_key: r.get(4)?,
                    allowed_cidrs: parse_cidrs(r.get(5)?),
                    enabled: r.get_bool(6)?,
                    last_seen_at: r.get(7)?,
                    last_seen_ip: r.get(8)?,
                    record_count: r.get::<i64>(9)? as u64,
                })
            },
        )
    }

    pub fn key_role(&self, id: i64) -> Result<Option<Role>> {
        self.lock().query_row(
            "SELECT role FROM authorized_key WHERE id = ?",
            params![id],
            |r| Ok(parse_role(r.get(0)?)),
        )
    }

    pub fn add_key(&self, key: NewKey<'_>) -> Result<i64> {
        let cidrs = serde_json::to_string(key.allowed_cidrs)?;
        self.mutate(|tx| {
            let exists: Option<String> = tx.query_row(
                "SELECT name FROM authorized_key WHERE fingerprint = ?",
                params![key.fingerprint],
                |r| r.get(0),
            )?;
            if let Some(name) = exists {
                bail!("this key is already registered as \"{name}\"");
            }
            tx.execute(
                "INSERT INTO authorized_key (name, role, public_key, fingerprint, allowed_cidrs, enabled, created_at)
                 VALUES (?, ?, ?, ?, ?, 1, ?)",
                params![key.name, key.role.as_str(), key.public_key, key.fingerprint, cidrs, now_rfc3339()],
            )?;
            tx.last_insert_rowid()
        })
    }

    pub fn update_key(&self, id: i64, name: &str, allowed_cidrs: &[String]) -> Result<()> {
        let cidrs = serde_json::to_string(allowed_cidrs)?;
        self.mutate(|tx| {
            let n = tx.execute(
                "UPDATE authorized_key SET name = ?, allowed_cidrs = ? WHERE id = ?",
                params![name, cidrs, id],
            )?;
            if n == 0 {
                bail!("key not found");
            }
            Ok(())
        })
    }

    pub fn set_key_role(&self, id: i64, role: Role) -> Result<()> {
        self.mutate(|tx| {
            tx.execute(
                "UPDATE authorized_key SET role = ? WHERE id = ?",
                params![role.as_str(), id],
            )?;
            Ok(())
        })
    }

    pub fn set_key_enabled(&self, id: i64, enabled: bool) -> Result<()> {
        self.mutate(|tx| {
            let n = tx.execute(
                "UPDATE authorized_key SET enabled = ? WHERE id = ?",
                params![enabled, id],
            )?;
            if n == 0 {
                bail!("key not found");
            }
            Ok(())
        })
    }

    /// 刪除公鑰，連同這個來源上傳的所有紀錄。
    pub fn delete_key(&self, id: i64) -> Result<()> {
        self.mutate(|tx| {
            tx.execute("DELETE FROM usage_record WHERE agent_id = ?", params![id])?;
            tx.execute("DELETE FROM key_activity WHERE key_id = ?", params![id])?;
            let n = tx.execute("DELETE FROM authorized_key WHERE id = ?", params![id])?;
            if n == 0 {
                bail!("key not found");
            }
            Ok(())
        })
    }

    /// 記錄成功登入（執行期狀態，不 bump revision）。
    pub fn record_activity(&self, key_id: i64, ip: &str) -> Result<()> {
        self.lock().execute(
            "INSERT INTO key_activity (key_id, last_seen_at, last_seen_ip) VALUES (?, ?, ?)
             ON CONFLICT(key_id) DO UPDATE SET last_seen_at = excluded.last_seen_at, last_seen_ip = excluded.last_seen_ip",
            params![key_id, now_rfc3339(), ip],
        )?;
        Ok(())
    }
}
