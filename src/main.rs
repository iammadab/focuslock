use clap::Parser;
use std::time::{Duration, Instant};
use std::{
    io::{BufRead, BufReader},
    os::unix::net::UnixStream,
    path::PathBuf,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::platform::unix::WindowExtUnix;
use tao::window::{Fullscreen, WindowBuilder};
use url::Url;
use wry::WebViewBuilderExtUnix;
use wry::{PageLoadEvent, WebViewBuilder};

const OVERLAY_SCRIPT: &str = r#"
(function () {
  if (window.__focuslockInjected) {
    return;
  }
  window.__focuslockInjected = true;

  var bar = document.createElement('div');
  bar.id = '__focuslock_bar';
  bar.style.position = 'fixed';
  bar.style.top = '0';
  bar.style.left = '0';
  bar.style.right = '0';
  bar.style.height = '24px';
  bar.style.zIndex = '2147483647';
  bar.style.display = 'flex';
  bar.style.alignItems = 'center';
  bar.style.justifyContent = 'center';
  bar.style.background = 'rgba(17, 24, 39, 0.72)';
  bar.style.color = '#f8fafc';
  bar.style.fontFamily = '"Space Grotesk", "IBM Plex Sans", sans-serif';
  bar.style.fontSize = '12px';
  bar.style.letterSpacing = '0.08em';
  bar.style.textTransform = 'uppercase';
  bar.style.pointerEvents = 'none';
  bar.style.backdropFilter = 'blur(6px)';
  bar.style.position = 'fixed';
  bar.style.overflow = 'hidden';

  var text = document.createElement('div');
  text.id = '__focuslock_text';
  text.textContent = 'Focuslock';
  bar.appendChild(text);

  var prompt = document.createElement('div');
  prompt.id = '__focuslock_prompt';
  prompt.style.position = 'absolute';
  prompt.style.top = '0';
  prompt.style.left = '0';
  prompt.style.right = '0';
  prompt.style.height = '24px';
  prompt.style.display = 'none';
  prompt.style.alignItems = 'center';
  prompt.style.justifyContent = 'center';
  prompt.style.gap = '10px';
  prompt.style.background = 'rgba(15, 23, 42, 0.92)';
  prompt.style.color = '#f8fafc';
  prompt.style.fontSize = '12px';
  prompt.style.letterSpacing = '0.06em';
  prompt.style.textTransform = 'uppercase';
  prompt.style.pointerEvents = 'auto';

  var promptLabel = document.createElement('span');
  promptLabel.id = '__focuslock_prompt_label';
  promptLabel.textContent = 'Enter unlock PIN';

  var promptInput = document.createElement('span');
  promptInput.id = '__focuslock_prompt_input';
  promptInput.textContent = '';
  promptInput.style.fontFamily = '"IBM Plex Mono", "JetBrains Mono", monospace';
  promptInput.style.fontSize = '12px';
  promptInput.style.letterSpacing = '0.2em';

  prompt.appendChild(promptLabel);
  prompt.appendChild(promptInput);
  bar.appendChild(prompt);

  var root = document.body || document.documentElement;
  root.appendChild(bar);

  var promptActive = false;
  var promptBuffer = '';

  window.__focuslockSetTimer = function (value) {
    var node = document.getElementById('__focuslock_text');
    if (node) {
      node.textContent = value;
    }
  };

  function updatePrompt() {
    var label = document.getElementById('__focuslock_prompt_label');
    var input = document.getElementById('__focuslock_prompt_input');
    if (input) {
      input.textContent = promptBuffer.replace(/./g, '•');
    }
  }

  window.__focuslockShowPrompt = function () {
    promptActive = true;
    promptBuffer = '';
    var label = document.getElementById('__focuslock_prompt_label');
    if (label) {
      label.textContent = 'Enter unlock PIN';
    }
    var node = document.getElementById('__focuslock_prompt');
    if (node) {
      node.style.display = 'flex';
    }
    updatePrompt();
  };

  window.__focuslockHidePrompt = function () {
    promptActive = false;
    var node = document.getElementById('__focuslock_prompt');
    if (node) {
      node.style.display = 'none';
    }
    promptBuffer = '';
  };

  window.__focuslockSetPromptMessage = function (message) {
    var label = document.getElementById('__focuslock_prompt_label');
    if (label) {
      label.textContent = message;
    }
  };

  window.__focuslockClearPrompt = function () {
    promptBuffer = '';
    updatePrompt();
  };

  window.addEventListener('keydown', function (event) {
    if (!promptActive && event.ctrlKey && event.shiftKey && (event.key === 'q' || event.key === 'Q')) {
      if (window.ipc && window.ipc.postMessage) {
        window.ipc.postMessage('escape_open');
      }
      event.preventDefault();
      event.stopPropagation();
      return;
    }
    if (!promptActive) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();

    if (event.key === 'Escape') {
      if (window.ipc && window.ipc.postMessage) {
        window.ipc.postMessage('escape_cancel');
      }
      return;
    }
    if (event.key === 'Enter') {
      if (window.ipc && window.ipc.postMessage) {
        window.ipc.postMessage('escape_submit:' + promptBuffer);
      }
      return;
    }
    if (event.key === 'Backspace') {
      promptBuffer = promptBuffer.slice(0, -1);
      updatePrompt();
      return;
    }
    if (event.key && event.key.length === 1) {
      promptBuffer += event.key;
      updatePrompt();
    }
  }, true);
})();
"#;

#[derive(Parser, Debug)]
#[command(name = "focuslock")]
#[command(about = "Fullscreen focus window with a timer overlay", long_about = None)]
struct Args {
    #[arg(long)]
    url: String,

    #[arg(long, default_value_t = 0)]
    minutes: u64,

    #[arg(long, default_value_t = 0)]
    seconds: u64,

    #[arg(long)]
    escape_key: Option<String>,
}

#[derive(Debug, Clone)]
enum AppEvent {
    FocusLost,
    PageLoaded,
    EscapeOpen,
    EscapeCancel,
    EscapeSubmit(String),
}

fn parse_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|err| format!("Invalid URL: {err}"))?;
    match url.scheme() {
        "http" | "https" => Ok(url),
        _ => Err("URL must start with http:// or https://".to_string()),
    }
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
            return monitor.get("name")?.as_str().map(|name| name.to_string());
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

fn move_window_to_empty_workspace(pid: u32) {
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

fn spawn_hyprland_watchdog(proxy: EventLoopProxy<AppEvent>, done: Arc<AtomicBool>) {
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
                    &format!("address:{}", refocus_addr),
                ])
                .status();
            let _ = proxy.send_event(AppEvent::FocusLost);
        }
    });
}

fn main() {
    gtk::init().expect("Failed to initialize GTK");
    let args = Args::parse();
    let escape_key = args
        .escape_key
        .or_else(|| std::env::var("FOCUSLOCK_ESCAPE_KEY").ok());
    let total_seconds = args.minutes.saturating_mul(60).saturating_add(args.seconds);
    if total_seconds == 0 {
        eprintln!("total duration must be greater than 0 (use --minutes and/or --seconds)");
        std::process::exit(2);
    }

    let url = match parse_url(&args.url) {
        Ok(url) => url,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    };
    let target_url = url.as_str().to_string();

    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    let fullscreen = event_loop
        .primary_monitor()
        .map(|monitor| Fullscreen::Borderless(Some(monitor)));

    let mut window_builder = WindowBuilder::new()
        .with_title("Focuslock")
        .with_decorations(false);
    if let Some(fullscreen) = fullscreen {
        window_builder = window_builder.with_fullscreen(Some(fullscreen));
    }

    let window = window_builder
        .build(&event_loop)
        .expect("Failed to create window");

    let vbox = window.default_vbox().expect("Failed to access gtk vbox");
    let builder = WebViewBuilder::new_gtk(vbox);

    let proxy = event_loop.create_proxy();
    let loading_html = format!(
        r#"<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <style>
      html, body {{
        margin: 0;
        padding: 0;
        width: 100%;
        height: 100%;
        background: #0f172a;
      }}
    </style>
  </head>
  <body>
    <script>
      window.addEventListener('DOMContentLoaded', function () {{
        window.location.replace({target_url:?});
      }});
    </script>
  </body>
</html>"#
    );
    let webview = builder
        .with_html(loading_html)
        .with_initialization_script(OVERLAY_SCRIPT)
        .with_background_color((15, 23, 42, 255))
        .with_on_page_load_handler({
            let proxy = proxy.clone();
            move |event, current_url| {
                if matches!(event, PageLoadEvent::Finished)
                    && !current_url.starts_with("about:")
                    && !current_url.starts_with("data:")
                {
                    let _ = proxy.send_event(AppEvent::PageLoaded);
                }
            }
        })
        .with_ipc_handler({
            let proxy = proxy.clone();
            move |request| {
                let body = request.body();
                if body == "escape_open" {
                    let _ = proxy.send_event(AppEvent::EscapeOpen);
                } else if body == "escape_cancel" {
                    let _ = proxy.send_event(AppEvent::EscapeCancel);
                } else if let Some(pin) = body.strip_prefix("escape_submit:") {
                    let _ = proxy.send_event(AppEvent::EscapeSubmit(pin.to_string()));
                }
            }
        })
        .build()
        .expect("Failed to build webview");

    move_window_to_empty_workspace(std::process::id());

    let done_flag = Arc::new(AtomicBool::new(false));
    spawn_hyprland_watchdog(proxy.clone(), done_flag.clone());

    let total = Duration::from_secs(total_seconds);
    let mut start: Option<Instant> = None;
    let mut loaded = false;
    let mut next_tick = Instant::now();
    let mut done = false;
    let mut flash_until: Option<Instant> = None;
    let mut prompt_open = false;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(next_tick);

        match event {
            Event::NewEvents(StartCause::Init) => {
                next_tick = Instant::now();
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                if done {
                    *control_flow = ControlFlow::Wait;
                    return;
                }

                if start.is_none() {
                    let _ = webview.evaluate_script(
                        "window.__focuslockSetTimer && window.__focuslockSetTimer('Loading');",
                    );
                    next_tick = Instant::now() + Duration::from_millis(300);
                    return;
                }

                if let Some(until) = flash_until {
                    if Instant::now() < until {
                        let _ = webview.evaluate_script(
                            "window.__focuslockSetTimer && window.__focuslockSetTimer('Timer reset');",
                        );
                        next_tick = Instant::now() + Duration::from_millis(300);
                        return;
                    }
                    flash_until = None;
                }

                let remaining = total.saturating_sub(start.unwrap().elapsed());
                if remaining.is_zero() {
                    done = true;
                    done_flag.store(true, Ordering::Relaxed);
                    let _ = webview.evaluate_script(
                        "window.__focuslockSetTimer && window.__focuslockSetTimer('Done');",
                    );
                    *control_flow = ControlFlow::Wait;
                    return;
                }

                let remaining_secs = remaining.as_secs();
                let text = if remaining_secs >= 3600 {
                    let hours = remaining_secs / 3600;
                    let minutes = (remaining_secs % 3600) / 60;
                    let seconds = remaining_secs % 60;
                    format!("{hours:02}:{minutes:02}:{seconds:02}")
                } else {
                    let minutes = remaining_secs / 60;
                    let seconds = remaining_secs % 60;
                    format!("{minutes:02}:{seconds:02}")
                };
                let script =
                    format!("window.__focuslockSetTimer && window.__focuslockSetTimer({text:?});");
                let _ = webview.evaluate_script(&script);

                next_tick = Instant::now() + Duration::from_secs(1);
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                if done {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::UserEvent(AppEvent::FocusLost) => {
                if done {
                    return;
                }
                if !loaded {
                    return;
                }
                start = Some(Instant::now());
                flash_until = Some(Instant::now() + Duration::from_secs(2));
                next_tick = Instant::now();
            }
            Event::UserEvent(AppEvent::PageLoaded) => {
                if done || loaded {
                    return;
                }
                loaded = true;
                start = Some(Instant::now());
                next_tick = Instant::now();
            }
            Event::UserEvent(AppEvent::EscapeOpen) => {
                if done || prompt_open {
                    return;
                }
                if escape_key.is_none() {
                    return;
                }
                prompt_open = true;
                let _ = webview.evaluate_script(
                    "window.__focuslockShowPrompt && window.__focuslockShowPrompt();",
                );
            }
            Event::UserEvent(AppEvent::EscapeCancel) => {
                if !prompt_open {
                    return;
                }
                prompt_open = false;
                let _ = webview.evaluate_script(
                    "window.__focuslockHidePrompt && window.__focuslockHidePrompt();",
                );
            }
            Event::UserEvent(AppEvent::EscapeSubmit(pin)) => {
                if !prompt_open {
                    return;
                }
                let Some(expected) = escape_key.as_ref() else {
                    return;
                };
                if pin == *expected {
                    prompt_open = false;
                    done = true;
                    done_flag.store(true, Ordering::Relaxed);
                    let _ = webview.evaluate_script(
                        "window.__focuslockHidePrompt && window.__focuslockHidePrompt();",
                    );
                    let _ = webview.evaluate_script(
                        "window.__focuslockSetTimer && window.__focuslockSetTimer('Unlocked');",
                    );
                    *control_flow = ControlFlow::Wait;
                } else {
                    let _ = webview.evaluate_script(
                        "window.__focuslockSetPromptMessage && window.__focuslockSetPromptMessage('Incorrect PIN');",
                    );
                    let _ = webview.evaluate_script(
                        "window.__focuslockClearPrompt && window.__focuslockClearPrompt();",
                    );
                }
            }
            _ => {}
        }
    });
}
