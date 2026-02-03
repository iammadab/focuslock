use std::time::{Duration, Instant};
use std::{
    io::{BufRead, BufReader},
    os::unix::net::UnixStream,
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
};
use tao::event_loop::EventLoopProxy;

use crate::AppEvent;

#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub pid: u32,
    pub address: String,
}

fn hyprland_socket_path() -> Option<PathBuf> {
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").ok()?;
    Some(
        PathBuf::from(runtime_dir)
            .join("hypr")
            .join(signature)
            .join(".socket2.sock"),
    )
}

pub fn list_hyprland_clients() -> Vec<ClientInfo> {
    let output = Command::new("hyprctl").args(["-j", "clients"]).output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let value: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };
    let clients = match value.as_array() {
        Some(clients) => clients,
        None => return Vec::new(),
    };

    let mut results = Vec::new();
    for client in clients {
        let pid = client.get("pid").and_then(|pid| pid.as_u64()).unwrap_or(0) as u32;
        let address = client
            .get("address")
            .and_then(|addr| addr.as_str())
            .unwrap_or("")
            .to_string();
        if pid == 0 || address.is_empty() {
            continue;
        }

        results.push(ClientInfo { pid, address });
    }

    results
}

pub fn find_client_by_pid(pid: u32) -> Option<ClientInfo> {
    list_hyprland_clients()
        .into_iter()
        .find(|client| client.pid == pid)
}

fn format_client_summary(client: &ClientInfo) -> String {
    format!("pid={} address={}", client.pid, client.address)
}

pub fn resolve_window_by_pid(pid: u32, timeout_ms: u64) -> Result<ClientInfo, String> {
    let start = Instant::now();
    let timeout = Duration::from_millis(timeout_ms);
    loop {
        if let Some(client) = find_client_by_pid(pid) {
            return Ok(client);
        }

        if start.elapsed() >= timeout {
            let last_clients = list_hyprland_clients();
            let mut message = String::from("no app window matched the launched process PID");
            if !last_clients.is_empty() {
                message.push_str("; recent clients: ");
                let summaries: Vec<String> = last_clients
                    .iter()
                    .take(6)
                    .map(format_client_summary)
                    .collect();
                message.push_str(&summaries.join(" | "));
            }
            return Err(message);
        }

        thread::sleep(Duration::from_millis(100));
    }
}

fn hyprland_active_monitor_name() -> Option<String> {
    let output = Command::new("hyprctl")
        .args(["-j", "monitors"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let monitors = value.as_array()?;
    for monitor in monitors {
        let focused = monitor.get("focused")?.as_bool()?;
        if focused {
            return monitor
                .get("name")
                .and_then(|name| name.as_str())
                .map(|name| name.to_string());
        }
    }
    None
}

fn hyprland_used_workspace_ids() -> Option<Vec<i64>> {
    let output = Command::new("hyprctl")
        .args(["-j", "workspaces"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let workspaces = value.as_array()?;
    let mut ids = Vec::new();
    for workspace in workspaces {
        if let Some(id) = workspace.get("id").and_then(|id| id.as_i64()) {
            ids.push(id);
        }
    }
    Some(ids)
}

fn next_empty_workspace_id(min_id: i64) -> Option<i64> {
    let used = hyprland_used_workspace_ids()?;
    for id in min_id..(min_id + 200) {
        if !used.contains(&id) {
            return Some(id);
        }
    }
    None
}

pub fn move_window_to_empty_workspace_by_address(address: &str) {
    if let Some(monitor_name) = hyprland_active_monitor_name() {
        let _ = Command::new("hyprctl")
            .args(["dispatch", "focusmonitor", &monitor_name])
            .status();
    }

    let workspace_id = match next_empty_workspace_id(2) {
        Some(id) => id,
        None => return,
    };

    let _ = Command::new("hyprctl")
        .args(["dispatch", "workspace", &workspace_id.to_string()])
        .status();

    let target = format!("{workspace_id},address:{address}");
    let moved = Command::new("hyprctl")
        .args(["dispatch", "movetoworkspacesilent", &target])
        .status()
        .map(|status| status.success())
        .unwrap_or(false);

    if !moved {
        let _ = Command::new("hyprctl")
            .args(["dispatch", "focuswindow", &format!("address:{address}")])
            .status();
        let _ = Command::new("hyprctl")
            .args([
                "dispatch",
                "movetoworkspacesilent",
                &workspace_id.to_string(),
            ])
            .status();
    }

    let _ = Command::new("hyprctl")
        .args(["dispatch", "workspace", &workspace_id.to_string()])
        .status();
    let _ = Command::new("hyprctl")
        .args(["dispatch", "focuswindow", &format!("address:{address}")])
        .status();
}

pub fn spawn_hyprland_watchdog_address(
    proxy: EventLoopProxy<AppEvent>,
    done: Arc<AtomicBool>,
    address: Arc<std::sync::Mutex<String>>,
) {
    thread::spawn(move || {
        let socket_path = match hyprland_socket_path() {
            Some(path) => path,
            None => return,
        };
        let stream = match UnixStream::connect(socket_path) {
            Ok(stream) => stream,
            Err(_) => return,
        };

        let mut last_refocus = Instant::now() - Duration::from_secs(5);
        let reader = BufReader::new(stream);

        for line in reader.lines().flatten() {
            if done.load(Ordering::Relaxed) {
                break;
            }
            let line = line.trim();
            if !line.starts_with("activewindowv2>>") {
                continue;
            }

            let payload = &line["activewindowv2>>".len()..];
            let active_addr = payload.split(',').next().unwrap_or("");
            if active_addr.is_empty() {
                continue;
            }

            let current_address = match address.lock() {
                Ok(guard) => guard.clone(),
                Err(_) => return,
            };

            if active_addr == current_address {
                continue;
            }

            if last_refocus.elapsed() < Duration::from_millis(300) {
                continue;
            }
            last_refocus = Instant::now();

            if done.load(Ordering::Relaxed) {
                break;
            }

            let _ = Command::new("hyprctl")
                .args([
                    "dispatch",
                    "focuswindow",
                    &format!("address:{current_address}"),
                ])
                .status();
            let _ = proxy.send_event(AppEvent::FocusLost);
        }
    });
}

pub fn spawn_hyprland_close_watcher(
    proxy: EventLoopProxy<AppEvent>,
    done: Arc<AtomicBool>,
    address: Arc<std::sync::Mutex<String>>,
) {
    thread::spawn(move || {
        let socket_path = match hyprland_socket_path() {
            Some(path) => path,
            None => return,
        };
        let stream = match UnixStream::connect(socket_path) {
            Ok(stream) => stream,
            Err(_) => return,
        };

        let reader = BufReader::new(stream);
        for line in reader.lines().flatten() {
            if done.load(Ordering::Relaxed) {
                break;
            }
            let line = line.trim();
            if !line.starts_with("closewindow>>") {
                continue;
            }
            let payload = &line["closewindow>>".len()..];
            let closed_addr = payload.split(',').next().unwrap_or("");
            if closed_addr.is_empty() {
                continue;
            }
            let normalized_closed = if closed_addr.starts_with("0x") {
                closed_addr.to_string()
            } else {
                format!("0x{closed_addr}")
            };
            let current_address = match address.lock() {
                Ok(guard) => guard.clone(),
                Err(_) => return,
            };
            if normalized_closed == current_address {
                let _ = proxy.send_event(AppEvent::AppClosed(current_address));
            }
        }
    });
}

pub fn focus_window_by_address(address: &str) {
    if address.trim().is_empty() {
        return;
    }
    let _ = Command::new("hyprctl")
        .args(["dispatch", "focuswindow", &format!("address:{address}")])
        .status();
}
