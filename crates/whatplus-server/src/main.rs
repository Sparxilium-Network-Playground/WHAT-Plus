//! WHAT Plus 伺服器。
//!
//! - `whatplus-server run`：daemon（SSH 服務、排程、Discord）
//! - `whatplus-server admin`：Admin TUI（公鑰、白名單、狀態），與 daemon 共用資料庫
//! - `whatplus-server setup ...`：給安裝精靈呼叫的非互動指令（見 `setup_cli`）

mod api;
mod app;
mod auth;
mod config;
mod daemon;
mod db;
mod delivery;
mod discord;
mod hostkey;
mod report;
mod sanitize;
mod scheduler;
mod setup_cli;
mod ssh;
mod tui;
mod validate;

use std::process::ExitCode;

use config::Settings;

fn main() -> ExitCode {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "run".into());
    let path = Settings::config_path();
    let settings = match Settings::load(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Configuration error: {e:#}");
            eprintln!(
                "Action: fix {} (run the setup script to create it).",
                path.display()
            );
            return ExitCode::from(2);
        }
    };
    match mode.as_str() {
        "run" => run_daemon(settings),
        "admin" => match tui::run(settings) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("Error: {e:#}");
                ExitCode::FAILURE
            }
        },
        "setup" => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            setup_cli::run(settings, &args)
        }
        other => {
            eprintln!("Unknown mode {other:?}. Usage: whatplus-server [run|admin|setup]");
            ExitCode::from(2)
        }
    }
}

fn run_daemon(settings: Settings) -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,russh=warn".into()),
        )
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
        .init();
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Error: cannot start runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(daemon::run(settings)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("APPLICATION FAILED TO START: {e:#}");
            ExitCode::FAILURE
        }
    }
}
