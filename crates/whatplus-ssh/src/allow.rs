//! IP 白名單。

use std::net::IpAddr;

use ipnet::IpNet;

/// 一組 CIDR。空白名單不允許任何位址（fail-closed）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IpAllowList {
    nets: Vec<IpNet>,
}

impl IpAllowList {
    /// 解析 CIDR 字串；單一位址（沒有 `/`）視為 /32 或 /128。
    pub fn parse<S: AsRef<str>>(items: &[S]) -> Result<Self, String> {
        let mut nets = Vec::with_capacity(items.len());
        for item in items {
            nets.push(parse_cidr(item.as_ref())?);
        }
        Ok(Self { nets })
    }

    pub fn is_empty(&self) -> bool {
        self.nets.is_empty()
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        let ip = normalize(ip);
        self.nets.iter().any(|net| net.contains(&ip))
    }
}

/// 檢查並正規化一個 CIDR 字串（例如 `192.168.1.5/16` → `192.168.0.0/16`）。
pub fn parse_cidr(raw: &str) -> Result<IpNet, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("empty address".into());
    }
    let net = if raw.contains('/') {
        raw.parse::<IpNet>()
            .map_err(|_| format!("invalid CIDR: {raw}"))?
    } else {
        let ip: IpAddr = raw.parse().map_err(|_| format!("invalid address: {raw}"))?;
        IpNet::from(ip)
    };
    Ok(net.trunc())
}

/// IPv4-mapped IPv6（`::ffff:192.168.1.2`）當成 IPv4 比對。
fn normalize(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(v6)),
        v4 => v4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_list_allows_nothing() {
        let list = IpAllowList::default();
        assert!(!list.contains("127.0.0.1".parse().unwrap()));
    }

    #[test]
    fn matches_cidr_and_mapped_v6() {
        let list = IpAllowList::parse(&["192.168.0.0/16"]).unwrap();
        assert!(list.contains("192.168.3.4".parse().unwrap()));
        assert!(list.contains("::ffff:192.168.3.4".parse().unwrap()));
        assert!(!list.contains("10.0.0.1".parse().unwrap()));
    }

    #[test]
    fn single_address_and_normalization() {
        assert_eq!(parse_cidr("10.1.2.3").unwrap().to_string(), "10.1.2.3/32");
        assert_eq!(
            parse_cidr("192.168.1.5/16").unwrap().to_string(),
            "192.168.0.0/16"
        );
        assert!(parse_cidr("192.168.0.0/40").is_err());
        assert!(parse_cidr("nope").is_err());
    }
}
