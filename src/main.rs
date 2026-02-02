use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tao::event_loop::ControlFlow;

mod config;
mod controller;
mod hyprland;
mod overlay;
mod server;
mod webview;

use crate::config::Config;
use crate::controller::AppState;
use crate::hyprland::{move_window_to_empty_workspace, spawn_hyprland_watchdog};
use crate::server::{find_available_port, spawn_done_server};
use crate::webview::build_app_view;

#[derive(Debug, Clone)]
pub enum AppEvent {
    FocusLost,
    PageLoaded,
    EscapeOpen,
    EscapeCancel,
    EscapeSubmit(String),
    ExternalDone,
}

fn main() {
    gtk::init().expect("Failed to initialize GTK");
    let config = match Config::from_args() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    };
    let Config {
        mut url,
        total_seconds,
        escape_key,
    } = config;
    let done_port = find_available_port(9742);
    url.query_pairs_mut()
        .append_pair("focuslock_port", &done_port.to_string());
    let target_url = url.as_str().to_string();

    let app_view = build_app_view(&target_url);
    let event_loop = app_view.event_loop;
    let _window = app_view.window;
    let webview = app_view.webview;
    let proxy = app_view.proxy;

    move_window_to_empty_workspace(std::process::id());

    let done_flag = Arc::new(AtomicBool::new(false));
    spawn_hyprland_watchdog(proxy.clone(), done_flag.clone());
    spawn_done_server(proxy.clone(), done_flag.clone(), done_port);

    let total = Duration::from_secs(total_seconds);
    let mut state = AppState::new(total, escape_key);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(state.next_tick());
        let response = state.handle_event(&event);
        if response.set_done_flag {
            done_flag.store(true, Ordering::Relaxed);
        }
        for script in response.scripts {
            let _ = webview.evaluate_script(script.as_str());
        }
        if let Some(control_flow_value) = response.control_flow {
            *control_flow = control_flow_value;
        }
    });
}
