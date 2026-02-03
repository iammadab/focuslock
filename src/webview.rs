use tao::event_loop::{EventLoop, EventLoopProxy};
use tao::platform::unix::WindowExtUnix;
use tao::window::{Fullscreen, Window, WindowBuilder};
use wry::WebViewBuilderExtUnix;
use wry::{PageLoadEvent, WebView, WebViewBuilder};

use crate::AppEvent;

#[allow(dead_code)]
pub struct WebViewHandle {
    pub window: Window,
    pub webview: WebView,
}

pub fn build_webview(
    event_loop: &EventLoop<AppEvent>,
    proxy: &EventLoopProxy<AppEvent>,
    target_url: &str,
) -> Result<WebViewHandle, String> {
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
        .build()
        .map_err(|err| format!("Failed to build webview: {err}"))?;

    Ok(WebViewHandle { window, webview })
}
