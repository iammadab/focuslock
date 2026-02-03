use libc::SIGRTMIN;
use signal_hook::consts::{SIGUSR1, SIGUSR2};
use signal_hook::iterator::Signals;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tao::event_loop::{ControlFlow, EventLoopBuilder};

use crate::config::RunTarget;
use crate::controller::AppState;
use crate::hyprland::{
    find_client_by_pid, focus_window_by_address, list_hyprland_clients,
    move_window_to_empty_workspace_by_address, resolve_window_by_pid, spawn_hyprland_close_watcher,
    spawn_hyprland_watchdog_address,
};
use crate::layer_overlay::build_layer_overlay;
use crate::target::{build_webview_target, spawn_app_cmd};
use crate::AppEvent;

const DEFAULT_WINDOW_TIMEOUT_MS: u64 = 4000;

pub fn run_session(
    total: Duration,
    escape_key: Option<String>,
    target: RunTarget,
) -> Result<(), String> {
    gtk::init().map_err(|err| format!("Failed to initialize GTK: {err}"))?;

    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let overlay = build_layer_overlay()?;

    let (pid, _webview_handle, reset_on_focus_loss, app_timeout_ms, app_cmd) = match target {
        RunTarget::Web { url } => {
            let target_url = url.as_str().to_string();
            let (handle, pid) = build_webview_target(&event_loop, &proxy, &target_url)?;
            (pid, Some(handle), true, DEFAULT_WINDOW_TIMEOUT_MS, None)
        }
        RunTarget::App {
            app_cmd,
            app_timeout_ms,
        } => {
            let pid = spawn_app_cmd(&app_cmd)?;
            (pid, None, false, app_timeout_ms, Some(app_cmd))
        }
    };

    let resolved = if _webview_handle.is_some() {
        resolve_window_by_pid_with_gtk(pid, app_timeout_ms)?
    } else {
        resolve_window_by_pid(pid, app_timeout_ms)?
    };
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
    spawn_hyprland_close_watcher(proxy.clone(), done_flag.clone(), address.clone());

    if app_cmd.is_some() {
        let _ = proxy.send_event(AppEvent::PageLoaded);
    }

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

    let mut state = AppState::new(total, escape_key, reset_on_focus_loss);
    let mut last_relaunch = Instant::now() - Duration::from_secs(1);
    let relaunch_backoff = Duration::from_millis(100);

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
            done_flag.store(true, Ordering::Relaxed);
            overlay.hide();
            if rtmin > 0 {
                reset_pin_submap();
            }
            *control_flow = ControlFlow::Exit;
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
        if let tao::event::Event::UserEvent(AppEvent::FocusLost) = event {
            if let Ok(guard) = address.lock() {
                focus_window_by_address(&guard);
            }
        }

        if let (Some(cmd), tao::event::Event::UserEvent(AppEvent::AppClosed(_))) =
            (app_cmd.as_deref(), event)
        {
            if done_flag.load(Ordering::Relaxed) {
                return;
            }
            if last_relaunch.elapsed() < relaunch_backoff {
                return;
            }
            match spawn_app_cmd(cmd).and_then(|pid| resolve_window_by_pid(pid, app_timeout_ms)) {
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
    });
}

fn resolve_window_by_pid_with_gtk(
    pid: u32,
    timeout_ms: u64,
) -> Result<crate::hyprland::ClientInfo, String> {
    let start = Instant::now();
    let timeout = Duration::from_millis(timeout_ms);
    loop {
        if let Some(client) = find_client_by_pid(pid) {
            return Ok(client);
        }
        while gtk::events_pending() {
            let _ = gtk::main_iteration_do(false);
        }
        if start.elapsed() >= timeout {
            let last_clients = list_hyprland_clients();
            let mut message = String::from("no app window matched the launched process PID");
            if !last_clients.is_empty() {
                message.push_str("; recent clients: ");
                let summaries: Vec<String> = last_clients
                    .iter()
                    .take(6)
                    .map(|client| format!("pid={} address={}", client.pid, client.address))
                    .collect();
                message.push_str(&summaries.join(" | "));
            }
            return Err(message);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn run_hyprctl_keyword(args: &[&str]) {
    match std::process::Command::new("hyprctl").args(args).status() {
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
    match std::process::Command::new("hyprctl")
        .args(["dispatch", "submap", "focuslock"])
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("hyprctl dispatch submap focuslock exited with {status}"),
        Err(err) => eprintln!("Failed to dispatch submap focuslock: {err}"),
    }
}

fn reset_pin_submap() {
    match std::process::Command::new("hyprctl")
        .args(["dispatch", "submap", "reset"])
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("hyprctl dispatch submap reset exited with {status}"),
        Err(err) => eprintln!("Failed to dispatch submap reset: {err}"),
    }
}
