//! 全域 IP 白名單（allowed_network）。

use anyhow::Result;

use super::Store;
use crate::params;

impl Store {
    pub fn allowed_networks(&self) -> Result<Vec<String>> {
        self.lock()
            .query_map("SELECT cidr FROM allowed_network ORDER BY cidr", &[], |r| r.get(0))
    }

    /// `cidr` 必須先經 `whatplus_ssh::allow::parse_cidr` 正規化。
    pub fn add_allowed_network(&self, cidr: &str) -> Result<()> {
        self.mutate(|tx| {
            tx.execute(
                "INSERT OR IGNORE INTO allowed_network (cidr) VALUES (?)",
                params![cidr],
            )?;
            Ok(())
        })
    }

    pub fn remove_allowed_network(&self, cidr: &str) -> Result<()> {
        self.mutate(|tx| {
            tx.execute("DELETE FROM allowed_network WHERE cidr = ?", params![cidr])?;
            Ok(())
        })
    }
}
