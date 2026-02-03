use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tao::event::StartCause;
use tao::event_loop::{ControlFlow, EventLoopBuilder};

mod config;
mod controller;
mod hyprland;
mod overlay;
mod overlay_window;
mod server;
mod webview;

use crate::config::{Config, RunTarget};
use crate::controller::AppState;
use crate::hyprland::{
    active_monitor_geometry, client_exists_by_address, move_window_to_empty_workspace,
    move_window_to_empty_workspace_by_address, resolve_app_window, spawn_hyprland_watchdog,
    spawn_hyprland_watchdog_address,
};
use crate::overlay_window::build_overlay_window;
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
    let config = match Config::from_args() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    };
    let Config {
        target,
        total_seconds,
        escape_key,
    } = config;

    let total = Duration::from_secs(total_seconds);

    match target {
        RunTarget::Web { mut url } => {
            gtk::init().expect("Failed to initialize GTK");
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
        RunTarget::App {
            app_cmd,
            app_class,
            app_title,
            app_timeout_ms,
        } => {
            gtk::init().expect("Failed to initialize GTK");
            let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
            let proxy = event_loop.create_proxy();
            let mut state = AppState::new(total, escape_key);
            let overlay = build_overlay_window(&event_loop);

            let app_class = app_class.clone();
            let app_title = app_title.clone();
            let app_timeout_ms = app_timeout_ms;
            let launch_and_resolve = move |cmd: &str| {
                let child = std::process::Command::new("sh")
                    .args(["-lc", &format!("exec {cmd}")])
                    .spawn()
                    .map_err(|err| format!("Failed to launch app command: {err}"))?;
                let pid = child.id();
                resolve_app_window(
                    pid,
                    app_class.as_deref(),
                    app_title.as_deref(),
                    app_timeout_ms,
                )
            };

            let resolved = match launch_and_resolve(&app_cmd) {
                Ok(client) => client,
                Err(err) => {
                    eprintln!("{err}");
                    std::process::exit(2);
                }
            };

            move_window_to_empty_workspace_by_address(&resolved.address);
            if let Some(geometry) = active_monitor_geometry() {
                overlay.set_position(geometry);
            }

            let address = Arc::new(std::sync::Mutex::new(resolved.address));
            let done_flag = Arc::new(AtomicBool::new(false));
            spawn_hyprland_watchdog_address(proxy.clone(), done_flag.clone(), address.clone());
            let _ = proxy.send_event(AppEvent::PageLoaded);
            let mut last_relaunch = Instant::now() - Duration::from_secs(1);
            let relaunch_backoff = Duration::from_millis(400);

            event_loop.run(move |event, _, control_flow| {
                *control_flow = ControlFlow::WaitUntil(state.next_tick());
                let response = state.handle_event(&event);
                if response.set_done_flag {
                    done_flag.store(true, Ordering::Relaxed);
                }
                if let Some(timer_text) = response.timer_text.as_deref() {
                    overlay.set_timer_text(timer_text);
                }
                if let Some(control_flow_value) = response.control_flow {
                    *control_flow = control_flow_value;
                }
                if response.set_done_flag {
                    *control_flow = ControlFlow::Exit;
                }

                if !done_flag.load(Ordering::Relaxed) {
                    if matches!(
                        event,
                        tao::event::Event::NewEvents(StartCause::ResumeTimeReached { .. })
                    ) {
                        let current_address = match address.lock() {
                            Ok(guard) => guard.clone(),
                            Err(_) => return,
                        };
                        if !client_exists_by_address(&current_address)
                            && last_relaunch.elapsed() >= relaunch_backoff
                        {
                            match launch_and_resolve(&app_cmd) {
                                Ok(client) => {
                                    move_window_to_empty_workspace_by_address(&client.address);
                                    if let Some(geometry) = active_monitor_geometry() {
                                        overlay.set_position(geometry);
                                    }
                                    if let Ok(mut guard) = address.lock() {
                                        *guard = client.address;
                                    }
                                    last_relaunch = Instant::now();
                                }
                                Err(err) => {
                                    eprintln!("{err}");
                                    done_flag.store(true, Ordering::Relaxed);
                                    overlay.hide();
                                    *control_flow = ControlFlow::Exit;
                                }
                            }
                        }
                    }
                }
            });
        }
    };
}
