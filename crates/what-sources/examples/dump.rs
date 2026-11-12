//! 開發用：讀取本機用量並列出每日彙總（UTC），用於和 ccusage CLI 對照。
//! `cargo run -p what-sources --example dump`

use jiff::{civil::Weekday, tz::TimeZone};
use what_core::report::{Granularity, summarize_periods};
use what_pricing::PricingTable;

fn main() -> anyhow::Result<()> {
    let pricing = PricingTable::embedded();
    let tz = TimeZone::system();
    for source in what_sources::all_sources() {
        let started = std::time::Instant::now();
        let loaded = source.load()?;
        let records = loaded.price(&pricing);
        println!(
            "== {} dirs={:?} raw={} records={} ({:?})",
            source.source(),
            source.data_dirs(),
            loaded.len(),
            records.len(),
            started.elapsed()
        );
        for day in summarize_periods(&records, Granularity::Daily, &tz, Weekday::Monday) {
            println!(
                "{}  tokens={:>12}  cost=${:>9.4}  models={}",
                day.start,
                day.totals.tokens.total(),
                day.totals.cost_usd,
                day.models
                    .iter()
                    .map(|m| m.model.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
    }
    Ok(())
}
