//! 協定型別 ↔ Slint 結構。

use slint::{ModelRc, SharedString, VecModel};
use whatplus_proto::{Destination, Language, ManualRange, ReportContent, ScheduleKind};

use crate::{CheckItem, Content};

pub fn content_to_ui(c: ReportContent) -> Content {
    Content {
        totals: c.totals,
        by_tool: c.by_tool,
        by_model: c.by_model,
        by_machine: c.by_machine,
        top_projects: c.top_projects,
        tokens: c.tokens,
    }
}

pub fn content_from_ui(c: &Content) -> ReportContent {
    ReportContent {
        totals: c.totals,
        by_tool: c.by_tool,
        by_model: c.by_model,
        by_machine: c.by_machine,
        top_projects: c.top_projects,
        tokens: c.tokens,
    }
}

pub fn range_from_index(i: i32) -> ManualRange {
    match i {
        0 => ManualRange::Today,
        2 => ManualRange::Last7Days,
        3 => ManualRange::ThisMonth,
        4 => ManualRange::LastMonth,
        _ => ManualRange::Yesterday,
    }
}

/// 下拉選單順序：0 = English、1 = 繁體中文。
pub fn language_from_index(i: i32) -> Language {
    if i == 1 { Language::ZhTw } else { Language::En }
}

pub fn destination_label(d: &Destination) -> String {
    format!("{}  ({} / #{})", d.name, d.guild_name, d.channel_name)
}

/// 目的地勾選清單；保留 `checked` 中已勾選的 id。
pub fn destination_checks(dests: &[Destination], checked: &[i64]) -> ModelRc<CheckItem> {
    let items: Vec<CheckItem> = dests
        .iter()
        .map(|d| CheckItem {
            id: d.id as i32,
            label: SharedString::from(destination_label(d)),
            checked: checked.contains(&d.id),
        })
        .collect();
    ModelRc::new(VecModel::from(items))
}

pub fn checked_ids(model: &ModelRc<CheckItem>) -> Vec<i64> {
    use slint::Model;
    model
        .iter()
        .filter(|c| c.checked)
        .map(|c| c.id as i64)
        .collect()
}

const WEEKDAYS_EN: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const WEEKDAYS_ZH: [&str; 7] = ["一", "二", "三", "四", "五", "六", "日"];

pub fn schedule_summary(kind: ScheduleKind, zh: bool) -> String {
    match kind {
        ScheduleKind::Daily { hour, minute } => {
            if zh {
                format!("每天 {hour:02}:{minute:02}")
            } else {
                format!("Daily at {hour:02}:{minute:02}")
            }
        }
        ScheduleKind::Weekly {
            weekday,
            hour,
            minute,
        } => {
            let i = (weekday.clamp(1, 7) - 1) as usize;
            if zh {
                format!("每週{} {hour:02}:{minute:02}", WEEKDAYS_ZH[i])
            } else {
                format!("Weekly on {} at {hour:02}:{minute:02}", WEEKDAYS_EN[i])
            }
        }
        ScheduleKind::Monthly { hour, minute } => {
            if zh {
                format!("每月 1 日 {hour:02}:{minute:02}")
            } else {
                format!("Monthly on the 1st at {hour:02}:{minute:02}")
            }
        }
    }
}

/// 表單欄位 → ScheduleKind。
pub fn schedule_kind(kind_index: i32, weekday_index: i32, hour: i32, minute: i32) -> ScheduleKind {
    let hour = hour.clamp(0, 23) as u8;
    let minute = minute.clamp(0, 59) as u8;
    match kind_index {
        1 => ScheduleKind::Weekly {
            weekday: (weekday_index.clamp(0, 6) + 1) as u8,
            hour,
            minute,
        },
        2 => ScheduleKind::Monthly { hour, minute },
        _ => ScheduleKind::Daily { hour, minute },
    }
}

/// ScheduleKind → (kind_index, weekday_index, hour, minute)。
pub fn schedule_fields(kind: ScheduleKind) -> (i32, i32, i32, i32) {
    match kind {
        ScheduleKind::Daily { hour, minute } => (0, 0, hour as i32, minute as i32),
        ScheduleKind::Weekly {
            weekday,
            hour,
            minute,
        } => (1, weekday as i32 - 1, hour as i32, minute as i32),
        ScheduleKind::Monthly { hour, minute } => (2, 0, hour as i32, minute as i32),
    }
}

pub fn strings(items: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    let v: Vec<SharedString> = items.into_iter().map(SharedString::from).collect();
    ModelRc::new(VecModel::from(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_round_trip() {
        for kind in [
            ScheduleKind::Daily { hour: 9, minute: 5 },
            ScheduleKind::Weekly {
                weekday: 7,
                hour: 23,
                minute: 59,
            },
            ScheduleKind::Monthly { hour: 0, minute: 0 },
        ] {
            let (k, w, h, m) = schedule_fields(kind);
            assert_eq!(schedule_kind(k, w, h, m), kind);
        }
        assert_eq!(
            schedule_summary(
                ScheduleKind::Weekly {
                    weekday: 1,
                    hour: 9,
                    minute: 0
                },
                true
            ),
            "每週一 09:00"
        );
    }
}
