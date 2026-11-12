//! Discord 目的地與報告排程。

use anyhow::{Result, bail};
use whatplus_proto::{Destination, Language, Schedule};

use super::{Store, now_rfc3339};
use crate::params;

/// 排程最近一次執行的狀態。
#[derive(Debug, Clone)]
pub struct ScheduleRun {
    pub last_due_at: String,
    pub result: String,
}

impl Store {
    pub fn list_destinations(&self) -> Result<Vec<Destination>> {
        self.lock().query_map(
            "SELECT d.id, d.name, d.guild_id, d.guild_name, d.channel_id, d.channel_name, d.language, d.enabled,
                    h.last_result
             FROM discord_destination d LEFT JOIN destination_health h ON h.destination_id = d.id
             ORDER BY d.name",
            &[],
            |r| {
                Ok(Destination {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    guild_id: r.get(2)?,
                    guild_name: r.get(3)?,
                    channel_id: r.get(4)?,
                    channel_name: r.get(5)?,
                    language: Language::from_code(&r.get::<String>(6)?),
                    enabled: r.get_bool(7)?,
                    last_result: r.get(8)?,
                })
            },
        )
    }

    pub fn destination(&self, id: i64) -> Result<Option<Destination>> {
        Ok(self.list_destinations()?.into_iter().find(|d| d.id == id))
    }

    /// 新增（`id == 0`）或修改目的地；回傳 id。
    pub fn save_destination(&self, d: &Destination) -> Result<i64> {
        self.mutate(|tx| {
            if d.id == 0 {
                tx.execute(
                    "INSERT INTO discord_destination (name, guild_id, guild_name, channel_id, channel_name, language, enabled)
                     VALUES (?, ?, ?, ?, ?, ?, ?)",
                    params![&d.name, &d.guild_id, &d.guild_name, &d.channel_id, &d.channel_name, d.language.code(), d.enabled],
                )?;
                return tx.last_insert_rowid();
            }
            let n = tx.execute(
                "UPDATE discord_destination SET name = ?, guild_id = ?, guild_name = ?, channel_id = ?,
                    channel_name = ?, language = ?, enabled = ? WHERE id = ?",
                params![&d.name, &d.guild_id, &d.guild_name, &d.channel_id, &d.channel_name, d.language.code(), d.enabled, d.id],
            )?;
            if n == 0 {
                bail!("destination not found");
            }
            Ok(d.id)
        })
    }

    pub fn delete_destination(&self, id: i64) -> Result<()> {
        self.mutate(|tx| {
            tx.execute(
                "DELETE FROM destination_health WHERE destination_id = ?",
                params![id],
            )?;
            tx.execute("DELETE FROM discord_destination WHERE id = ?", params![id])?;
            Ok(())
        })
    }

    pub fn set_destination_result(&self, id: i64, result: &str) -> Result<()> {
        self.lock().execute(
            "INSERT INTO destination_health (destination_id, last_result, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(destination_id) DO UPDATE SET last_result = excluded.last_result, updated_at = excluded.updated_at",
            params![id, result, now_rfc3339()],
        )?;
        Ok(())
    }

    pub fn list_schedules(&self) -> Result<Vec<Schedule>> {
        self.lock().query_map(
            "SELECT s.id, s.name, s.kind_json, s.destination_ids, s.content_json, s.enabled, r.last_run_at
             FROM report_schedule s LEFT JOIN schedule_run r ON r.schedule_id = s.id
             ORDER BY s.name",
            &[],
            |r| {
                Ok(Schedule {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    kind: serde_json::from_str(&r.get::<String>(2)?)?,
                    destination_ids: serde_json::from_str(&r.get::<String>(3)?)?,
                    content: serde_json::from_str(&r.get::<String>(4)?)?,
                    enabled: r.get_bool(5)?,
                    last_run_at: r.get(6)?,
                })
            },
        )
    }

    pub fn save_schedule(&self, s: &Schedule) -> Result<i64> {
        let kind = serde_json::to_string(&s.kind)?;
        let dests = serde_json::to_string(&s.destination_ids)?;
        let content = serde_json::to_string(&s.content)?;
        self.mutate(|tx| {
            if s.id == 0 {
                tx.execute(
                    "INSERT INTO report_schedule (name, kind_json, destination_ids, content_json, enabled, created_at)
                     VALUES (?, ?, ?, ?, ?, ?)",
                    params![&s.name, kind, dests, content, s.enabled, now_rfc3339()],
                )?;
                return tx.last_insert_rowid();
            }
            // 修改後以現在為基準重新計算（created_at = now、清除執行紀錄），避免立刻補送過去的報告。
            let n = tx.execute(
                "UPDATE report_schedule SET name = ?, kind_json = ?, destination_ids = ?, content_json = ?,
                    enabled = ?, created_at = ? WHERE id = ?",
                params![&s.name, kind, dests, content, s.enabled, now_rfc3339(), s.id],
            )?;
            if n == 0 {
                bail!("schedule not found");
            }
            tx.execute("DELETE FROM schedule_run WHERE schedule_id = ?", params![s.id])?;
            Ok(s.id)
        })
    }

    pub fn delete_schedule(&self, id: i64) -> Result<()> {
        self.mutate(|tx| {
            tx.execute(
                "DELETE FROM schedule_run WHERE schedule_id = ?",
                params![id],
            )?;
            tx.execute("DELETE FROM report_schedule WHERE id = ?", params![id])?;
            Ok(())
        })
    }

    pub fn schedule_created_at(&self, id: i64) -> Result<Option<String>> {
        self.lock().query_row(
            "SELECT created_at FROM report_schedule WHERE id = ?",
            params![id],
            |r| r.get(0),
        )
    }

    pub fn schedule_run(&self, id: i64) -> Result<Option<ScheduleRun>> {
        self.lock().query_row(
            "SELECT last_due_at, result FROM schedule_run WHERE schedule_id = ?",
            params![id],
            |r| {
                Ok(ScheduleRun {
                    last_due_at: r.get(0)?,
                    result: r.get(1)?,
                })
            },
        )
    }

    pub fn set_schedule_run(&self, id: i64, due_at: &str, result: &str) -> Result<()> {
        self.lock().execute(
            "INSERT INTO schedule_run (schedule_id, last_due_at, last_run_at, result) VALUES (?, ?, ?, ?)
             ON CONFLICT(schedule_id) DO UPDATE SET last_due_at = excluded.last_due_at,
                last_run_at = excluded.last_run_at, result = excluded.result",
            params![id, due_at, now_rfc3339(), result],
        )?;
        Ok(())
    }
}
