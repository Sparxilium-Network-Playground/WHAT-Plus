//! Antigravity 對話資料庫的本機快照。
//!
//! Antigravity 以 WAL 模式寫入 SQLite。來源端在容器中以唯讀方式掛載這些檔案時，
//! 容器內的 SQLite 無法與主機上的寫入端共用 `-shm`（Docker Desktop 跨作業系統），
//! 讀到的會是過期的內容。因此讀取前先把 `*.db` 與 `*.db-wal` 複製到可寫的暫存目錄，
//! 讓 SQLite 從 WAL 自行重建索引，再交給 ccusage 讀取快照。
//!
//! 只在設定 `WHATPLUS_ANTIGRAVITY_SNAPSHOT`（Docker image 預設開啟）且使用者沒有自行設定
//! `ANTIGRAVITY_DATA_DIR` 時啟用。只複製大小或修改時間有變的檔案。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

/// 與 what-sources 的預設 Antigravity 根目錄相同（相對於 HOME）。
const ROOTS: [&str; 6] = [
    ".gemini/antigravity",
    ".gemini/antigravity-cli",
    ".gemini/antigravity-ide",
    ".gemini/antigravity-backup",
    ".gemini/antigravity-acp",
    ".config/antigravity",
];

type Stamp = (u64, Option<SystemTime>);

struct Plan {
    /// (原始 conversations 目錄, 快照 conversations 目錄)
    pairs: Vec<(PathBuf, PathBuf)>,
    /// 上次複製時來源檔案的（大小, 修改時間）。
    copied: Mutex<HashMap<PathBuf, Stamp>>,
}

static PLAN: OnceLock<Plan> = OnceLock::new();

/// 在啟動任何執行緒之前呼叫：建立快照目錄並把 `ANTIGRAVITY_DATA_DIR` 指向它。
pub fn init(mode: &str) {
    let Some(base) = std::env::var_os("WHATPLUS_ANTIGRAVITY_SNAPSHOT") else {
        return;
    };
    if std::env::var_os("ANTIGRAVITY_DATA_DIR").is_some() {
        return;
    }
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return;
    };
    let base = PathBuf::from(base).join(mode);
    let mut pairs = Vec::new();
    for root in ROOTS {
        let source = root
            .split('/')
            .fold(home.clone(), |p, part| p.join(part))
            .join("conversations");
        let target = base.join(root.replace('/', "_")).join("conversations");
        if fs::create_dir_all(&target).is_ok() {
            pairs.push((source, target));
        }
    }
    let list: Vec<String> = pairs
        .iter()
        .filter_map(|(_, t)| t.parent().map(|p| p.display().to_string()))
        .collect();
    // SAFETY: 在 main 一開始、建立 tokio runtime 與任何執行緒之前呼叫。
    unsafe { std::env::set_var("ANTIGRAVITY_DATA_DIR", list.join(",")) };
    let _ = PLAN.set(Plan {
        pairs,
        copied: Mutex::new(HashMap::new()),
    });
}

/// 更新快照。沒有啟用時不做事。回傳無法複製的檔案數。
pub fn refresh() -> usize {
    let Some(plan) = PLAN.get() else {
        return 0;
    };
    let mut copied = plan.copied.lock().unwrap_or_else(|e| e.into_inner());
    let mut failures = 0;
    for (source, target) in &plan.pairs {
        failures += sync_dir(source, target, &mut copied);
    }
    failures
}

fn stamp(path: &Path) -> Option<Stamp> {
    let meta = fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()))
}

fn is_copied_file(name: &str) -> bool {
    name.ends_with(".db") || name.ends_with(".db-wal")
}

/// 讓 `target` 與 `source` 中的 `*.db`／`*.db-wal` 一致。
fn sync_dir(source: &Path, target: &Path, copied: &mut HashMap<PathBuf, Stamp>) -> usize {
    let wanted: Vec<(String, PathBuf)> = fs::read_dir(source)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
                .filter(|(name, _)| is_copied_file(name))
                .collect()
        })
        .unwrap_or_default();

    // 來源已經沒有的檔案（例如 WAL 已 checkpoint 刪除）也從快照移除。
    if let Ok(entries) = fs::read_dir(target) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            let keep = is_copied_file(&name) && wanted.iter().any(|(n, _)| *n == name);
            if !keep && !name.ends_with(".db-shm") {
                let _ = fs::remove_file(entry.path());
                copied.remove(&source.join(&name));
            }
        }
    }

    let mut failures = 0;
    for (name, path) in &wanted {
        let Some(now) = stamp(path) else {
            continue;
        };
        if copied.get(path) == Some(&now) {
            continue;
        }
        let dest = target.join(name);
        // 內容變了：舊的 -shm 對新的 WAL 無效，一併刪除讓 SQLite 重建。
        let db_name = name.trim_end_matches("-wal");
        let _ = fs::remove_file(target.join(format!("{db_name}-shm")));
        match copy_with_mtime(path, &dest, now.1) {
            Ok(()) => {
                copied.insert(path.clone(), now);
            }
            Err(e) => {
                failures += 1;
                copied.remove(path);
                tracing::debug!("cannot copy {}: {e}", path.display());
            }
        }
    }
    failures
}

/// 複製並保留修改時間（ccusage 在資料庫沒有時間戳時以檔案時間代替）。
fn copy_with_mtime(from: &Path, to: &Path, mtime: Option<SystemTime>) -> std::io::Result<()> {
    let tmp = to.with_extension("partial");
    fs::copy(from, &tmp)?;
    if let Some(t) = mtime {
        fs::File::options()
            .write(true)
            .open(&tmp)?
            .set_modified(t)?;
    }
    fs::rename(&tmp, to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_copies_changed_and_removes_stale() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let mut copied = HashMap::new();
        fs::write(src.path().join("a.db"), b"one").unwrap();
        fs::write(src.path().join("a.db-wal"), b"wal").unwrap();
        fs::write(src.path().join("note.txt"), b"skip").unwrap();

        assert_eq!(sync_dir(src.path(), dst.path(), &mut copied), 0);
        assert_eq!(fs::read(dst.path().join("a.db")).unwrap(), b"one");
        assert!(dst.path().join("a.db-wal").exists());
        assert!(!dst.path().join("note.txt").exists());

        // WAL 被 checkpoint 後消失、資料庫內容改變。
        fs::write(dst.path().join("a.db-shm"), b"old shm").unwrap();
        fs::remove_file(src.path().join("a.db-wal")).unwrap();
        fs::write(src.path().join("a.db"), b"two!").unwrap();
        assert_eq!(sync_dir(src.path(), dst.path(), &mut copied), 0);
        assert_eq!(fs::read(dst.path().join("a.db")).unwrap(), b"two!");
        assert!(!dst.path().join("a.db-wal").exists());
        assert!(!dst.path().join("a.db-shm").exists());
    }
}
