use tao::dpi::LogicalSize;
use tao::event_loop::EventLoop;
use tao::platform::unix::WindowExtUnix;
use tao::window::WindowBuilder;
use wry::WebViewBuilderExtUnix;
use wry::{WebView, WebViewBuilder};

use crate::AppEvent;

pub struct OverlayWindow {
    pub window: tao::window::Window,
    pub webview: WebView,
}

pub fn build_overlay_window(event_loop: &EventLoop<AppEvent>) -> OverlayWindow {
    let window = WindowBuilder::new()
        .with_title("Focuslock Overlay")
        .with_decorations(false)
        .with_resizable(false)
        .with_always_on_top(true)
        .with_inner_size(LogicalSize::new(420.0, 80.0))
        .build(event_loop)
        .expect("Failed to create overlay window");

    let vbox = window.default_vbox().expect("Failed to access gtk vbox");
    let webview = WebViewBuilder::new_gtk(vbox)
        .with_html(
            r#"<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <style>
      :root {
        color-scheme: only light;
      }
      html, body {
        margin: 0;
        padding: 0;
        width: 100%;
        height: 100%;
        background: #0b1221;
        color: #e2e8f0;
        font-family: "IBM Plex Sans", "Noto Sans", sans-serif;
      }
      .wrap {
        display: flex;
        align-items: center;
        justify-content: space-between;
        height: 100%;
        padding: 14px 18px;
        box-sizing: border-box;
      }
      .label {
        font-size: 14px;
        letter-spacing: 0.2em;
        text-transform: uppercase;
        color: #94a3b8;
      }
      .timer {
        font-size: 32px;
        font-weight: 600;
        letter-spacing: 0.08em;
      }
    </style>
  </head>
  <body>
    <div class="wrap">
      <div class="label">Focuslock</div>
      <div class="timer">00:00</div>
    </div>
  </body>
</html>"#,
        )
        .build()
        .expect("Failed to build overlay webview");

    OverlayWindow { window, webview }
}
