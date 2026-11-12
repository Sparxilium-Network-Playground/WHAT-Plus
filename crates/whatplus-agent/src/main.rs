//! WHAT Plus 來源端。
//!
//! - `whatplus-agent run`：依間隔讀取本機用量並上傳
//! - `whatplus-agent admin`：設定 TUI（伺服器位址、私鑰、驗證伺服器指紋、間隔、立即上傳）

mod collect;
mod config;
mod daemon;
mod tui;
mod upload;

use std::process::ExitCode;

use config::Paths;

fn main() -> ExitCode {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "run".into());
    let paths = Paths::from_env();
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Error: cannot start runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    match mode.as_str() {
        "run" => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "info,russh=warn".into()),
                )
                .init();
            runtime.block_on(daemon::run(paths));
            ExitCode::SUCCESS
        }
        "admin" => match tui::run(&runtime, paths) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("Error: {e:#}");
                ExitCode::FAILURE
            }
        },
        other => {
            eprintln!("Unknown mode {other:?}. Usage: whatplus-agent [run|admin]");
            ExitCode::from(2)
        }
    }
}
