use tao::dpi::{PhysicalPosition, PhysicalSize};
use tao::event_loop::EventLoop;
use tao::platform::unix::WindowExtUnix;
use tao::window::WindowBuilder;
use wry::WebViewBuilderExtUnix;
use wry::{WebView, WebViewBuilder};

use crate::hyprland::MonitorGeometry;
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
        .with_transparent(true)
        .build(event_loop)
        .expect("Failed to create overlay window");

    let _ = window.set_ignore_cursor_events(true);

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
        background: transparent;
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
        background: rgba(15, 23, 42, 0.82);
        border: 1px solid rgba(148, 163, 184, 0.25);
        border-radius: 12px;
        box-shadow: 0 12px 36px rgba(15, 23, 42, 0.3);
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
      <div class="timer" id="timer">00:00</div>
    </div>
    <script>
      window.setTimerText = function (value) {
        var el = document.getElementById('timer');
        if (el) {
          el.textContent = value;
        }
      };
    </script>
  </body>
</html>"#,
        )
        .with_background_color((0, 0, 0, 0))
        .build()
        .expect("Failed to build overlay webview");

    OverlayWindow { window, webview }
}

impl OverlayWindow {
    pub fn set_timer_text(&self, text: &str) {
        let script = format!(
            "window.setTimerText && window.setTimerText({text:?});",
            text = text
        );
        let _ = self.webview.evaluate_script(&script);
    }

    pub fn set_position(&self, geometry: MonitorGeometry) {
        let width = 200;
        let height = 60;
        let margin = 16;
        let _ = geometry.height;
        let x = geometry.x + geometry.width - width - margin;
        let y = geometry.y + margin;
        self.window
            .set_inner_size(PhysicalSize::new(width as u32, height as u32));
        self.window.set_outer_position(PhysicalPosition::new(x, y));
    }
}
