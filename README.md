# Claude Code Project Tracker

Auto-tracks every Claude Code project on your machine: locations, status, GitHub URLs, deploy URLs, launch/deploy instructions, and full session history. Nothing leaves your machine.

![Claude Tracker screenshot](docs/screenshot.png)

Two halves you can install independently:

- **Plugin** — the hooks + CLI that record session activity to SQLite. Install via Claude Code's plugin marketplace.
- **Desktop dashboard** (optional) — a Tauri app that visualises the data the plugin collects.

## Install — plugin (recommended)

```
/plugin marketplace add Amoner/project-claude-tracker
/plugin install claude-tracker
```

Hooks activate on the next Claude Code session. No DMG, no Gatekeeper, no editing `settings.json` by hand.

**Slash commands:**
- `/claude-tracker:recent` — top-10 most-recently-active projects as a table
- `/claude-tracker:dashboard` — opens the desktop app if installed

## Install — desktop dashboard (optional)

The dashboard lets you search projects, edit metadata, manually add folders, import from IDE history (VS Code, Cursor, JetBrains), launch `claude` in your preferred terminal, and browse session history.

1. Download the latest `Claude.Tracker_<version>_aarch64.dmg` from [Releases](../../releases) (Apple Silicon Macs only for now)
2. Drag **Claude Tracker.app** to Applications
3. Macs will show "damaged" on unsigned builds — strip the quarantine tag once:
   ```
   xattr -cr /Applications/Claude\ Tracker.app
   ```
4. Launch normally

The dashboard reads the same SQLite DB the plugin writes, so you can install one or both in any order.

## How it works

```
┌─ Claude Code ──────────────────────┐
│                                    │
│  Claude Code fires lifecycle hooks │
│  (SessionStart, UserPromptSubmit,  │
│   Stop, SessionEnd, CwdChanged)    │
│                                    │
└──────────────┬─────────────────────┘
               │
               ▼
      ┌──────────────────┐
      │  tracker-cli     │  ← shipped by plugin (or bundled in the GUI)
      │     ingest       │
      └────────┬─────────┘
               │
               ▼
      ~/.claude-tracker/
        └── db.sqlite          ← shared by plugin + GUI
               │
               ▼
      ┌──────────────────┐
      │  Desktop GUI     │  ← optional Tauri app reads the DB
      └──────────────────┘
```

## Requirements

- [Claude Code](https://claude.ai/code) (for the plugin)
- macOS Apple Silicon (for the dashboard app; Intel Mac + Linux + Windows builds aren't published yet)

## Building from source

```sh
# Prerequisites: Rust toolchain, Node.js 18+
git clone https://github.com/Amoner/project-claude-tracker
cd project-claude-tracker
# CLI + plugin binaries
cargo build -p tracker-cli --release
# Desktop dashboard
cd tracker-app && npm install && npm run tauri build
# Output: target/release/bundle/macos/Claude Tracker.app
```

## Crate layout

| Crate / dir | Description |
|---|---|
| `crates/tracker-core` | DB schema, hook management, project discovery/sync, event ingestion, plugin detection |
| `crates/tracker-cli` | CLI sidecar — called by Claude Code hooks (`ingest`, `recent`, `discover`, `sync`, `list`, `doctor`) |
| `tracker-app/` | Tauri desktop UI |
| `plugins/claude-tracker/` | Plugin source — manifest, hooks, slash commands, per-arch binary shim |
| `.claude-plugin/marketplace.json` | Marketplace manifest so `/plugin marketplace add` discovers the plugin |

## License

MIT
