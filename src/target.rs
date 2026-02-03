use tao::event_loop::{EventLoop, EventLoopProxy};

use crate::webview::{build_webview, WebViewHandle};
use crate::AppEvent;

pub fn spawn_app_cmd(cmd: &str) -> Result<u32, String> {
    let child = std::process::Command::new("sh")
        .args(["-lc", &format!("exec {cmd}")])
        .spawn()
        .map_err(|err| format!("Failed to launch app command: {err}"))?;
    Ok(child.id())
}

pub fn build_webview_target(
    event_loop: &EventLoop<AppEvent>,
    proxy: &EventLoopProxy<AppEvent>,
    target_url: &str,
) -> Result<(WebViewHandle, u32), String> {
    let handle = build_webview(event_loop, proxy, target_url)?;
    Ok((handle, std::process::id()))
}
