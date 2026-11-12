//! 報告期間與排程時間的計算（以伺服器時區解讀）。

use anyhow::Result;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{ToSpan, Zoned};
use whatplus_proto::{ManualRange, ScheduleKind};

/// 報告種類（決定標題）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportKind {
    Daily,
    Weekly,
    Monthly,
    Manual,
}

/// `[start, end)`。
#[derive(Debug, Clone, PartialEq)]
pub struct Period {
    pub kind: ReportKind,
    pub start: Zoned,
    pub end: Zoned,
}

impl Period {
    /// 給人看的期間，例如 `2026-11-12`、`2026-11-06 – 2026-11-12`、`2026-10`。
    pub fn label(&self) -> String {
        let first = self.start.date();
        let last = self
            .end
            .date()
            .yesterday()
            .unwrap_or(first)
            .max(first);
        if first == last {
            return first.to_string();
        }
        if first.day() == 1 && last.tomorrow().is_ok_and(|d| d.day() == 1) && first.month() == last.month() {
            return format!("{:04}-{:02}", first.year(), first.month());
        }
        format!("{first} – {last}")
    }
}

fn day_start(date: Date, tz: &TimeZone) -> Result<Zoned> {
    Ok(date.to_zoned(tz.clone())?)
}

fn first_of_month(date: Date) -> Date {
    date.first_of_month()
}

pub fn manual_period(range: ManualRange, now: &Zoned) -> Result<Period> {
    let tz = now.time_zone();
    let today = now.date();
    let tomorrow = today.tomorrow()?;
    let (start, end) = match range {
        ManualRange::Today => (today, tomorrow),
        ManualRange::Yesterday => (today.yesterday()?, today),
        ManualRange::Last7Days => (today.checked_sub(6.days())?, tomorrow),
        ManualRange::ThisMonth => (first_of_month(today), tomorrow),
        ManualRange::LastMonth => {
            let this = first_of_month(today);
            (first_of_month(this.yesterday()?), this)
        }
    };
    Ok(Period {
        kind: ReportKind::Manual,
        start: day_start(start, tz)?,
        end: day_start(end, tz)?,
    })
}

/// 排程在 `due` 觸發時要報告的期間。
pub fn scheduled_period(kind: ScheduleKind, due: &Zoned) -> Result<Period> {
    let tz = due.time_zone();
    let today = due.date();
    let (report_kind, start, end) = match kind {
        ScheduleKind::Daily { .. } => (ReportKind::Daily, today.yesterday()?, today),
        ScheduleKind::Weekly { .. } => (ReportKind::Weekly, today.checked_sub(7.days())?, today),
        ScheduleKind::Monthly { .. } => {
            let this = first_of_month(today);
            (ReportKind::Monthly, first_of_month(this.yesterday()?), this)
        }
    };
    Ok(Period {
        kind: report_kind,
        start: day_start(start, tz)?,
        end: day_start(end, tz)?,
    })
}

/// `now` 之前（含）最近一次應觸發的時間。
pub fn latest_due(kind: ScheduleKind, now: &Zoned) -> Result<Zoned> {
    let tz = now.time_zone().clone();
    let today = now.date();
    let at = |date: Date, hour: u8, minute: u8| -> Result<Zoned> {
        Ok(date.at(hour as i8, minute as i8, 0, 0).to_zoned(tz.clone())?)
    };
    Ok(match kind {
        ScheduleKind::Daily { hour, minute } => {
            let c = at(today, hour, minute)?;
            if c > *now { at(today.yesterday()?, hour, minute)? } else { c }
        }
        ScheduleKind::Weekly {
            weekday,
            hour,
            minute,
        } => {
            let current = today.weekday().to_monday_one_offset() as i64;
            let back = (current - weekday as i64).rem_euclid(7);
            let date = today.checked_sub(back.days())?;
            let c = at(date, hour, minute)?;
            if c > *now { at(date.checked_sub(7.days())?, hour, minute)? } else { c }
        }
        ScheduleKind::Monthly { hour, minute } => {
            let first = first_of_month(today);
            let c = at(first, hour, minute)?;
            if c > *now {
                at(first_of_month(first.yesterday()?), hour, minute)?
            } else {
                c
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn z(s: &str) -> Zoned {
        s.parse().unwrap()
    }

    #[test]
    fn daily_due_and_period() {
        let kind = ScheduleKind::Daily { hour: 9, minute: 0 };
        let now = z("2026-11-13T08:00:00+08:00[Asia/Taipei]");
        let due = latest_due(kind, &now).unwrap();
        assert_eq!(due.to_string(), "2026-11-12T09:00:00+08:00[Asia/Taipei]");
        let p = scheduled_period(kind, &due).unwrap();
        assert_eq!(p.label(), "2026-11-11");
    }

    #[test]
    fn weekly_due() {
        // 2026-11-13 是週五；週一 09:00 → 2026-11-09。
        let kind = ScheduleKind::Weekly { weekday: 1, hour: 9, minute: 0 };
        let now = z("2026-11-13T10:00:00+08:00[Asia/Taipei]");
        let due = latest_due(kind, &now).unwrap();
        assert_eq!(due.date().to_string(), "2026-11-09");
        let p = scheduled_period(kind, &due).unwrap();
        assert_eq!(p.label(), "2026-11-02 – 2026-11-08");
    }

    #[test]
    fn monthly_due_and_label() {
        let kind = ScheduleKind::Monthly { hour: 0, minute: 30 };
        let now = z("2026-11-01T00:10:00+08:00[Asia/Taipei]");
        let due = latest_due(kind, &now).unwrap();
        assert_eq!(due.date().to_string(), "2026-10-01");
        let p = scheduled_period(kind, &due).unwrap();
        assert_eq!(p.label(), "2026-09");
    }

    #[test]
    fn manual_ranges() {
        let now = z("2026-11-13T10:00:00+08:00[Asia/Taipei]");
        assert_eq!(manual_period(ManualRange::Today, &now).unwrap().label(), "2026-11-13");
        assert_eq!(manual_period(ManualRange::LastMonth, &now).unwrap().label(), "2026-10");
        assert_eq!(
            manual_period(ManualRange::Last7Days, &now).unwrap().label(),
            "2026-11-07 – 2026-11-13"
        );
    }
}
