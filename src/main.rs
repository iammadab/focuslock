use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::platform::unix::WindowExtUnix;
use tao::window::{Fullscreen, WindowBuilder};
use wry::WebViewBuilderExtUnix;
use wry::{PageLoadEvent, WebViewBuilder};

mod config;
mod hyprland;
mod overlay;

use crate::config::Config;
use crate::hyprland::{move_window_to_empty_workspace, spawn_hyprland_watchdog};
use crate::overlay::{CLEAR_PROMPT_SCRIPT, HIDE_PROMPT_SCRIPT, OVERLAY_SCRIPT, SHOW_PROMPT_SCRIPT};

#[derive(Debug, Clone)]
pub enum AppEvent {
    FocusLost,
    PageLoaded,
    EscapeOpen,
    EscapeCancel,
    EscapeSubmit(String),
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
        url,
        total_seconds,
        escape_key,
    } = config;
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
                    let _ = webview.evaluate_script(&overlay::set_timer_script("Loading"));
                    next_tick = Instant::now() + Duration::from_millis(300);
                    return;
                }

                if let Some(until) = flash_until {
                    if Instant::now() < until {
                        let _ = webview.evaluate_script(&overlay::set_timer_script("Timer reset"));
                        next_tick = Instant::now() + Duration::from_millis(300);
                        return;
                    }
                    flash_until = None;
                }

                let remaining = total.saturating_sub(start.unwrap().elapsed());
                if remaining.is_zero() {
                    done = true;
                    done_flag.store(true, Ordering::Relaxed);
                    let _ = webview.evaluate_script(&overlay::set_timer_script("Done"));
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
                let _ = webview.evaluate_script(&overlay::set_timer_script(&text));

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
                let _ = webview.evaluate_script(SHOW_PROMPT_SCRIPT);
            }
            Event::UserEvent(AppEvent::EscapeCancel) => {
                if !prompt_open {
                    return;
                }
                prompt_open = false;
                let _ = webview.evaluate_script(HIDE_PROMPT_SCRIPT);
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
                    let _ = webview.evaluate_script(HIDE_PROMPT_SCRIPT);
                    let _ = webview.evaluate_script(&overlay::set_timer_script("Unlocked"));
                    *control_flow = ControlFlow::Wait;
                } else {
                    let _ = webview
                        .evaluate_script(&overlay::set_prompt_message_script("Incorrect PIN"));
                    let _ = webview.evaluate_script(CLEAR_PROMPT_SCRIPT);
                }
            }
            _ => {}
        }
    });
}
