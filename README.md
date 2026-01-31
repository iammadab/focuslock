# focuslock

Fullscreen WebView for focus sessions on Linux/Hyprland. It opens a URL in a dedicated workspace, keeps you on task with a countdown bar, and refocuses the window if you switch away.

## Requirements
- Linux (Wayland/X11); designed for Hyprland on Wayland
- Hyprland + `hyprctl` (for workspace jump and refocus watchdog)
- Rust toolchain
- WebKitGTK 4.1 runtime (GTK3-based)

Arch/Omarchy:
```bash
sudo pacman -S webkit2gtk-4.1
```

## Quick Start

Build and run:
```bash
cargo run -- --url https://monkeytype.com --minutes 25
```

Short test run:
```bash
cargo run -- --url https://monkeytype.com --seconds 20
```

Enable the escape hatch (Ctrl+Shift+Q):
```bash
cargo run -- --url https://monkeytype.com --minutes 25 --escape-key 1234
```

## Usage

```bash
focuslock --url <URL> [--minutes <N>] [--seconds <N>] [--escape-key <PIN>]
```

Flags:
- `--url`: target URL (http/https)
- `--minutes`: minutes to focus (optional)
- `--seconds`: seconds to focus (optional)
- `--escape-key`: enable unlock prompt with a PIN (optional)

Environment:
- `FOCUSLOCK_ESCAPE_KEY`: PIN for the unlock prompt (CLI flag overrides this)

## Advanced

### Behavior
- Opens in a new empty Hyprland workspace (ID >= 2) on the active monitor.
- Shows a thin top-bar timer overlay injected into the page.
- Timer starts after the first real page load finishes.
- Refocuses the window and resets the timer when you switch away (until done).
- When done or unlocked, focus is no longer forced and the window can be closed.

### Escape Hatch
- Press `Ctrl+Shift+Q` to open the unlock prompt.
- Enter the PIN and press `Enter` to unlock.
- Press `Esc` to cancel and return to focus mode.

### Install as a CLI

Local install:
```bash
cargo install --path .
```

Then run:
```bash
focuslock --url https://monkeytype.com --minutes 25
```

### Troubleshooting

Slow first load:
- WebKitGTK can take a few seconds to warm up on first launch. The app shows a loading bar until the page is ready.

Missing WebKitGTK:
- On Arch: `sudo pacman -S webkit2gtk-4.1`

## Notes
- The timer bar is injected into the page DOM (in-webview overlay).
- Hyprland watchdog refocuses and resets on focus loss (deterrent, not a hard lock).
