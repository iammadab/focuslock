# focuslock

Fullscreen app focus sessions on Linux/Hyprland. It launches a command in a dedicated workspace, keeps you on task with a countdown overlay, and refocuses the window if you switch away.

## Requirements
- Linux (Wayland/X11); designed for Hyprland on Wayland
- Hyprland + `hyprctl` (for workspace jump and refocus watchdog)
- Rust toolchain

## Quick Start

Build and run:
```bash
cargo run -- --app-cmd "omarchy-launch-webapp https://monkeytype.com" --minutes 25
```

Short test run:
```bash
cargo run -- --app-cmd "omarchy-launch-webapp https://monkeytype.com" --seconds 20
```

Enable the escape hatch (Ctrl+Shift+Q):
```bash
cargo run -- --app-cmd "omarchy-launch-webapp https://monkeytype.com" --minutes 25 --escape-key 1234
```

## Usage

```bash
focuslock --app-cmd <CMD> [--minutes <N>] [--seconds <N>] [--escape-key <PIN>]
```

Flags:
- `--app-cmd`: command to launch (quoted if it contains spaces)
- `--minutes`: minutes to focus (optional)
- `--seconds`: seconds to focus (optional)
- `--escape-key`: enable unlock prompt with a PIN (optional)

Environment:
- `FOCUSLOCK_ESCAPE_KEY`: PIN for the unlock prompt (CLI flag overrides this)

## Advanced

### Behavior
- Opens in a new empty Hyprland workspace (ID >= 2) on the active monitor.
- Shows a thin timer overlay while running.
- Timer starts after the app window resolves.
- Refocuses the window and resets the timer when you switch away (until done).
- When done or unlocked, focus is no longer forced and the window can be closed.

### Escape Hatch
- Press `Ctrl+Shift+Q` to open the unlock prompt.
- Enter the PIN and press `Enter` to unlock.
- Press `Esc` to cancel and return to focus mode.


### Notes
- `--url` is intentionally removed for now; we can reintroduce it later as a convenience alias that expands to `--app-cmd "omarchy-launch-webapp <url>"` once the app-cmd flow is validated.
- Hyprland watchdog refocuses and resets on focus loss (deterrent, not a hard lock).

### Install as a CLI

Local install:
```bash
cargo install --path .
```

Then run:
```bash
focuslock --app-cmd "omarchy-launch-webapp https://monkeytype.com" --minutes 25
```

### Troubleshooting

Slow first launch:
- Some apps take a few seconds to appear; the overlay stays in "Loading" until the window resolves.
