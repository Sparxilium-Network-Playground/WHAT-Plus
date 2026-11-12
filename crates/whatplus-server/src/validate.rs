//! 管理客戶端送來的資料檢查。錯誤訊息直接回給客戶端，只描述哪個欄位不對。

use whatplus_proto::{Destination, Schedule};
use whatplus_ssh::allow::parse_cidr;

use crate::discord::rest::valid_snowflake;

pub const MAX_NAME: usize = 64;

pub fn name(field: &str, value: &str) -> Result<String, String> {
    let v = value.trim();
    if v.is_empty() {
        return Err(format!("{field} is required"));
    }
    if v.chars().count() > MAX_NAME {
        return Err(format!("{field} is longer than {MAX_NAME} characters"));
    }
    if v.chars().any(char::is_control) {
        return Err(format!("{field} contains control characters"));
    }
    Ok(v.to_string())
}

/// 檢查並正規化 CIDR 清單。
pub fn cidrs(items: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for item in items.iter().filter(|s| !s.trim().is_empty()) {
        let net = parse_cidr(item)?.to_string();
        if !out.contains(&net) {
            out.push(net);
        }
    }
    Ok(out)
}

pub fn destination(d: &Destination) -> Result<Destination, String> {
    if !valid_snowflake(&d.guild_id) || !valid_snowflake(&d.channel_id) {
        return Err("invalid Discord server or channel id".into());
    }
    let mut d = d.clone();
    d.name = name("name", &d.name)?;
    d.guild_name = d.guild_name.chars().take(100).collect();
    d.channel_name = d.channel_name.chars().take(100).collect();
    d.last_result = None;
    Ok(d)
}

pub fn schedule(s: &Schedule, known_destinations: &[i64]) -> Result<Schedule, String> {
    let mut s = s.clone();
    s.name = name("name", &s.name)?;
    s.kind.validate()?;
    if s.destination_ids.is_empty() {
        return Err("select at least one destination".into());
    }
    if let Some(id) = s.destination_ids.iter().find(|id| !known_destinations.contains(id)) {
        return Err(format!("destination {id} does not exist"));
    }
    s.destination_ids.sort_unstable();
    s.destination_ids.dedup();
    s.last_run_at = None;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_cidrs() {
        assert!(name("name", "  ").is_err());
        assert!(name("name", "a\nb").is_err());
        assert_eq!(name("name", " pc ").unwrap(), "pc");
        assert_eq!(
            cidrs(&["192.168.1.1/16".into(), "".into(), "192.168.0.0/16".into()]).unwrap(),
            vec!["192.168.0.0/16"]
        );
        assert!(cidrs(&["x".into()]).is_err());
    }
}
