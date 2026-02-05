use libc::SIGRTMIN;
use signal_hook::consts::{SIGUSR1, SIGUSR2};
use signal_hook::iterator::Signals;
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use std::{env, fs, path::PathBuf};
use tao::event_loop::{ControlFlow, EventLoopBuilder};

mod config;
mod controller;
mod hyprland;
mod layer_overlay;

use crate::config::{Config, RunTarget};
use crate::controller::AppState;
use crate::hyprland::{
    focus_window_by_address, move_window_to_empty_workspace_by_address, resolve_app_window,
    spawn_hyprland_close_watcher, spawn_hyprland_watchdog_address,
};
use crate::layer_overlay::build_layer_overlay;

#[derive(Debug, Clone)]
pub enum AppEvent {
    FocusLost,
    HotkeyNotify,
    PinDigit(u8),
    PinBackspace,
    PinSubmit,
    PinCancel,
    Tick,
    AppClosed(String),
}

fn run_hyprctl_keyword(args: &[&str]) {
    match Command::new("hyprctl").args(args).status() {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("hyprctl {:?} exited with {status}", args),
        Err(err) => eprintln!("Failed to run hyprctl {:?}: {err}", args),
    }
}

fn bind_session_hotkey() {
    run_hyprctl_keyword(&[
        "keyword",
        "bind",
        "CTRL SHIFT, Q, exec, pkill -USR1 focuslock",
    ]);
}

fn unbind_session_hotkey() {
    run_hyprctl_keyword(&["keyword", "unbind", "CTRL SHIFT, Q"]);
}

fn signal_rtmin() -> Option<i32> {
    let value = SIGRTMIN();
    if value <= 0 {
        return None;
    }
    Some(value)
}

fn pin_signal_submit(rtmin: i32) -> i32 {
    rtmin + 10
}

fn pin_signal_backspace(rtmin: i32) -> i32 {
    rtmin + 11
}

fn bind_pin_submap() {
    run_hyprctl_keyword(&["keyword", "submap", "focuslock"]);
    for digit in 0..=9u8 {
        let bind = format!(" , {digit}, exec, pkill -SIGRTMIN+{digit} focuslock");
        run_hyprctl_keyword(&["keyword", "bind", &bind]);
    }
    let submit_bind = " , Return, exec, pkill -SIGRTMIN+10 focuslock";
    let submit_enter = " , Enter, exec, pkill -SIGRTMIN+10 focuslock";
    let cancel_bind = " , Escape, exec, pkill -USR2 focuslock";
    let backspace_bind = " , BackSpace, exec, pkill -SIGRTMIN+11 focuslock";
    let backspace_alt = " , Backspace, exec, pkill -SIGRTMIN+11 focuslock";
    run_hyprctl_keyword(&["keyword", "bind", submit_bind]);
    run_hyprctl_keyword(&["keyword", "bind", submit_enter]);
    run_hyprctl_keyword(&["keyword", "bind", cancel_bind]);
    run_hyprctl_keyword(&["keyword", "bind", backspace_bind]);
    run_hyprctl_keyword(&["keyword", "bind", backspace_alt]);
    run_hyprctl_keyword(&["keyword", "submap", "reset"]);
}

fn unbind_pin_submap() {
    run_hyprctl_keyword(&["keyword", "submap", "focuslock"]);
    for digit in 0..=9u8 {
        let bind = format!(" , {digit}");
        run_hyprctl_keyword(&["keyword", "unbind", &bind]);
    }
    run_hyprctl_keyword(&["keyword", "unbind", " , Return"]);
    run_hyprctl_keyword(&["keyword", "unbind", " , Enter"]);
    run_hyprctl_keyword(&["keyword", "unbind", " , Escape"]);
    run_hyprctl_keyword(&["keyword", "unbind", " , BackSpace"]);
    run_hyprctl_keyword(&["keyword", "unbind", " , Backspace"]);
    run_hyprctl_keyword(&["keyword", "submap", "reset"]);
}

fn activate_pin_submap() {
    match Command::new("hyprctl")
        .args(["dispatch", "submap", "focuslock"])
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("hyprctl dispatch submap focuslock exited with {status}"),
        Err(err) => eprintln!("Failed to dispatch submap focuslock: {err}"),
    }
}

fn reset_pin_submap() {
    match Command::new("hyprctl")
        .args(["dispatch", "submap", "reset"])
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("hyprctl dispatch submap reset exited with {status}"),
        Err(err) => eprintln!("Failed to dispatch submap reset: {err}"),
    }
}

fn focuslock_profile_dir() -> Result<PathBuf, String> {
    let base = if let Ok(value) = env::var("XDG_DATA_HOME") {
        PathBuf::from(value)
    } else if let Ok(home) = env::var("HOME") {
        PathBuf::from(home).join(".local").join("share")
    } else {
        return Err("Missing XDG_DATA_HOME or HOME for profile directory".to_string());
    };
    Ok(base.join("focuslock").join("profile"))
}

fn ensure_profile_dir() -> Result<PathBuf, String> {
    let dir = focuslock_profile_dir()?;
    fs::create_dir_all(&dir).map_err(|err| {
        format!(
            "Failed to create focuslock profile dir {}: {err}",
            dir.display()
        )
    })?;
    Ok(dir)
}

fn chromium_app_command(url: &str, profile_dir: &PathBuf) -> String {
    format!(
        "chromium --app=\"{url}\" --user-data-dir=\"{}\"",
        profile_dir.display()
    )
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
        RunTarget::Web {
            url,
            app_timeout_ms,
        } => {
            let profile_dir = match ensure_profile_dir() {
                Ok(dir) => dir,
                Err(err) => {
                    eprintln!("{err}");
                    std::process::exit(2);
                }
            };
            let app_cmd = chromium_app_command(url.as_str(), &profile_dir);
            run_app_session(total, escape_key, app_cmd, app_timeout_ms);
        }
        RunTarget::App {
            app_cmd,
            app_timeout_ms,
        } => {
            run_app_session(total, escape_key, app_cmd, app_timeout_ms);
        }
    };
}

fn run_app_session(
    total: Duration,
    escape_key: Option<String>,
    app_cmd: String,
    app_timeout_ms: u64,
) {
    gtk::init().expect("Failed to initialize GTK");
    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let mut state = AppState::new(total, escape_key, false);
    let overlay = match build_layer_overlay() {
        Ok(overlay) => overlay,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    };

    let launch_and_resolve = move |cmd: &str| {
        let child = std::process::Command::new("sh")
            .args(["-lc", &format!("exec {cmd}")])
            .spawn()
            .map_err(|err| format!("Failed to launch app command: {err}"))?;
        let pid = child.id();
        resolve_app_window(pid, app_timeout_ms)
    };

    let resolved = match launch_and_resolve(&app_cmd) {
        Ok(client) => client,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    };

    state.mark_loaded();

    move_window_to_empty_workspace_by_address(&resolved.address);

    let address = Arc::new(std::sync::Mutex::new(resolved.address));
    let done_flag = Arc::new(AtomicBool::new(false));
    let rtmin = match signal_rtmin() {
        Some(value) => value,
        None => {
            eprintln!("SIGRTMIN unavailable; PIN submap disabled");
            0
        }
    };
    let mut timer_done = false;
    if rtmin > 0 {
        unbind_pin_submap();
        bind_pin_submap();
    }
    bind_session_hotkey();
    let unbind_done = done_flag.clone();
    let unbind_rtmin = rtmin;
    std::thread::spawn(move || {
        while !unbind_done.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(200));
        }
        unbind_session_hotkey();
        if unbind_rtmin > 0 {
            unbind_pin_submap();
        }
    });
    spawn_hyprland_watchdog_address(proxy.clone(), done_flag.clone(), address.clone());
    let signal_done = done_flag.clone();
    let signal_proxy = proxy.clone();
    let mut signal_list = vec![SIGUSR1, SIGUSR2];
    if rtmin > 0 {
        for offset in 0..=11 {
            signal_list.push(rtmin + offset);
        }
    }
    match Signals::new(signal_list) {
        Ok(mut signals) => {
            let handle = signals.handle();
            let handle_done = done_flag.clone();
            std::thread::spawn(move || {
                while !handle_done.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(200));
                }
                handle.close();
            });
            std::thread::spawn(move || {
                for signal in signals.forever() {
                    if signal_done.load(Ordering::Relaxed) {
                        break;
                    }
                    if signal == SIGUSR1 {
                        let _ = signal_proxy.send_event(AppEvent::HotkeyNotify);
                    } else if signal == SIGUSR2 {
                        let _ = signal_proxy.send_event(AppEvent::PinCancel);
                    } else if rtmin > 0 {
                        let submit = pin_signal_submit(rtmin);
                        let backspace = pin_signal_backspace(rtmin);
                        if signal == submit {
                            let _ = signal_proxy.send_event(AppEvent::PinSubmit);
                        } else if signal == backspace {
                            let _ = signal_proxy.send_event(AppEvent::PinBackspace);
                        } else if signal >= rtmin && signal <= rtmin + 9 {
                            let digit = (signal - rtmin) as u8;
                            let _ = signal_proxy.send_event(AppEvent::PinDigit(digit));
                        }
                    }
                }
            });
        }
        Err(err) => eprintln!("Failed to register signal handler: {err}"),
    }
    let tick_done = done_flag.clone();
    let tick_proxy = proxy.clone();
    std::thread::spawn(move || {
        while !tick_done.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_secs(1));
            if tick_done.load(Ordering::Relaxed) {
                break;
            }
            let _ = tick_proxy.send_event(AppEvent::Tick);
        }
    });
    let focus_address = address.clone();
    let mut last_relaunch = Instant::now() - Duration::from_secs(1);
    let relaunch_backoff = Duration::from_millis(100);
    let close_done = done_flag.clone();
    spawn_hyprland_close_watcher(proxy.clone(), close_done, address.clone());

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        let response = state.handle_event(&event);
        if response.pin_mode_started && rtmin > 0 {
            activate_pin_submap();
        }
        if response.pin_mode_ended && rtmin > 0 {
            reset_pin_submap();
        }
        if response.set_done_flag {
            timer_done = true;
            if rtmin > 0 {
                reset_pin_submap();
            }
        }
        if let tao::event::Event::UserEvent(AppEvent::FocusLost) = event {
            if let Ok(guard) = focus_address.lock() {
                focus_window_by_address(&guard);
            }
        }
        if let Some(timer_text) = response.timer_text.as_deref() {
            overlay.set_timer_text(timer_text);
        }
        if let Some(control_flow_value) = response.control_flow {
            *control_flow = control_flow_value;
        }
        while gtk::events_pending() {
            let _ = gtk::main_iteration_do(false);
        }
        if response.set_done_flag {
            *control_flow = ControlFlow::Wait;
        }

        if !done_flag.load(Ordering::Relaxed) {
            if let tao::event::Event::UserEvent(AppEvent::AppClosed(_)) = event {
                if timer_done {
                    done_flag.store(true, Ordering::Relaxed);
                    overlay.hide();
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                if last_relaunch.elapsed() < relaunch_backoff {
                    return;
                }
                match launch_and_resolve(&app_cmd) {
                    Ok(client) => {
                        move_window_to_empty_workspace_by_address(&client.address);
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
    });
}
