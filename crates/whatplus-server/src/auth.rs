//! 驗證用的設定快照（公鑰＋IP 白名單），依資料庫 revision 重新載入。
//!
//! 對應 GitStatus 的 RuleSet / RuleSetHolder：讀取時拿不可變快照，重新載入時原子替換。

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::Result;
use whatplus_proto::Role;
use whatplus_ssh::allow::IpAllowList;

use crate::db::{AuthKey, Store};

#[derive(Debug, Default)]
pub struct AuthSnapshot {
    pub revision: i64,
    /// 依指紋索引，只含啟用中的公鑰。
    keys: HashMap<String, AuthKey>,
    allow: IpAllowList,
}

/// 驗證成功的身分。
#[derive(Debug, Clone)]
pub struct Identity {
    pub key_id: i64,
    pub name: String,
    pub role: Role,
}

impl AuthSnapshot {
    pub fn load(store: &Store) -> Result<Self> {
        // revision 前後各讀一次，確保讀到的是一致的設定。
        loop {
            let before = store.revision()?;
            let keys = store.auth_keys()?;
            let cidrs = store.allowed_networks()?;
            if store.revision()? != before {
                continue;
            }
            let allow = IpAllowList::parse(&cidrs).unwrap_or_else(|e| {
                tracing::warn!("ignoring invalid allowlist entry: {e}");
                IpAllowList::default()
            });
            let keys = keys
                .into_iter()
                .filter(|k| k.enabled)
                .map(|k| (k.fingerprint.clone(), k))
                .collect();
            return Ok(Self {
                revision: before,
                keys,
                allow,
            });
        }
    }

    /// 至少有一把啟用中的 admin 公鑰。沒有的話伺服器不接受任何連線、不排程。
    pub fn has_admin(&self) -> bool {
        self.keys.values().any(|k| k.role == Role::Admin)
    }

    pub fn ip_allowed(&self, ip: IpAddr) -> bool {
        self.allow.contains(ip)
    }

    pub fn knows(&self, fingerprint: &str) -> bool {
        self.keys.contains_key(fingerprint)
    }

    /// 公鑰已註冊、啟用，且來源 IP 符合該公鑰的限制（若有）。
    pub fn authenticate(&self, fingerprint: &str, ip: Option<IpAddr>) -> Option<Identity> {
        let key = self.keys.get(fingerprint)?;
        if !key.allowed_cidrs.is_empty() {
            let list = IpAllowList::parse(&key.allowed_cidrs).ok()?;
            if !ip.is_some_and(|ip| list.contains(ip)) {
                return None;
            }
        }
        Some(Identity {
            key_id: key.id,
            name: key.name.clone(),
            role: key.role,
        })
    }
}

pub struct AuthHolder {
    current: RwLock<Arc<AuthSnapshot>>,
}

impl AuthHolder {
    pub fn new(snapshot: AuthSnapshot) -> Self {
        Self {
            current: RwLock::new(Arc::new(snapshot)),
        }
    }

    pub fn current(&self) -> Arc<AuthSnapshot> {
        self.current
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// revision 改變時重新載入；回傳是否有更新。
    pub fn refresh(&self, store: &Store) -> Result<bool> {
        if store.revision()? == self.current().revision {
            return Ok(false);
        }
        let next = AuthSnapshot::load(store)?;
        tracing::info!(
            revision = next.revision,
            keys = next.keys.len(),
            "authorization settings reloaded"
        );
        *self.current.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(next);
        Ok(true)
    }

    pub async fn poll(self: Arc<Self>, store: Arc<Store>, interval: Duration) {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            if let Err(e) = self.refresh(&store) {
                tracing::warn!("reloading authorization settings failed: {e:#}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::NewKey;

    #[test]
    fn per_key_cidr_and_admin_gate() {
        let store = Store::open_in_memory().unwrap();
        let snap = AuthSnapshot::load(&store).unwrap();
        assert!(!snap.has_admin());

        store
            .add_key(NewKey {
                name: "laptop",
                role: Role::Agent,
                public_key: "k1",
                fingerprint: "fp1",
                allowed_cidrs: &["192.168.1.0/24".to_string()],
            })
            .unwrap();
        store
            .add_key(NewKey {
                name: "me",
                role: Role::Admin,
                public_key: "k2",
                fingerprint: "fp2",
                allowed_cidrs: &[],
            })
            .unwrap();
        store.add_allowed_network("192.168.0.0/16").unwrap();

        let holder = AuthHolder::new(snap);
        assert!(holder.refresh(&store).unwrap());
        let snap = holder.current();
        assert!(snap.has_admin());
        assert!(snap.ip_allowed("192.168.5.5".parse().unwrap()));
        assert!(!snap.ip_allowed("10.0.0.1".parse().unwrap()));
        assert!(
            snap.authenticate("fp1", Some("192.168.1.9".parse().unwrap()))
                .is_some()
        );
        assert!(
            snap.authenticate("fp1", Some("192.168.2.9".parse().unwrap()))
                .is_none()
        );
        assert!(
            snap.authenticate("fp2", Some("192.168.2.9".parse().unwrap()))
                .is_some()
        );
        assert!(snap.authenticate("nope", None).is_none());
    }
}
