//! 以 ccusage（MIT，Copyright (c) 2025 ryoppippi）實作的資料來源。
//!
//! 安全性：ccusage 的價格查詢一律以 `offline = true`、`CostMode::Display` 載入，
//! 且本程式從不安裝 HTTP fetcher，因此讀取用量時不會連網。

use std::{
    collections::{BTreeSet, HashMap},
    env, fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use ccusage_adapter_codex::{CodexServiceTier, CodexSpeedPolicy, CodexTokenUsageEvent};
use ccusage_core::{
    LoadedEntry, PricingMap, TokenUsageRaw,
    cli::{AgentReportKind, CodexSpeed, CostMode, SharedArgs},
    model_aliases::resolve_model_name,
};
use jiff::Timestamp;
use serde::Deserialize;
use what_core::{Source, TokenCounts, UsageRecord};
use what_pricing::PricingTable;

use crate::{LoadedUsage, UsageSource};

fn shared_args() -> SharedArgs {
    let mut shared = SharedArgs::with_defaults();
    // json = true 讓 ccusage 不在 stderr 顯示進度；offline 確保不連網；
    // Display 模式讓 adapter 不自行計價，改由我們用目前的價格表計價。
    shared.json = true;
    shared.offline = true;
    shared.mode = CostMode::Display;
    // ccusage 預設每個 CPU 核心同時把一整個日誌檔讀進記憶體；單一檔案可能超過 100 MB，
    // 在有記憶體上限的容器中會被 OOM kill。設定此變數改為一次讀一個檔案。
    shared.single_thread = std::env::var_os("WHAT_SOURCES_SINGLE_THREAD").is_some();
    shared
}

fn home_dir() -> Option<PathBuf> {
    ccusage_core::home::home_dir()
}

fn timestamp_from_millis(millis: i64) -> Option<Timestamp> {
    Timestamp::from_millisecond(millis).ok()
}

// ───────────────────────────── Claude Code ─────────────────────────────

/// Claude Code（`~/.claude/projects`、`~/.config/claude/projects` 或 `CLAUDE_CONFIG_DIR`）。
pub struct ClaudeCodeSource;

impl UsageSource for ClaudeCodeSource {
    fn source(&self) -> Source {
        Source::ClaudeCode
    }

    fn data_dirs(&self) -> Vec<PathBuf> {
        // 與 ccusage 的 claude_paths 相同規則（該函式未公開）。
        let mut out = Vec::new();
        if let Ok(raw) = env::var("CLAUDE_CONFIG_DIR") {
            for part in raw.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                let path = PathBuf::from(part);
                let projects = if path.ends_with("projects") {
                    path
                } else {
                    path.join("projects")
                };
                if projects.is_dir() && !out.contains(&projects) {
                    out.push(projects);
                }
            }
            return out;
        }
        let Some(home) = home_dir() else {
            return out;
        };
        let xdg = env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home.join(".config"));
        for base in [xdg.join("claude"), home.join(".claude")] {
            let projects = base.join("projects");
            if projects.is_dir() && !out.contains(&projects) {
                out.push(projects);
            }
        }
        out
    }

    fn load(&self) -> Result<Box<dyn LoadedUsage>> {
        what_pricing::install_embedded_snapshots();
        let entries = ccusage_adapter_claude::load_entries(&shared_args(), None)
            .map_err(|e| anyhow::anyhow!("{e}"))
            .context("讀取 Claude Code 用量失敗")?;
        let projects = claude_project_names(&self.data_dirs());
        Ok(Box::new(ClaudeLoaded { entries, projects }))
    }
}

struct ClaudeLoaded {
    entries: Vec<LoadedEntry>,
    /// 專案目錄名稱（如 `C--code-GitStatus`）→ 顯示名稱（取自日誌的 `cwd`）。
    projects: HashMap<String, String>,
}

impl LoadedUsage for ClaudeLoaded {
    fn price(&self, pricing: &PricingTable) -> Vec<UsageRecord> {
        let map = pricing.ccusage_map();
        let aliases = HashMap::new();
        self.entries
            .iter()
            .filter_map(|entry| {
                let mut record = claude_record(entry, map, &aliases)?;
                if let Some(name) = self.projects.get(entry.project.as_ref()) {
                    record.project = name.clone();
                }
                Some(record)
            })
            .collect()
    }

    fn len(&self) -> usize {
        self.entries.len()
    }
}

fn claude_record(
    entry: &LoadedEntry,
    map: &PricingMap,
    aliases: &HashMap<String, String>,
) -> Option<UsageRecord> {
    let data = &entry.data;
    let usage = data.message.usage;
    let pricing_model = data.message.model.as_deref();
    let cost = ccusage_core::calculate_cost_for_usage_at(
        pricing_model,
        usage,
        data.cost_usd,
        Some(entry.timestamp),
        CostMode::Auto,
        Some(map),
    );
    let pricing_missing = ccusage_core::missing_pricing_model_for_usage(
        pricing_model,
        usage,
        data.cost_usd,
        CostMode::Auto,
        Some(map),
    )
    .is_some();
    let model = entry
        .model
        .as_deref()
        .map(|m| resolve_model_name(m).into_owned())
        .unwrap_or_default();
    Some(UsageRecord {
        source: Source::ClaudeCode,
        timestamp: timestamp_from_millis(entry.timestamp.as_millis())?,
        model,
        project: ccusage_core::format_project_name(&entry.project, aliases),
        session_id: data
            .session_id
            .clone()
            .unwrap_or_else(|| entry.session_id.to_string()),
        tokens: TokenCounts {
            input: usage.input_tokens,
            output: usage.output_tokens,
            cache_creation: usage.cache_creation_token_count(),
            cache_read: usage.cache_read_input_tokens,
            reasoning: 0,
        },
        cost_usd: cost,
        pricing_missing,
    })
}

// ───────────────────────────── Codex ─────────────────────────────

/// OpenAI Codex（`~/.codex/sessions`、`~/.codex/archived_sessions` 或 `CODEX_HOME`）。
pub struct CodexSource;

fn codex_homes() -> Vec<PathBuf> {
    if let Ok(raw) = env::var("CODEX_HOME") {
        return raw
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .collect();
    }
    home_dir()
        .map(|h| vec![h.join(".codex")])
        .unwrap_or_default()
}

impl UsageSource for CodexSource {
    fn source(&self) -> Source {
        Source::Codex
    }

    fn data_dirs(&self) -> Vec<PathBuf> {
        codex_homes()
            .into_iter()
            .flat_map(|home| [home.join("sessions"), home.join("archived_sessions")])
            .filter(|dir| dir.is_dir())
            .collect()
    }

    fn load(&self) -> Result<Box<dyn LoadedUsage>> {
        what_pricing::install_embedded_snapshots();
        let (events, _detected) =
            ccusage_adapter_codex::load_codex_events_with_detection(&shared_args())
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("讀取 Codex 用量失敗")?;
        let projects = codex_session_projects(&self.data_dirs());
        // 讀取 Codex 設定中的預設 service tier（只讀本機設定檔）。
        let speed = ccusage_adapter_codex::resolve_codex_speed(CodexSpeed::Auto);
        Ok(Box::new(CodexLoaded {
            events: dedupe_codex_events(events),
            projects,
            speed,
        }))
    }
}

struct CodexLoaded {
    events: Vec<CodexTokenUsageEvent>,
    /// session id → 專案（工作目錄名稱）。
    projects: HashMap<String, String>,
    speed: CodexSpeedPolicy,
}

impl LoadedUsage for CodexLoaded {
    fn price(&self, pricing: &PricingTable) -> Vec<UsageRecord> {
        let map = pricing.ccusage_map();
        self.events
            .iter()
            .filter_map(|event| codex_record(event, map, self.speed, &self.projects))
            .collect()
    }

    fn len(&self) -> usize {
        self.events.len()
    }
}

/// 與 ccusage `aggregate_events` 相同的前處理：沒有模型的事件略過；
/// 同一個 compaction response 只算一次，並合併各副本記錄的 service tier。
fn dedupe_codex_events(events: Vec<CodexTokenUsageEvent>) -> Vec<CodexTokenUsageEvent> {
    let has_model = |e: &CodexTokenUsageEvent| e.model.as_deref().is_some_and(|m| !m.is_empty());
    let mut tiers: HashMap<String, Option<CodexServiceTier>> = HashMap::new();
    for event in events.iter().filter(|e| has_model(e)) {
        if let Some(id) = &event.response_id {
            let tier = tiers.entry(id.clone()).or_insert(None);
            *tier = merge_service_tiers(*tier, event.service_tier);
        }
    }
    let mut seen = BTreeSet::new();
    events
        .into_iter()
        .filter(|e| has_model(e))
        .filter_map(|mut event| {
            if let Some(id) = event.response_id.clone() {
                if !seen.insert(id.clone()) {
                    return None;
                }
                event.service_tier = tiers.get(&id).copied().flatten();
            }
            Some(event)
        })
        .collect()
}

/// 移植自 ccusage `merge_codex_service_tiers`（未公開）：明確記錄優先於未分類；
/// 副本互相衝突時取 Standard，讓費用估計保守且與檔案順序無關。
fn merge_service_tiers(
    current: Option<CodexServiceTier>,
    incoming: Option<CodexServiceTier>,
) -> Option<CodexServiceTier> {
    use CodexServiceTier::*;
    match (current, incoming) {
        (Some(Standard), _) | (_, Some(Standard)) => Some(Standard),
        (Some(Ultrafast), _) | (_, Some(Ultrafast)) => Some(Ultrafast),
        (Some(Fast), _) | (_, Some(Fast)) => Some(Fast),
        (Some(Flex), _) | (_, Some(Flex)) => Some(Flex),
        (None, None) => None,
    }
}

fn codex_record(
    event: &CodexTokenUsageEvent,
    map: &PricingMap,
    speed: CodexSpeedPolicy,
    projects: &HashMap<String, String>,
) -> Option<UsageRecord> {
    let raw_model = event.model.as_deref().filter(|m| !m.is_empty())?;
    let timestamp: Timestamp = event.timestamp.parse().ok()?;
    // 單一事件建成一個 group，交給 ccusage 計價，以沿用其長上下文分段與 service tier 規則。
    let groups = ccusage_adapter_codex::aggregate_events(
        std::slice::from_ref(event),
        AgentReportKind::Daily,
        Some("UTC"),
    )
    .ok()?;
    let group = groups.into_values().next()?;
    let cost = ccusage_adapter_codex::calculate_group_cost(&group, map, speed);
    let pricing_missing = group.models.iter().any(|(model, usage)| {
        ccusage_adapter_codex::codex_model_missing_pricing(model, usage, map)
    });
    let model = resolve_model_name(raw_model).into_owned();
    let input = ccusage_adapter_codex::non_cached_input_tokens(
        event.input_tokens,
        event.cached_input_tokens,
        event.cache_creation_tokens,
    );
    Some(UsageRecord {
        source: Source::Codex,
        timestamp,
        model,
        project: projects
            .get(&event.session_id)
            .cloned()
            .unwrap_or_else(|| "Unknown Project".to_string()),
        session_id: event.session_id.clone(),
        tokens: TokenCounts {
            input,
            output: event.output_tokens,
            cache_creation: event.cache_creation_tokens,
            cache_read: event.cached_input_tokens,
            reasoning: event.reasoning_output_tokens,
        },
        cost_usd: cost,
        pricing_missing,
    })
}

// ───────────────────────────── Antigravity ─────────────────────────────

/// Google Antigravity（`~/.gemini/antigravity*/conversations/*.db` 或 `ANTIGRAVITY_DATA_DIR`）。
/// SQLite 以唯讀方式開啟；跨目錄（例如 `antigravity` 與 `antigravity-backup`）的重複對話由 ccusage 依訊息識別去重。
pub struct AntigravitySource;

/// 與 ccusage `paths.rs` 相同的預設根目錄（該常數未公開）。
const ANTIGRAVITY_ROOTS: [&str; 6] = [
    ".gemini/antigravity",
    ".gemini/antigravity-cli",
    ".gemini/antigravity-ide",
    ".gemini/antigravity-backup",
    ".gemini/antigravity-acp",
    ".config/antigravity",
];

impl UsageSource for AntigravitySource {
    fn source(&self) -> Source {
        Source::Antigravity
    }

    fn data_dirs(&self) -> Vec<PathBuf> {
        if let Ok(raw) = env::var("ANTIGRAVITY_DATA_DIR") {
            return raw
                .split(',')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(PathBuf::from)
                .map(|root| {
                    let nested = root.join("conversations");
                    if nested.is_dir() { nested } else { root }
                })
                .filter(|dir| dir.is_dir())
                .collect();
        }
        let Some(home) = home_dir() else {
            return Vec::new();
        };
        ANTIGRAVITY_ROOTS
            .iter()
            .map(|root| {
                root.split('/')
                    .fold(home.clone(), |path, part| path.join(part))
                    .join("conversations")
            })
            .filter(|dir| dir.is_dir())
            .collect()
    }

    fn load(&self) -> Result<Box<dyn LoadedUsage>> {
        what_pricing::install_embedded_snapshots();
        // Display 模式不計價；價格表在 price() 時才套用，換價格表不必重讀資料庫。
        let empty = PricingMap::default();
        let (entries, _warnings) =
            ccusage_adapter_antigravity::load_entries_with_warnings(&shared_args(), &empty)
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("讀取 Antigravity 用量失敗")?;
        Ok(Box::new(AntigravityLoaded { entries }))
    }
}

struct AntigravityLoaded {
    entries: Vec<LoadedEntry>,
}

impl LoadedUsage for AntigravityLoaded {
    fn price(&self, pricing: &PricingTable) -> Vec<UsageRecord> {
        let map = pricing.ccusage_map();
        self.entries
            .iter()
            .filter_map(|entry| antigravity_record(entry, map))
            .collect()
    }

    fn len(&self) -> usize {
        self.entries.len()
    }
}

fn antigravity_record(entry: &LoadedEntry, map: &PricingMap) -> Option<UsageRecord> {
    let usage = entry.data.message.usage;
    let model = entry.model.clone().unwrap_or_default();
    // ccusage 把思考 token 放在 extra_total_tokens，計價時併入 output。
    let reasoning = entry.extra_total_tokens;
    let cost_usage = TokenUsageRaw {
        output_tokens: usage.output_tokens.saturating_add(reasoning),
        cache_creation: None,
        ..usage
    };
    let (cost, pricing_missing) = antigravity_cost(&model, cost_usage, entry.timestamp, map);
    Some(UsageRecord {
        source: Source::Antigravity,
        timestamp: timestamp_from_millis(entry.timestamp.as_millis())?,
        model: resolve_model_name(&model).into_owned(),
        project: "Antigravity".to_string(),
        session_id: entry.session_id.to_string(),
        tokens: TokenCounts {
            input: usage.input_tokens,
            output: cost_usage.output_tokens,
            cache_creation: usage.cache_creation_input_tokens,
            cache_read: usage.cache_read_input_tokens,
            reasoning,
        },
        cost_usd: cost,
        pricing_missing,
    })
}

/// 移植自 ccusage `calculate_antigravity_cost` / `missing_antigravity_pricing`（未公開）。
/// 差異：`LoadedEntry` 不保留 API provider，改以模型名稱是否為 `gemini-*` 判斷是否嘗試 Google 前綴。
/// 由於裸模型名稱永遠先試，只有在找不到時才會用到前綴。
fn antigravity_cost(
    model: &str,
    usage: TokenUsageRaw,
    timestamp: ccusage_core::TimestampMs,
    map: &PricingMap,
) -> (f64, bool) {
    let total = usage
        .input_tokens
        .saturating_add(usage.output_tokens)
        .saturating_add(usage.cache_creation_token_count())
        .saturating_add(usage.cache_read_input_tokens);
    let candidates = antigravity_model_candidates(model);
    let cost = if gemini_flash_pricing_alias(model).is_some() {
        candidates
            .iter()
            .find_map(|c| find_exact_pricing_with_alias(map, c))
            .map(|p| ccusage_core::calculate_cost_from_pricing(usage, p))
    } else {
        candidates.iter().find_map(|c| {
            map.find(c).map(|_| {
                ccusage_core::calculate_cost_for_usage_at(
                    Some(c),
                    usage,
                    None,
                    Some(timestamp),
                    CostMode::Calculate,
                    Some(map),
                )
            })
        })
    };
    (cost.unwrap_or(0.0), total > 0 && cost.is_none())
}

const GOOGLE_PRICING_PREFIXES: [&str; 4] = ["google", "gemini", "vertex_ai", "openrouter/google"];

fn gemini_flash_pricing_alias(model: &str) -> Option<&'static str> {
    match model {
        "gemini-3.8-flash-high" | "gemini-3.8-flash-medium" | "gemini-3.8-flash-low" => {
            Some("gemini-3.8-flash")
        }
        "gemini-3.7-flash-high" | "gemini-3.7-flash-medium" | "gemini-3.7-flash-low" => {
            Some("gemini-3.7-flash")
        }
        "gemini-3.6-flash-high" | "gemini-3.6-flash-medium" | "gemini-3.6-flash-low" => {
            Some("gemini-3.6-flash")
        }
        _ => None,
    }
}

fn antigravity_model_candidates(model: &str) -> Vec<String> {
    let mut models = vec![model];
    models.extend(gemini_flash_pricing_alias(model));
    let google = model.starts_with("gemini");
    let mut out = Vec::new();
    for m in models {
        out.push(m.to_string());
        if google {
            out.extend(GOOGLE_PRICING_PREFIXES.iter().map(|p| format!("{p}/{m}")));
        }
    }
    let mut seen = BTreeSet::new();
    out.retain(|c| seen.insert(c.clone()));
    out
}

fn find_exact_pricing_with_alias(map: &PricingMap, model: &str) -> Option<ccusage_core::Pricing> {
    map.find_exact_with_fallback(model).or_else(|| {
        let alias = resolve_model_name(model);
        (alias.as_ref() != model)
            .then(|| map.find_exact_with_fallback(alias.as_ref()))
            .flatten()
    })
}

// ───────────────────────────── 專案名稱 ─────────────────────────────
//
// ccusage 的 Claude 專案名稱來自編碼後的目錄名（例如 `C--code-GitStatus`），
// 在 Windows 上無法可靠還原；Codex 事件則不帶工作目錄。
// 因此兩者都改為讀日誌開頭幾行的 `cwd` 欄位，取最後一段路徑作為專案名稱。

/// 每個檔案最多讀這麼多行，避免掃描整個大檔。
const CWD_SCAN_LINES: usize = 50;

#[derive(Deserialize)]
struct CwdLine {
    #[serde(rename = "type")]
    kind: Option<String>,
    cwd: Option<String>,
    payload: Option<CwdPayload>,
}

#[derive(Deserialize)]
struct CwdPayload {
    cwd: Option<String>,
}

/// 開啟 JSONL（`.zst` 自動解壓縮），逐行讀取。
fn open_lines(path: &Path) -> Option<Box<dyn Iterator<Item = String>>> {
    let file = fs::File::open(path).ok()?;
    let is_zst = path.extension().is_some_and(|e| e == "zst");
    let reader: Box<dyn BufRead> = if is_zst {
        Box::new(BufReader::new(zstd::stream::read::Decoder::new(file).ok()?))
    } else {
        Box::new(BufReader::new(file))
    };
    Some(Box::new(reader.lines().map_while(Result::ok)))
}

/// 在檔案開頭找第一個 `cwd`：Claude 在頂層，Codex 在 `session_meta.payload`。
fn first_cwd(path: &Path) -> Option<String> {
    for line in open_lines(path)?.take(CWD_SCAN_LINES) {
        if !line.contains("\"cwd\"") {
            continue;
        }
        let Ok(parsed) = serde_json::from_str::<CwdLine>(&line) else {
            continue;
        };
        let cwd = parsed.cwd.or_else(|| {
            (parsed.kind.as_deref() == Some("session_meta"))
                .then(|| parsed.payload.and_then(|p| p.cwd))
                .flatten()
        });
        if let Some(cwd) = cwd.filter(|c| !c.trim().is_empty()) {
            return Some(project_name_from_path(&cwd));
        }
    }
    None
}

fn is_jsonl(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.ends_with(".jsonl") || name.ends_with(".jsonl.zst")
}

/// 遞迴列出目錄下的 JSONL 檔（排序，讓結果穩定）。
fn jsonl_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(read) = fs::read_dir(&current) else {
            continue;
        };
        for item in read.flatten() {
            let path = item.path();
            if path.is_dir() {
                stack.push(path);
            } else if is_jsonl(&path) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Claude：`projects/<目錄名>` → 專案名稱。每個專案目錄找到第一個含 `cwd` 的檔案即停止。
fn claude_project_names(project_roots: &[PathBuf]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for root in project_roots {
        let Ok(read) = fs::read_dir(root) else {
            continue;
        };
        for item in read.flatten() {
            let dir = item.path();
            let Some(name) = dir.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
                continue;
            };
            if !dir.is_dir() || out.contains_key(&name) {
                continue;
            }
            if let Some(project) = jsonl_files(&dir).iter().find_map(|f| first_cwd(f)) {
                out.insert(name, project);
            }
        }
    }
    out
}

/// Codex：session id → 專案名稱。
/// session id 規則與 ccusage 相同：相對於 sessions 目錄、去掉 `.zst` 與 `.jsonl`、以 `/` 連接。
fn codex_session_projects(dirs: &[PathBuf]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for dir in dirs {
        for path in jsonl_files(dir) {
            if let Some(project) = first_cwd(&path) {
                out.insert(codex_session_id(dir, &path), project);
            }
        }
    }
    out
}

fn codex_session_id(dir: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(dir).unwrap_or(path);
    let relative = if relative.extension().is_some_and(|e| e == "zst") {
        relative.with_extension("")
    } else {
        relative.to_path_buf()
    };
    relative
        .with_extension("")
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect::<Vec<_>>()
        .join("/")
}

/// 取路徑最後一段作為專案名稱（同時處理 `\` 與 `/`）。
fn project_name_from_path(path: &str) -> String {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_name_handles_both_separators() {
        assert_eq!(
            project_name_from_path(r"C:\code\SmartPluginAssistant"),
            "SmartPluginAssistant"
        );
        assert_eq!(project_name_from_path("/home/a/proj/"), "proj");
    }

    #[test]
    fn antigravity_candidates_follow_ccusage_order() {
        assert_eq!(
            antigravity_model_candidates("gemini-3.7-flash-medium"),
            [
                "gemini-3.7-flash-medium",
                "google/gemini-3.7-flash-medium",
                "gemini/gemini-3.7-flash-medium",
                "vertex_ai/gemini-3.7-flash-medium",
                "openrouter/google/gemini-3.7-flash-medium",
                "gemini-3.7-flash",
                "google/gemini-3.7-flash",
                "gemini/gemini-3.7-flash",
                "vertex_ai/gemini-3.7-flash",
                "openrouter/google/gemini-3.7-flash",
            ]
        );
        assert_eq!(
            antigravity_model_candidates("claude-sonnet-4-6"),
            ["claude-sonnet-4-6"]
        );
    }

    #[test]
    fn codex_project_map_reads_session_meta() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("2026/10/23");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            nested.join("rollout-x.jsonl"),
            r#"{"timestamp":"2026-10-22T20:27:25.948Z","type":"session_meta","payload":{"cwd":"C:\\code\\Demo"}}
{"type":"event_msg"}"#,
        )
        .unwrap();
        let compressed = zstd::encode_all(
            &br#"{"type":"session_meta","payload":{"cwd":"/home/a/Zipped"}}"#[..],
            3,
        )
        .unwrap();
        fs::write(nested.join("rollout-z.jsonl.zst"), compressed).unwrap();
        let map = codex_session_projects(&[dir.path().to_path_buf()]);
        assert_eq!(
            map.get("2026/10/23/rollout-x").map(String::as_str),
            Some("Demo")
        );
        assert_eq!(
            map.get("2026/10/23/rollout-z").map(String::as_str),
            Some("Zipped")
        );
    }

    #[test]
    fn claude_project_names_use_cwd() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("C--code-What-Have-I");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("s.jsonl"),
            concat!(
                r#"{"type":"summary"}"#,
                "\n",
                r#"{"cwd":"C:\\code\\What-Have-I","type":"user"}"#,
                "\n"
            ),
        )
        .unwrap();
        let map = claude_project_names(&[root.path().to_path_buf()]);
        assert_eq!(
            map.get("C--code-What-Have-I").map(String::as_str),
            Some("What-Have-I")
        );
    }
}
