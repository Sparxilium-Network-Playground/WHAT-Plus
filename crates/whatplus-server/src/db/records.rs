//! 來源端上傳的用量紀錄（usage_record）。

use anyhow::Result;
use jiff::Timestamp;
use sha2::{Digest, Sha256};
use what_core::{Source, TokenCounts, UsageRecord};

use super::Store;
use crate::params;

/// 讀出的紀錄，附上來源（機器）名稱。
#[derive(Debug, Clone)]
pub struct StoredRecord {
    pub agent_name: String,
    pub record: UsageRecord,
}

/// 紀錄的去重鍵：同一筆 API 回應重送時得到相同的鍵（費用不列入，價格表更新後會覆寫）。
pub fn record_key(r: &UsageRecord) -> String {
    let t = &r.tokens;
    let raw = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}",
        r.source.id(),
        r.timestamp,
        r.model,
        r.session_id,
        t.input,
        t.output,
        t.cache_creation,
        t.cache_read,
        t.reasoning
    );
    let digest = Sha256::digest(raw.as_bytes());
    digest[..16].iter().map(|b| format!("{b:02x}")).collect()
}

fn source_from_id(id: &str) -> Source {
    Source::ALL
        .into_iter()
        .find(|s| s.id() == id)
        .unwrap_or(Source::ClaudeCode)
}

fn clamp_u64(v: u64) -> i64 {
    v.min(i64::MAX as u64) as i64
}

impl Store {
    /// 寫入一批紀錄；已存在的鍵會被覆寫。回傳寫入筆數。
    pub fn upsert_records(&self, agent_id: i64, records: &[UsageRecord]) -> Result<usize> {
        self.lock().transaction(false, |tx| {
            for r in records {
                let cost = if r.cost_usd.is_finite() && r.cost_usd >= 0.0 {
                    r.cost_usd
                } else {
                    0.0
                };
                tx.execute(
                    "INSERT INTO usage_record (agent_id, record_key, ts_ms, source, model, project, session_id,
                        input, output, cache_creation, cache_read, reasoning, cost_usd, pricing_missing)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT(agent_id, record_key) DO UPDATE SET
                        project = excluded.project, cost_usd = excluded.cost_usd,
                        pricing_missing = excluded.pricing_missing",
                    params![
                        agent_id,
                        record_key(r),
                        r.timestamp.as_millisecond(),
                        r.source.id(),
                        &r.model,
                        &r.project,
                        &r.session_id,
                        clamp_u64(r.tokens.input),
                        clamp_u64(r.tokens.output),
                        clamp_u64(r.tokens.cache_creation),
                        clamp_u64(r.tokens.cache_read),
                        clamp_u64(r.tokens.reasoning),
                        cost,
                        r.pricing_missing,
                    ],
                )?;
            }
            Ok(records.len())
        })
    }

    /// 這個來源最新一筆紀錄的時間與總筆數。
    pub fn upload_state(&self, agent_id: i64) -> Result<(Option<Timestamp>, u64)> {
        let row = self.lock().query_row(
            "SELECT MAX(ts_ms), COUNT(*) FROM usage_record WHERE agent_id = ?",
            params![agent_id],
            |r| Ok((r.get::<Option<i64>>(0)?, r.get::<i64>(1)?)),
        )?;
        let (max, count) = row.unwrap_or((None, 0));
        Ok((
            max.and_then(|ms| Timestamp::from_millisecond(ms).ok()),
            count as u64,
        ))
    }

    pub fn record_count(&self) -> Result<u64> {
        let n: Option<i64> = self
            .lock()
            .query_row("SELECT COUNT(*) FROM usage_record", &[], |r| r.get(0))?;
        Ok(n.unwrap_or(0) as u64)
    }

    /// 讀取 `[start, end)` 期間的紀錄（只含仍啟用中的來源）。
    pub fn records_between(&self, start: Timestamp, end: Timestamp) -> Result<Vec<StoredRecord>> {
        self.lock().query_map(
            "SELECT k.name, u.ts_ms, u.source, u.model, u.project, u.session_id, u.input, u.output,
                    u.cache_creation, u.cache_read, u.reasoning, u.cost_usd, u.pricing_missing
             FROM usage_record u JOIN authorized_key k ON k.id = u.agent_id
             WHERE u.ts_ms >= ? AND u.ts_ms < ? AND k.enabled = 1",
            params![start.as_millisecond(), end.as_millisecond()],
            |r| {
                let ms: i64 = r.get(1)?;
                let source: String = r.get(2)?;
                Ok(StoredRecord {
                    agent_name: r.get(0)?,
                    record: UsageRecord {
                        source: source_from_id(&source),
                        timestamp: Timestamp::from_millisecond(ms).unwrap_or(Timestamp::UNIX_EPOCH),
                        model: r.get(3)?,
                        project: r.get(4)?,
                        session_id: r.get(5)?,
                        tokens: TokenCounts {
                            input: r.get::<i64>(6)? as u64,
                            output: r.get::<i64>(7)? as u64,
                            cache_creation: r.get::<i64>(8)? as u64,
                            cache_read: r.get::<i64>(9)? as u64,
                            reasoning: r.get::<i64>(10)? as u64,
                        },
                        cost_usd: r.get(11)?,
                        pricing_missing: r.get_bool(12)?,
                    },
                })
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::NewKey;
    use whatplus_proto::Role;

    fn sample(ts: &str, cost: f64) -> UsageRecord {
        UsageRecord {
            source: Source::Codex,
            timestamp: ts.parse().unwrap(),
            model: "gpt-5".into(),
            project: "demo".into(),
            session_id: "s1".into(),
            tokens: TokenCounts {
                input: 10,
                output: 5,
                ..Default::default()
            },
            cost_usd: cost,
            pricing_missing: false,
        }
    }

    #[test]
    fn upsert_dedupes_and_updates_cost() {
        let store = Store::open_in_memory().unwrap();
        let id = store
            .add_key(NewKey {
                name: "pc",
                role: Role::Agent,
                public_key: "ssh-ed25519 AAAA",
                fingerprint: "SHA256:x",
                allowed_cidrs: &[],
            })
            .unwrap();
        let a = sample("2026-11-01T00:00:00Z", 1.0);
        let b = sample("2026-11-02T00:00:00Z", 2.0);
        store.upsert_records(id, &[a.clone(), b]).unwrap();
        store.upsert_records(id, &[sample("2026-11-01T00:00:00Z", 3.0)]).unwrap();

        let (latest, count) = store.upload_state(id).unwrap();
        assert_eq!(count, 2);
        assert_eq!(latest, Some("2026-11-02T00:00:00Z".parse().unwrap()));

        let rows = store
            .records_between("2026-11-01T00:00:00Z".parse().unwrap(), "2026-11-02T00:00:00Z".parse().unwrap())
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].record.cost_usd, 3.0);
        assert_eq!(rows[0].agent_name, "pc");

        store.set_key_enabled(id, false).unwrap();
        let rows = store
            .records_between(Timestamp::UNIX_EPOCH, "2030-01-01T00:00:00Z".parse().unwrap())
            .unwrap();
        assert!(rows.is_empty());
    }
}
