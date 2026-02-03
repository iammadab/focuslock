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
    pub class: String,
    pub title: String,
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

fn find_hyprland_address(pid: u32) -> Option<String> {
    let output = Command::new("hyprctl")
        .args(["-j", "clients"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let clients = value.as_array()?;
    for client in clients {
        let client_pid = client.get("pid")?.as_u64()? as u32;
        if client_pid == pid {
            let address = client.get("address")?.as_str()?;
            return Some(address.to_string());
        }
    }
    None
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
        let class = client
            .get("class")
            .and_then(|class| class.as_str())
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                client
                    .get("initialClass")
                    .and_then(|class| class.as_str())
                    .filter(|value| !value.trim().is_empty())
            })
            .unwrap_or("")
            .to_string();
        let title = client
            .get("title")
            .and_then(|title| title.as_str())
            .unwrap_or("")
            .to_string();

        if pid == 0 || address.is_empty() {
            continue;
        }

        results.push(ClientInfo {
            pid,
            class,
            title,
            address,
        });
    }

    results
}

pub fn client_exists_by_address(address: &str) -> bool {
    if address.trim().is_empty() {
        return false;
    }
    list_hyprland_clients()
        .into_iter()
        .any(|client| client.address == address)
}

pub fn find_client_by_pid(pid: u32) -> Option<ClientInfo> {
    list_hyprland_clients()
        .into_iter()
        .find(|client| client.pid == pid)
}

pub fn find_client_by_class(class: &str) -> Option<ClientInfo> {
    let target = class.trim().to_lowercase();
    if target.is_empty() {
        return None;
    }
    list_hyprland_clients()
        .into_iter()
        .find(|client| !client.class.is_empty() && client.class.to_lowercase() == target)
}

pub fn find_client_by_title_contains(title: &str) -> Option<ClientInfo> {
    let target = title.trim().to_lowercase();
    if target.is_empty() {
        return None;
    }
    list_hyprland_clients()
        .into_iter()
        .find(|client| !client.title.is_empty() && client.title.to_lowercase().contains(&target))
}

fn format_client_summary(client: &ClientInfo) -> String {
    let class = if client.class.is_empty() {
        "<none>"
    } else {
        client.class.as_str()
    };
    let title = if client.title.is_empty() {
        "<none>"
    } else {
        client.title.as_str()
    };
    format!("pid={} class={} title={}", client.pid, class, title)
}

pub fn resolve_app_window(
    pid: u32,
    app_class: Option<&str>,
    app_title: Option<&str>,
    timeout_ms: u64,
) -> Result<ClientInfo, String> {
    let start = Instant::now();
    let timeout = Duration::from_millis(timeout_ms);
    loop {
        if let Some(client) = find_client_by_pid(pid) {
            return Ok(client);
        }

        if let Some(class) = app_class {
            if let Some(client) = find_client_by_class(class) {
                return Ok(client);
            }
        }

        if let Some(title) = app_title {
            if let Some(client) = find_client_by_title_contains(title) {
                return Ok(client);
            }
        }

        if start.elapsed() >= timeout {
            let last_clients = list_hyprland_clients();
            let mut message = String::from("no app window matched the launched process");
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

pub fn move_window_to_empty_workspace(pid: u32) {
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

    let mut address = None;
    for _ in 0..20 {
        address = find_hyprland_address(pid);
        if address.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    let Some(address) = address else {
        return;
    };

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

pub fn spawn_hyprland_watchdog(proxy: EventLoopProxy<AppEvent>, done: Arc<AtomicBool>) {
    thread::spawn(move || {
        let socket_path = match hyprland_socket_path() {
            Some(path) => path,
            None => return,
        };
        let stream = match UnixStream::connect(socket_path) {
            Ok(stream) => stream,
            Err(_) => return,
        };

        let pid = std::process::id();
        let mut address = find_hyprland_address(pid);
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

            if address.as_deref() == Some(active_addr) {
                continue;
            }

            if address.is_none() {
                address = find_hyprland_address(pid);
            }

            let Some(refocus_addr) = address.clone() else {
                continue;
            };

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
                    &format!("address:{refocus_addr}"),
                ])
                .status();
            let _ = proxy.send_event(AppEvent::FocusLost);
        }
    });
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
