use std::time::Duration;

mod config;
mod controller;
mod hyprland;
mod layer_overlay;
mod session;
mod target;
mod webview;

use crate::config::Config;

#[derive(Debug, Clone)]
pub enum AppEvent {
    FocusLost,
    PageLoaded,
    HotkeyNotify,
    PinDigit(u8),
    PinBackspace,
    PinSubmit,
    PinCancel,
    Tick,
    AppClosed(String),
}

fn main() {
    let config = match Config::from_args() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    };

    let total = Duration::from_secs(config.total_seconds);
    if let Err(err) = session::run_session(total, config.escape_key, config.target) {
        eprintln!("{err}");
        std::process::exit(2);
    }
}
