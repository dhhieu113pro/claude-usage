# Claude Usage Widget

A small, transparent Windows desktop widget that shows Claude usage for several profiles at once.
Built in Rust with [windows-rs](https://github.com/microsoft/windows-rs) (Direct2D + DirectWrite).

Each profile is drawn as a double ring:

- **Inner ring / centre number** – 5-hour session usage (% used)
- **Outer ring** – 7-day weekly usage (% used), shown as a small pill on the ring
- **Text under the name** – the local date the weekly window resets, e.g. `Resets Oct 29`

Ring colour is based on percent used, per ring:

| Used | Colour |
|---|---|
| up to 60% | green |
| above 60% up to 90% | orange |
| above 90% | red |

## Features

- Any number of profiles, laid out in a compact grid of 2 rows
- Transparent, always-on-top and click-through: mouse clicks go to the apps underneath
- Tray icon menu: **Refresh now**, **Unlock to move** (drag the widget), **Edit config**, **Quit**
- Per-monitor DPI aware; starts bottom-right of the work area
- Draws its own tray icon at startup (dark disc with an orange and green double ring); if it is hidden, drag it out of the tray overflow
- Refreshes every 15 minutes by default; the old numbers stay visible while a refresh runs
- Config is reloaded on every refresh, so adding or removing profiles needs no restart

## How it works

Every profile is a Claude Code config directory (`CLAUDE_CONFIG_DIR`). For each one the widget runs
`claude -p "/usage"` (hidden window, 60 s timeout) with that variable set and parses the text:

```
Current session: 0% used · resets Oct 27, 2:20pm (Asia/Bangkok)
Current week (all models): 93% used · resets Oct 28, 9pm (Asia/Bangkok)
```

Claude Code handles authentication and token refresh itself, so the widget never touches
`.credentials.json`. `claude.exe` must be on `PATH`.

> The output format of `/usage` is not a stable interface; if a Claude Code update changes it,
> the profile shows `bad output` and `parse()` in `src/usage.rs` needs adjusting.

## Configuration

`%APPDATA%\claude-usage-widget\config.json` is created on first run:

```json
{
  "refresh_seconds": 900,
  "profiles": [
    { "name": "main", "command": "claude" }
  ]
}
```

| Field | Meaning |
|---|---|
| `name` | Label shown under the rings |
| `command` | Launcher command. Its `.cmd`/`.bat` on `PATH` is parsed for `CLAUDE_CONFIG_DIR`; without one, `~/.claude` is used |
| `config_dir` | Explicit config directory (`~` expanded). Takes priority over `command` |
| `refresh_seconds` | Poll interval in seconds, default 900 (15 min), minimum 15. "Refresh now" and restarting the app each make an immediate request, so avoid spamming them |

Profile launchers look like this (`claude-work.cmd`):

```bat
set "CLAUDE_CONFIG_DIR=%USERPROFILE%\claude-profiles\claude-work"
claude.exe %*
```

## MCP servers

Tray menu → **Manage MCP servers** opens a local page (`127.0.0.1`, random port and token, only while the widget runs) to add, edit and delete MCP servers (`stdio` with args and env vars, or `http` with headers).

- The shared list is stored in `%APPDATA%\claude-usage-widget\mcp.json`.
- Each profile has a checkbox per server. It is **on** when the server is present in `mcpServers` of `<profile dir>\.claude.json`; toggling adds or removes the entry and leaves the rest of the file untouched.
- Editing or renaming a server updates it in every profile that has it on; deleting removes it from all of them.
- Env values are stored in plain text in both files, as Claude Code does itself. Restart running Claude sessions to pick up changes.

## Build and run

Requires the Rust MSVC toolchain.

```powershell
cargo build --release
.\target\release\claude-usage-widget.exe
```

Quit the widget from the tray before rebuilding, otherwise the exe is locked.

## Start with Windows

```powershell
$exe = "$PWD\target\release\claude-usage-widget.exe"
Set-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Run ClaudeUsageWidget "`"$exe`""
```

Remove with `Remove-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Run ClaudeUsageWidget`.

## Troubleshooting

| Label | Meaning |
|---|---|
| `no claude` | `claude.exe` could not be started (not on `PATH`) |
| `token expired` | The output mentions an expired login; run `/login` on that profile |
| `bad output` | `claude -p "/usage"` printed something the parser does not recognise (logged out, or format changed) |
| `timeout` | No result within 60 s |

## Project layout

- `src/main.rs` – window, layered rendering, tray menu, polling thread
- `src/config.rs` – config file, profile directory resolution
- `src/usage.rs` – runs `claude -p "/usage"` and parses it
- `src/mcp.rs` – central MCP list, sync into profile `.claude.json`
- `src/web.rs`, `src/mcp_ui.html` – localhost server and UI for the MCP manager
