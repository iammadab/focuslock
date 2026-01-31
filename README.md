# focuslock (v3)

Fullscreen WebView with a thin top-bar timer overlay and Hyprland refocus watchdog.

## Requirements
- Rust toolchain
- WebView runtime for Linux (WebKitGTK)

## Run
```bash
cargo run -- --url https://monkeytype.com --minutes 25
```

```bash
cargo run -- --url https://monkeytype.com --seconds 20
```

## Notes
- The timer bar is injected into the page DOM (in-webview overlay).
- On Hyprland, a watchdog refocuses the window and resets the timer on focus loss.
- A future version can replace this with a native overlay window for stricter control.
