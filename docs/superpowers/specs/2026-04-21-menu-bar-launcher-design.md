# Menu-bar launcher + library redesign

**Date:** 2026-04-21
**Status:** design approved; implementation pending
**Scope:** macOS only (Apple Silicon). Windows/Linux behavior unchanged.

## Goal

Make the tracker's primary job — "jump back into one of the last projects I was working on" — take two keystrokes instead of three clicks. Demote the current full window to a library/maintenance surface, and fix the broken Launch action that blocks the primary flow.

## Users & jobs

Single user (the author) running 20+ concurrent Claude Code projects. In priority order:

1. **Launch recent project.** 95% of opens. "I want to resume work on one of the last 5 projects."
2. **Browse / search all projects.** "Where did I put that thing from two months ago?"
3. **Edit metadata.** Notes, deploy URL, launch instructions, status overrides.
4. **Maintenance.** Find projects, rescan, resync, hook install, choose terminal.

Everything in this spec arranges around (1).

## Surfaces

Two surfaces, one DB, one core:

| Surface | Purpose | Frequency |
|---|---|---|
| **Popover** (new) | Recent-5 launcher in the menu bar | ~95% of opens |
| **Library window** (rethink of today's app) | Browse all, edit metadata, maintenance | ~5% of opens |

Both read the same SQLite (`~/.claude-tracker/db.sqlite`) and share the same Tauri commands. The only new backend work is a tray icon, global shortcut registration, and a second Tauri webview window for the popover.

## Popover — menu-bar launcher

### Presentation
- Menu-bar icon (mono SVG, 16 px). Click toggles the popover.
- Popover is a separate Tauri webview window, positioned under the menu-bar icon, sized `520 × 440` max, non-resizable, always-on-top while open.
- Dismisses on: Launch action, Esc, click-outside, hotkey pressed again, app lost focus.
- Opens in <200 ms — content is eagerly fetched on tray icon creation, refreshed when the popover is about to show.

### Content
- Rows sourced from `recent_active(limit=5)` — already exists in `api.ts:66` / Rust backend.
- Each row uses **three click zones** (same structure in the library):
  1. **Left area** (name, path, status dot, relative time) — click toggles inline expand.
  2. **Launch** (green, primary) — runs `start_claude(id)`.
  3. **View** (neutral) — runs `open_in_finder(path)`.
- Expanded row shows (read-only): description, GitHub URL as link, session/prompt counts, first-seen date. No editing in the popover.
- Status dot colors match `StatusBadge` palette: active / developing / deployed / planning / idle / stale / archived.
- Relative time formatted as `12m`, `2h`, `3d`, `5d`, `3w`, `2mo` (same logic as today's `time.ts`; reuse).

### Footer
- Always-visible search input. Empty search → show recent 5. Non-empty search → broaden to full DB, match against `name`, `path`, `github_url`, `notes` (matches today's filter in `App.tsx:57`).
- "Manage ↗" link bottom-right opens the library window.

### Empty state
- Fewer than 5 recent projects: show what exists.
- Zero projects: show a hint ("no projects yet — start a Claude Code session") and a "Find projects" button that opens the library window with the Find modal pre-opened.

### Keyboard
- `↑` / `↓` navigate selection.
- `Enter` — Launch selected row.
- `Space` — expand/collapse selected row.
- `⌘↵` — View selected row (reveal in Finder).
- `/` — focus search.
- `Esc` — if a row is expanded, collapse it; else dismiss popover.

## Global hotkey

- Default: `⌃⌥C` (Ctrl+Option+C) — toggles the popover.
- Configurable in Settings. Persisted in the `settings` table as `launcher_hotkey`.
- Registered via Tauri's `tauri-plugin-global-shortcut`. If registration fails (hotkey already claimed), surface a non-blocking banner in the library window pointing to Settings.

## Library window — inline-expand table

Replaces today's two-tab (`dashboard` / `settings`) layout. One main surface.

### Top bar
- **Search** (full-width on the left) — same matching rules as popover.
- **Filter pills** on the right:
  - `all N` (always shown first, is the default).
  - One pill per status (`active`, `developing`, `deployed`, `planning`, `idle`, `stale`).
  - `+ archived` toggle (right-most; off by default).
  - Pills show live counts. Clicking a pill filters to that status; only one status pill active at a time (radio-style). `+ archived` is an independent toggle.

### Table rows
- Same three-zone structure as popover rows — same components, same keyboard model.
- Ordered by `last_active_at` desc (same as popover). No other sort options in v1 (YAGNI).
- Clicking left area expands inline (below the row) with editable detail.

### Expanded row (editable)
- Notes — multi-line textarea, saves on blur (matches today's `ProjectDetail.tsx` notes UX).
- GitHub URL — read-only link.
- Launch instructions — textarea.
- Deploy URL + deploy platform + deploy instructions — inputs.
- Status override (dropdown with all values, plus "auto").
- Archive toggle.
- Activity summary (sessions, prompts, first-seen, enrichment synced).
- Collapses on second click of left area, Esc, or collapse arrow.

### Bottom chrome
- Bottom-left: **gear** icon → Settings modal (terminal preference, hooks, plugin status, view release notes, change hotkey).
- Bottom-right: **"…" menu** for maintenance actions:
  - Find projects (opens today's `FindProjects` modal).
  - Rescan (today's `run_discover`).
  - Sync (force) (today's `run_sync(true, true)`).
  - Reinstall hooks / plugin status.
- Hook status indicator (green/amber dot + text) sits to the left of the "…" menu so it stays glanceable but not loud.

## Launch-action bug fix

The current Launch button silently succeeds without opening a terminal. Root cause investigation and fix belong in this spec because the popover's primary action shares the same code path.

### Observations
- `terminal.log` is empty despite multiple failed clicks, meaning `terminal.launch()` returned `Ok(())`.
- `spawn_detached` uses `Stdio::null` for stdin / stdout / stderr (`crates/tracker-core/src/terminal.rs:320-326`), which hides any child-process failure.
- `Terminal::Ghostty` path uses `ghostty +new-window`, which on macOS requires the Ghostty app to already be running; otherwise the CLI exits 0 without opening a window.

### Fix
1. **Capture stderr.** In `spawn_detached`, replace `Stdio::null()` for stderr with `Stdio::from(File::options().create(true).append(true).open(terminal_log_path)?)` so kernel-level redirect appends the child's stderr to `~/.claude-tracker/logs/terminal.log`. Keep stdin/stdout nulled. Rotate the log at 1 MB by truncating-with-rename on open when size exceeds the cap.
2. **Probe Ghostty.** Before using `ghostty +new-window`, check whether Ghostty is already running (via `pgrep -x ghostty` or NSWorkspace running-apps). If not running, launch via `open -na Ghostty --args --working-directory=<cwd> --command="<shell> -l -i -c claude"` so macOS handles the app launch.
3. **Surface failures.** The Tauri `start_claude` command currently only errors if the cwd doesn't exist or if `terminal.launch` returns `Err`. Add a post-spawn readiness check: after spawn, wait up to 300 ms; if the child has exited non-zero, return an error with the tail of `terminal.log`.
4. **Inline banner.** Today's red banner in `ProjectDetail.tsx` stays; add equivalent in the popover row (collapses back on re-click).
5. **Telemetry.** Record every Launch attempt in a new `launches` table: `id, project_id, attempted_at, outcome (ok|spawn_err|child_err|path_missing), message, terminal_slug`. Library shows "last launch failed 2 min ago" chip in the expanded row when the latest entry is an error.

### Tests
- Unit: Ghostty path picks `open -na` when Ghostty is not running; picks `+new-window` when it is.
- Unit: `launches` table insert + most-recent-per-project query.
- Integration (manual, documented): click Launch against a project whose Ghostty instance is closed; verify a window appears with `claude` running.

## Onboarding / first-launch behavior

- On first install, if the user hasn't granted Accessibility permissions, the global hotkey silently fails to register. Show a one-time banner in the library window explaining how to grant it (System Settings → Privacy & Security → Accessibility) with a "Open settings" button.
- `Onboarding` modal unchanged in scope; restyled only.

## Components (frontend)

New / renamed:
- `components/Popover.tsx` — root of the popover webview.
- `components/ProjectRow.tsx` — shared three-zone row (used by popover and library).
- `components/LibraryTable.tsx` — the full table with filter pills.
- `components/RowDetail.tsx` — expanded content (two variants: read-only for popover, editable for library).
- `components/Settings.tsx` — renamed from `SettingsPage`; now a modal triggered by gear, not a tab.

Kept (restyled only):
- `Onboarding.tsx`, `FindProjects.tsx`, `ReleaseNotes.tsx`, `StatusBadge.tsx`.

Deleted:
- Tab navigation in `App.tsx` (replaced by single library view + Settings modal).

## Backend (Rust / Tauri)

New:
- Menu-bar tray icon wiring in `src-tauri/src/lib.rs` via `tauri::tray`.
- Second webview window definition in `tauri.conf.json` for the popover (`label: "popover"`, `decorations: false`, `transparent: true`, `always_on_top: true`, `resizable: false`).
- Global shortcut registration using `tauri-plugin-global-shortcut`.
- `launches` table and command `record_launch_attempt` / `get_last_launch(project_id)`.
- Helper `spawn_with_stderr_capture` in `crates/tracker-core/src/terminal.rs` replacing `spawn_detached`.
- Ghostty running-probe fallback in `Terminal::Ghostty::build_command`.

Unchanged:
- All existing commands (`list_projects`, `recent_active`, `run_sync`, `run_discover`, `get_hook_status`, `install_hooks`, `open_in_finder`, `open_url`, `start_claude`, etc.).
- SQLite schema for `projects` (`launches` is additive).
- Ingestion path (hook CLI → `tracker-cli ingest`).

## Visual language

- Palette: zinc-800/900/950 background, zinc-300/400 text, emerald-600 for Launch, existing `StatusBadge` hues for dots and badges.
- Row height: 44 px collapsed, 120–180 px expanded depending on content.
- Status dot: 8 px circle, filled with the status hue.
- Paths rendered in `ui-monospace, SF Mono, monospace` at 11 px, zinc-500.
- Names in system sans at 13 px, zinc-100, semibold.
- No new fonts; no animations over 120 ms.

## Phasing

Each phase is mergeable/shippable on its own:

1. **Fix Launch** — `spawn_with_stderr_capture`, Ghostty running-probe, `launches` table, inline error banner on current `ProjectDetail`. Proves the fix in the existing UI before adding surfaces.
2. **Popover + tray** — tray icon, popover webview, `ProjectRow` + `RowDetail (read-only)`, popover-scoped keyboard.
3. **Global hotkey** — `tauri-plugin-global-shortcut`, settings entry, accessibility-permission banner.
4. **Library rethink** — `LibraryTable`, filter pills, gear/"…"/hook-status chrome. Existing sidebar+detail dies.
5. **Row editing in library** — `RowDetail (editable)` with save-on-blur for notes and inputs for metadata.
6. **Polish** — restyle `Onboarding`, `FindProjects`, `ReleaseNotes`, release notes announcing the new UI.

## Out of scope

- Windows menu-bar / system-tray variant (system tray exists on Windows but popover UX differs — separate spec).
- Linux support.
- Live deploy polling changes.
- Sorting or grouping in library beyond `last_active_at` desc + filter pills.
- Sessions/prompts timeline visualization.
- Multi-select / bulk actions.
- i18n.

## Verification checklist (done = all pass)

- [ ] Menu-bar icon present after launch; click opens popover under the icon.
- [ ] `⌃⌥C` toggles popover from any foreground app.
- [ ] Popover shows the 5 most recent projects, ordered newest first.
- [ ] Row click zones: left expands, Launch runs claude in the preferred terminal, View opens Finder at the project path.
- [ ] Launch works when Ghostty isn't already running (regression-tests the bug).
- [ ] Launch failures render an inline banner and a row entry in `terminal.log`.
- [ ] Keyboard: ↑/↓/Enter/Space/⌘↵/`/`/Esc all behave per spec.
- [ ] Library: filter pills show live counts, search filters, row inline-expand works, notes save on blur.
- [ ] Settings modal opens from gear icon; hotkey change takes effect without restart.
- [ ] `launches` table records every attempt with outcome.
