# claude-tracker (plugin)

The hooks + CLI half of [Claude Tracker](https://github.com/Amoner/project-claude-tracker). Registers Claude Code hooks on `SessionStart`, `SessionEnd`, `UserPromptSubmit`, `Stop`, and `CwdChanged`, invoking `tracker-cli ingest <Event>` which writes to `~/.claude-tracker/db.sqlite`.

## Install

```
/plugin marketplace add Amoner/project-claude-tracker
/plugin install claude-tracker
```

That's it — no DMG, no Gatekeeper, no manual settings.json editing. Hooks are active on the next Claude Code session.

## Slash commands

- **`/claude-tracker:recent`** — top-10 most-recently-active projects as a table
- **`/claude-tracker:dashboard`** — opens the desktop app if installed

## Desktop dashboard (optional)

The plugin handles tracking; the **desktop app** provides a dashboard UI (project list, search, deploy metadata, manual project adds, IDE-history import, terminal launcher, etc.). Install separately from the [Releases page](https://github.com/Amoner/project-claude-tracker/releases) — it reads the same SQLite DB the plugin writes.

## Data

All session data lives at `~/.claude-tracker/db.sqlite`. The plugin never uploads anything or talks to any external service.
