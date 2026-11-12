//! WHAT Plus 管理客戶端（Windows）。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backend;
mod convert;
mod ctx;
mod i18n;
mod pages;
mod settings;

use std::sync::{Arc, Mutex};

use slint::ComponentHandle;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let store = Arc::new(settings::SettingsStore::new());
    let settings = Arc::new(Mutex::new(store.load()));
    let backend = match backend::Backend::new() {
        Ok(b) => Arc::new(b),
        Err(e) => {
            rfd::MessageDialog::new()
                .set_title("WHAT Plus")
                .set_description(format!("Cannot start: {e:#}"))
                .set_level(rfd::MessageLevel::Error)
                .show();
            return Ok(());
        }
    };

    let ui = AppWindow::new()?;
    let ctx = ctx::Ctx {
        cache: Default::default(),
        ui: ui.as_weak(),
        backend,
        settings,
        store,
    };
    pages::init(&ctx, &ui);
    ui.run()
}
