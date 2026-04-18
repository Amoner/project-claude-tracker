# Claude Code Project Tracker

A macOS desktop app that auto-tracks every Claude Code project on your machine: locations, status, GitHub URLs, deploy URLs, launch/deploy instructions, and session history.

Auto-discovers projects by scanning `~/.claude/projects/` and registers global Claude Code hooks so new sessions record themselves automatically.

![Claude Tracker screenshot](docs/screenshot.png)

## Features

- Auto-discovers all Claude Code projects on your machine
- Tracks sessions, prompt counts, last-active time per project
- Infers status (active / idle / archived) from activity
- Syncs GitHub URLs and deploy URLs
- One-click **Start** to open a terminal at the project folder and run `claude`
- Tray icon for quick access
- Settings to install/uninstall hooks and pick your preferred terminal

## Requirements

- macOS (Apple Silicon — aarch64)
- [Claude Code](https://claude.ai/code) installed

## Install

1. Download `Claude.Tracker_0.1.0_aarch64.dmg` from [Releases](../../releases)
2. Open the DMG and drag **Claude Tracker** to Applications
3. Right-click the app → **Open** (required once to bypass Gatekeeper on unsigned builds)
4. Go to **Settings → Hooks → Install** to register the Claude Code hooks

## Building from source

```sh
# Prerequisites: Rust toolchain, Node.js 18+
git clone https://github.com/am0n3r/project-claude-tracker
cd project-claude-tracker
cd tracker-app && npm install
npm run tauri build
# Output: target/release/bundle/macos/Claude Tracker.app
```

## Crate layout

| Crate | Description |
|---|---|
| `tracker-core` | DB schema, hook management, project discovery/sync, event ingestion |
| `tracker-cli` | CLI sidecar — called by Claude Code hooks to record events |
| `tracker-app` | Tauri desktop UI |

## License

MIT
