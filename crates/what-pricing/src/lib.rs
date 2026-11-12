//! 模型價格表。
//!
//! - 預設只用建置時內嵌的 LiteLLM 快照，**完全不連網**。
//! - 使用者在設定中按下「更新價格」時才呼叫 [`fetch_update`]，以 HTTPS 下載 LiteLLM 價格表，
//!   驗證格式與內容後才寫入本機快取；之後啟動時會載入快取覆蓋內嵌值。
//! - 底層使用 ccusage 的 `PricingMap`。ccusage-core 本身不含 HTTP client，
//!   且本程式從不呼叫 `set_json_fetcher`，因此 ccusage 不可能自行連網。

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use ccusage_core::PricingMap;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

/// LiteLLM 官方價格表。只在使用者手動更新時存取。
pub const LITELLM_PRICING_URL: &str =
    "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";

/// 下載大小上限，避免異常回應耗盡記憶體（目前檔案約 4 MB）。
const MAX_DOWNLOAD_BYTES: u64 = 32 * 1024 * 1024;
/// 有效價格表至少要能解析出這麼多模型。
const MIN_VALID_MODELS: usize = 200;
/// 驗證時必須找得到價格的代表性模型。
const REQUIRED_MODELS: &[&str] = &["claude-sonnet-4-5", "gpt-5"];

const CACHE_FILE: &str = "litellm-pricing.json";
const META_FILE: &str = "litellm-pricing.meta.json";

/// 價格表來源。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PricingOrigin {
    /// 建置時內嵌的快照。
    Embedded,
    /// 使用者手動更新後的快取。
    Updated {
        fetched_at: Timestamp,
        models: usize,
    },
}

/// 已載入的價格表。
pub struct PricingTable {
    map: PricingMap,
    origin: PricingOrigin,
}

impl std::fmt::Debug for PricingTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PricingTable")
            .field("origin", &self.origin)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheMeta {
    fetched_at: Timestamp,
    models: usize,
    source_url: String,
}

/// 把建置時內嵌的價格快照交給 ccusage。可重複呼叫，只有第一次生效。
/// 任何會用到 ccusage 價格資料的程式碼執行前都必須先呼叫。
pub fn install_embedded_snapshots() {
    ccusage_core::pricing::set_embedded_snapshots(ccusage_core::pricing::EmbeddedSnapshots {
        litellm_pricing: ccusage_pricing_snapshots::LITELLM_PRICING,
        models_dev_pricing: ccusage_pricing_snapshots::MODELS_DEV_PRICING,
        models_dev_catalog_rules: ccusage_pricing_snapshots::MODELS_DEV_CATALOG_RULES,
    });
}

impl PricingTable {
    /// 只使用內嵌快照。
    pub fn embedded() -> Self {
        install_embedded_snapshots();
        Self {
            map: PricingMap::load_embedded(),
            origin: PricingOrigin::Embedded,
        }
    }

    /// 載入內嵌快照，若 `cache_dir` 內有有效的更新快取則套用。
    /// 快取損毀時忽略並回退到內嵌快照，不會讓程式無法啟動。
    pub fn load(cache_dir: &Path) -> Self {
        let mut table = Self::embedded();
        let (Ok(json), Ok(meta)) = (
            fs::read_to_string(cache_dir.join(CACHE_FILE)),
            fs::read_to_string(cache_dir.join(META_FILE)),
        ) else {
            return table;
        };
        let Ok(meta) = serde_json::from_str::<CacheMeta>(&meta) else {
            return table;
        };
        let loaded = table.map.load_json(&json);
        if loaded >= MIN_VALID_MODELS {
            table.origin = PricingOrigin::Updated {
                fetched_at: meta.fetched_at,
                models: loaded,
            };
        } else {
            // 部分載入的結果不可信，重新從內嵌快照開始。
            table = Self::embedded();
        }
        table
    }

    pub fn origin(&self) -> &PricingOrigin {
        &self.origin
    }

    /// 是否有此模型的價格。
    pub fn has_model(&self, model: &str) -> bool {
        self.map.find(model).is_some()
    }

    /// 給資料來源轉接層使用的 ccusage 價格表。其他模組不應依賴此方法。
    #[doc(hidden)]
    pub fn ccusage_map(&self) -> &PricingMap {
        &self.map
    }
}

/// 驗證下載內容：必須是 JSON 物件、可解析出足夠的模型、且包含代表性模型。
/// 回傳可載入的模型數。
pub fn validate_pricing_json(json: &str) -> Result<usize> {
    let value: serde_json::Value = serde_json::from_str(json).context("價格表不是有效的 JSON")?;
    if !value.is_object() {
        bail!("價格表格式錯誤：最外層不是 JSON 物件");
    }
    let mut map = PricingMap::default();
    let loaded = map.load_json(json);
    if loaded < MIN_VALID_MODELS {
        bail!("價格表只包含 {loaded} 個可用模型，低於下限 {MIN_VALID_MODELS}");
    }
    for model in REQUIRED_MODELS {
        if map.find(model).is_none() {
            bail!("價格表缺少必要模型 {model}");
        }
    }
    Ok(loaded)
}

/// 以 HTTPS 下載最新價格表，驗證後寫入 `cache_dir`。只應由使用者操作觸發。
pub fn fetch_update(cache_dir: &Path) -> Result<PricingOrigin> {
    let json = download(LITELLM_PRICING_URL)?;
    store_update(cache_dir, &json, LITELLM_PRICING_URL, Timestamp::now())
}

/// 驗證並寫入快取（拆出來方便測試）。先寫暫存檔再改名，避免寫到一半損毀。
pub fn store_update(
    cache_dir: &Path,
    json: &str,
    source_url: &str,
    fetched_at: Timestamp,
) -> Result<PricingOrigin> {
    let models = validate_pricing_json(json)?;
    fs::create_dir_all(cache_dir)
        .with_context(|| format!("無法建立快取目錄 {}", cache_dir.display()))?;
    let meta = CacheMeta {
        fetched_at,
        models,
        source_url: source_url.to_string(),
    };
    write_atomic(&cache_dir.join(CACHE_FILE), json.as_bytes())?;
    write_atomic(
        &cache_dir.join(META_FILE),
        serde_json::to_string_pretty(&meta)?.as_bytes(),
    )?;
    Ok(PricingOrigin::Updated { fetched_at, models })
}

/// 刪除更新快取，回到內嵌快照。
pub fn clear_cache(cache_dir: &Path) -> Result<()> {
    for name in [CACHE_FILE, META_FILE] {
        let path = cache_dir.join(name);
        if path.exists() {
            fs::remove_file(&path).with_context(|| format!("無法刪除 {}", path.display()))?;
        }
    }
    Ok(())
}

fn download(url: &str) -> Result<String> {
    if !url.starts_with("https://") {
        bail!("只允許 HTTPS 下載價格表");
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .https_only(true)
        .build()
        .into();
    let mut response = agent.get(url).call().context("下載價格表失敗")?;
    let mut body = String::new();
    response
        .body_mut()
        .as_reader()
        .take(MAX_DOWNLOAD_BYTES + 1)
        .read_to_string(&mut body)
        .context("讀取價格表內容失敗")?;
    if body.len() as u64 > MAX_DOWNLOAD_BYTES {
        bail!("價格表超過 {MAX_DOWNLOAD_BYTES} bytes 上限");
    }
    Ok(body)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp: PathBuf = path.with_extension("tmp");
    {
        let mut file =
            fs::File::create(&tmp).with_context(|| format!("無法寫入 {}", tmp.display()))?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path).with_context(|| format!("無法取代 {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SNAPSHOT: &str = include_str!("../../../vendor/pricing/litellm-pricing.json");

    #[test]
    fn embedded_snapshot_prices_known_models() {
        let table = PricingTable::embedded();
        assert_eq!(table.origin(), &PricingOrigin::Embedded);
        assert!(table.has_model("claude-sonnet-4-5"));
        assert!(table.has_model("gpt-5"));
    }

    #[test]
    fn rejects_invalid_payloads() {
        assert!(validate_pricing_json("not json").is_err());
        assert!(validate_pricing_json("[]").is_err());
        assert!(
            validate_pricing_json(r#"{"x":{"input_cost_per_token":1,"output_cost_per_token":1}}"#)
                .is_err()
        );
    }

    #[test]
    fn stores_and_reloads_cache() {
        let dir = tempfile::tempdir().unwrap();
        let fetched_at: Timestamp = "2026-11-12T00:00:00Z".parse().unwrap();
        let origin =
            store_update(dir.path(), SNAPSHOT, "https://example.test", fetched_at).unwrap();
        assert!(matches!(origin, PricingOrigin::Updated { .. }));
        let table = PricingTable::load(dir.path());
        assert!(
            matches!(table.origin(), PricingOrigin::Updated { fetched_at: t, .. } if *t == fetched_at)
        );
        clear_cache(dir.path()).unwrap();
        assert_eq!(
            PricingTable::load(dir.path()).origin(),
            &PricingOrigin::Embedded
        );
    }

    #[test]
    fn corrupt_cache_falls_back_to_embedded() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(CACHE_FILE), "{}").unwrap();
        fs::write(
            dir.path().join(META_FILE),
            r#"{"fetched_at":"2026-11-12T00:00:00Z","models":1,"source_url":"x"}"#,
        )
        .unwrap();
        assert_eq!(
            PricingTable::load(dir.path()).origin(),
            &PricingOrigin::Embedded
        );
    }
}
