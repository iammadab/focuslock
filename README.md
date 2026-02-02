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

### External Done Trigger
Focuslock exposes a local HTTP endpoint that can unlock the session.

Use case:
- For outcome-based focus sessions where time is not the only constraint (e.g., finish 10 puzzles, read 10 pages, complete a study milestone).

Why this exists:
- It lets external tools or custom task trackers signal completion without relying solely on the timer.

Endpoint:
- `POST http://127.0.0.1:<focuslock_port>/done`

Port discovery:
- Focuslock appends `focuslock_port` to the URL it loads.
- The port starts at `9742` and increments until a free port is found.
- External apps should read `focuslock_port` and call `http://127.0.0.1:<focuslock_port>/done`.

Behavior:
- Unlocks immediately (same as timer completion).

Responses:
- `200 OK` when the session is marked done
- `409 Conflict` if already done
- `404` for unknown paths, `405` for wrong methods

Example:
```bash
curl -X POST http://127.0.0.1:9742/done
```

Example URL param:
```text
https://your-task.app/session/abc?focuslock_port=9742
```

#### Integration Notes
- Read the `focuslock_port` query param from the URL Focuslock loads.
- Build the endpoint as `http://127.0.0.1:<focuslock_port>/done`.
- Send `POST` to that endpoint when the task is complete.
- Treat `409 Conflict` as already-complete; retrying is safe but unnecessary.
- This endpoint is localhost-only; it is meant for apps running inside the Focuslock webview.

Note: this endpoint is bound to localhost. To trigger it from another device,
you would need to bind the server to a LAN address and allow the port in your firewall.

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
