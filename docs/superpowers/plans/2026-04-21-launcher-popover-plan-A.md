# Menu-bar launcher (Plan A) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a menu-bar popover + global hotkey that launches `claude` in a terminal for one of the last 5 projects, and fix the silently-failing Launch action that the primary flow depends on.

**Architecture:** A second Tauri webview window (`popover`) is spawned on demand, positioned under the tray icon, and dismissed on blur/Esc/Launch. The existing `main` window becomes the library/maintenance surface (Plan B). Launch-path hardening lives in `tracker-core` so both the current `ProjectDetail` Start button and the new popover share the fix. A new SQLite table `launches` records attempt outcomes for telemetry.

**Tech Stack:** Rust (tracker-core, Tauri backend), React 18 + TypeScript + Tailwind + Vite (frontend), `tauri-plugin-global-shortcut` v2, rusqlite, SQLite.

**Spec reference:** [`docs/superpowers/specs/2026-04-21-menu-bar-launcher-design.md`](../specs/2026-04-21-menu-bar-launcher-design.md) — sections 2 (Popover), 4 (Launch bug fix), 5 (Global hotkey). Library rethink (spec §3) is out of scope for this plan and covered by Plan B.

---

## File structure

**Create:**
- `tracker-app/src/popover.tsx` — popover webview entry point, mounts `<Popover />`.
- `tracker-app/popover.html` — HTML host for the popover (second Vite entry).
- `tracker-app/src/components/Popover.tsx` — popover root component (search, row list, empty state).
- `tracker-app/src/components/ProjectRow.tsx` — three-zone row (shared between popover and Plan B's library).
- `tracker-app/src/components/RowDetail.tsx` — expanded-row content (read-only in popover).
- `tracker-app/src/hooks/useKeyboardNav.ts` — ↑/↓/Enter/Space/⌘↵/Esc handling for rows.

**Modify:**
- `crates/tracker-core/src/db.rs` — add `launches` table migration, `LaunchOutcome` enum, `record_launch`, `last_launch` methods.
- `crates/tracker-core/src/terminal.rs` — rename/replace `spawn_detached` → `spawn_with_stderr_capture`; add Ghostty running-probe + `open -na` fallback; add 1 MB log rotation.
- `crates/tracker-core/src/paths.rs` — expose `terminal_log_path()` helper.
- `tracker-app/src-tauri/Cargo.toml` — add `tauri-plugin-global-shortcut = "2"`.
- `tracker-app/src-tauri/tauri.conf.json` — add `popover` window definition; register plugin in capabilities.
- `tracker-app/src-tauri/src/lib.rs` — register global shortcut plugin, initialize tray with popover toggle, expose new commands.
- `tracker-app/src-tauri/src/tray.rs` — replace submenu-based tray with popover-toggling tray.
- `tracker-app/src-tauri/src/commands.rs` — add `toggle_popover`, `hide_popover`, `get_last_launch`, `get_launcher_hotkey`, `set_launcher_hotkey`; rewrite `start_claude` to record outcomes.
- `tracker-app/src/api.ts` — add `getLastLaunch`, `getLauncherHotkey`, `setLauncherHotkey`.
- `tracker-app/vite.config.ts` — add second rollup entry for `popover.html`.
- `tracker-app/src/components/ProjectDetail.tsx` — replace inline `handleStart` error banner with reusable `LaunchResult` display (shared with popover).

**Test:**
- `crates/tracker-core/src/db.rs` — in-line `#[cfg(test)]` tests for `launches` table (new cases in existing mod).
- `crates/tracker-core/src/terminal.rs` — in-line tests for Ghostty probe fallback and stderr capture.
- `tracker-app/src/components/ProjectRow.test.tsx` — new; render + click-zone tests (Vitest + React Testing Library).
- `tracker-app/src/components/Popover.test.tsx` — new; keyboard nav + search broadening.

---

## Phase 0 · Setup & safety nets

### Task 0.1: Add test harness for the frontend

**Files:**
- Modify: `tracker-app/package.json`
- Create: `tracker-app/vitest.config.ts`
- Create: `tracker-app/src/test/setup.ts`

- [ ] **Step 1: Check whether Vitest is already configured**

Run: `grep -E '"(vitest|@testing-library)' tracker-app/package.json || echo "not installed"`
Expected: `not installed` (confirms we need to add it; if already present, skip to Task 0.2).

- [ ] **Step 2: Install Vitest + React Testing Library**

Run from `tracker-app/`:
```
npm install -D vitest @testing-library/react @testing-library/jest-dom @testing-library/user-event jsdom @vitest/coverage-v8
```
Expected: packages added under `devDependencies`, lockfile updated.

- [ ] **Step 3: Add the vitest config**

Create `tracker-app/vitest.config.ts`:
```ts
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    globals: true,
  },
});
```

- [ ] **Step 4: Add the test setup**

Create `tracker-app/src/test/setup.ts`:
```ts
import "@testing-library/jest-dom/vitest";
import { vi, afterEach } from "vitest";
import { cleanup } from "@testing-library/react";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));
```

- [ ] **Step 5: Add the `test` script to package.json**

In `tracker-app/package.json` under `"scripts"`, add:
```
"test": "vitest run",
"test:watch": "vitest"
```

- [ ] **Step 6: Verify the harness works**

Run: `cd tracker-app && npm test -- --reporter=verbose`
Expected: `No test files found` exit 0 (harness loads, just no tests yet).

- [ ] **Step 7: Commit**

```
git add tracker-app/package.json tracker-app/package-lock.json tracker-app/vitest.config.ts tracker-app/src/test/setup.ts
git commit -m "chore: add vitest + react testing library harness"
```

---

## Phase 1 · Launch action hardening

### Task 1.1: Add `launches` table migration

**Files:**
- Modify: `crates/tracker-core/src/db.rs`

- [ ] **Step 1: Write the failing test**

Append to the `#[cfg(test)] mod tests` block in `crates/tracker-core/src/db.rs` (use `make_db()` helper already defined there — if not, grep for an existing test helper and reuse it):
```rust
#[test]
fn launches_table_exists() {
    let db = make_db();
    let count: i64 = db
        .conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'launches'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "launches table was not created by migrate()");
}
```

- [ ] **Step 2: Run the test and confirm it fails**

Run: `cargo test -p tracker-core launches_table_exists -- --nocapture`
Expected: FAIL — `launches table was not created by migrate()`.

- [ ] **Step 3: Add the table to `migrate()`**

In `crates/tracker-core/src/db.rs`, inside the `execute_batch` string in `migrate()`, append (after the `settings` table block):
```sql
CREATE TABLE IF NOT EXISTS launches (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    attempted_at TEXT NOT NULL,
    outcome TEXT NOT NULL,
    message TEXT,
    terminal_slug TEXT
);

CREATE INDEX IF NOT EXISTS idx_launches_project_attempted
    ON launches(project_id, attempted_at DESC);
```

- [ ] **Step 4: Run the test again**

Run: `cargo test -p tracker-core launches_table_exists`
Expected: PASS.

- [ ] **Step 5: Commit**

```
git add crates/tracker-core/src/db.rs
git commit -m "feat(core): add launches table for Launch telemetry"
```

### Task 1.2: `LaunchOutcome` enum + `record_launch` / `last_launch`

**Files:**
- Modify: `crates/tracker-core/src/db.rs`

- [ ] **Step 1: Write the failing tests**

Append to the `#[cfg(test)] mod tests` block in `db.rs`:
```rust
#[test]
fn record_and_fetch_last_launch() {
    use chrono::{DateTime, Utc};
    let db = make_db();
    let id = db.upsert_project_by_path(std::path::Path::new("/tmp/x"), "x").unwrap();
    assert!(db.last_launch(id).unwrap().is_none());

    db.record_launch(id, LaunchOutcome::Ok, None, Some("ghostty")).unwrap();
    let first = db.last_launch(id).unwrap().expect("launch row");
    assert_eq!(first.outcome, LaunchOutcome::Ok);
    assert_eq!(first.terminal_slug.as_deref(), Some("ghostty"));
    assert!(first.message.is_none());

    std::thread::sleep(std::time::Duration::from_millis(2));
    db.record_launch(id, LaunchOutcome::SpawnErr, Some("no such file".into()), Some("terminal_app")).unwrap();
    let second = db.last_launch(id).unwrap().expect("launch row");
    assert_eq!(second.outcome, LaunchOutcome::SpawnErr);
    assert_eq!(second.message.as_deref(), Some("no such file"));
    let attempted_at: DateTime<Utc> = second.attempted_at.parse().unwrap();
    assert!(attempted_at <= Utc::now());
}
```

- [ ] **Step 2: Run the test — fails to compile**

Run: `cargo test -p tracker-core record_and_fetch_last_launch`
Expected: compile error — `LaunchOutcome` / `record_launch` / `last_launch` not found.

- [ ] **Step 3: Add the enum and methods**

In `crates/tracker-core/src/db.rs`:

Near the other enums/types (or at the top of `impl Db`), add:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchOutcome {
    Ok,
    SpawnErr,
    ChildErr,
    PathMissing,
}

impl LaunchOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            LaunchOutcome::Ok => "ok",
            LaunchOutcome::SpawnErr => "spawn_err",
            LaunchOutcome::ChildErr => "child_err",
            LaunchOutcome::PathMissing => "path_missing",
        }
    }

    pub fn from_str(s: &str) -> Option<LaunchOutcome> {
        match s {
            "ok" => Some(LaunchOutcome::Ok),
            "spawn_err" => Some(LaunchOutcome::SpawnErr),
            "child_err" => Some(LaunchOutcome::ChildErr),
            "path_missing" => Some(LaunchOutcome::PathMissing),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LaunchRecord {
    pub project_id: i64,
    pub attempted_at: String,
    pub outcome: LaunchOutcome,
    pub message: Option<String>,
    pub terminal_slug: Option<String>,
}
```

Inside `impl Db`, add:
```rust
pub fn record_launch(
    &self,
    project_id: i64,
    outcome: LaunchOutcome,
    message: Option<String>,
    terminal_slug: Option<&str>,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    self.conn
        .prepare_cached(
            "INSERT INTO launches (project_id, attempted_at, outcome, message, terminal_slug) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?
        .execute(params![
            project_id,
            now,
            outcome.as_str(),
            message,
            terminal_slug
        ])?;
    Ok(())
}

pub fn last_launch(&self, project_id: i64) -> Result<Option<LaunchRecord>> {
    let row = self
        .conn
        .prepare_cached(
            "SELECT project_id, attempted_at, outcome, message, terminal_slug \
             FROM launches WHERE project_id = ?1 \
             ORDER BY attempted_at DESC, id DESC LIMIT 1",
        )?
        .query_row(params![project_id], |r| {
            let outcome_str: String = r.get(2)?;
            Ok(LaunchRecord {
                project_id: r.get(0)?,
                attempted_at: r.get(1)?,
                outcome: LaunchOutcome::from_str(&outcome_str).unwrap_or(LaunchOutcome::ChildErr),
                message: r.get(3)?,
                terminal_slug: r.get(4)?,
            })
        })
        .optional()?;
    Ok(row)
}
```

- [ ] **Step 4: Run the test**

Run: `cargo test -p tracker-core record_and_fetch_last_launch`
Expected: PASS.

- [ ] **Step 5: Commit**

```
git add crates/tracker-core/src/db.rs
git commit -m "feat(core): record_launch + last_launch on Db"
```

### Task 1.3: Expose `terminal_log_path()` helper

**Files:**
- Modify: `crates/tracker-core/src/paths.rs`

- [ ] **Step 1: Write the failing test**

In `crates/tracker-core/src/paths.rs`, append inside an existing (or new) `#[cfg(test)] mod tests` block:
```rust
#[test]
fn terminal_log_path_under_logs_dir() {
    let p = terminal_log_path().unwrap();
    assert!(p.to_string_lossy().ends_with("logs/terminal.log"), "got: {}", p.display());
}
```

- [ ] **Step 2: Confirm compile/test fail**

Run: `cargo test -p tracker-core terminal_log_path_under_logs_dir`
Expected: compile error — `terminal_log_path` not found.

- [ ] **Step 3: Implement `terminal_log_path`**

In `crates/tracker-core/src/paths.rs` (alongside the other path helpers; grep for `pub fn logs_dir` or `pub fn data_dir` to see the pattern):
```rust
pub fn terminal_log_path() -> anyhow::Result<std::path::PathBuf> {
    let dir = logs_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("terminal.log"))
}
```

If `logs_dir()` does not already exist, add it above (and verify which existing helper returns `~/.claude-tracker/` by running `grep -nE 'pub fn .*_dir|home|tracker' crates/tracker-core/src/paths.rs`):
```rust
pub fn logs_dir() -> anyhow::Result<std::path::PathBuf> {
    // Reuse the helper that returns `~/.claude-tracker/`. In this repo that's
    // `data_dir()` (if renamed, replace with the matching function found above).
    Ok(data_dir()?.join("logs"))
}
```
The existing `paths::append_log("terminal.log", ...)` call in `commands.rs:start_claude` already writes into this directory, so `logs_dir` is guaranteed to resolve to `~/.claude-tracker/logs/`. If the helper's name differs, `terminal_log_path()` must still resolve to the same file that `append_log("terminal.log", ...)` targets — otherwise old diagnostic logs go to a different file than the stderr redirect.

- [ ] **Step 4: Run the test**

Run: `cargo test -p tracker-core terminal_log_path_under_logs_dir`
Expected: PASS.

- [ ] **Step 5: Commit**

```
git add crates/tracker-core/src/paths.rs
git commit -m "feat(core): expose terminal_log_path() for stderr redirect"
```

### Task 1.4: Add 1 MB log rotation helper

**Files:**
- Modify: `crates/tracker-core/src/paths.rs`

- [ ] **Step 1: Write the failing test**

Append to the `#[cfg(test)] mod tests` block in `paths.rs`:
```rust
#[test]
fn rotate_if_over_cap_moves_file() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("terminal.log");
    std::fs::write(&p, vec![b'x'; 1024]).unwrap();
    // 1 KiB cap → should rotate a 1024-byte file.
    rotate_log_if_over(&p, 512).unwrap();
    assert!(!p.exists(), "original should be moved aside");
    let rotated = tmp.path().join("terminal.log.1");
    assert!(rotated.exists(), "rotated file expected at .1");
}

#[test]
fn rotate_skips_when_under_cap() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("terminal.log");
    std::fs::write(&p, b"small").unwrap();
    rotate_log_if_over(&p, 1_000_000).unwrap();
    assert!(p.exists(), "should not rotate when under cap");
}
```

Add `tempfile = "3"` to `crates/tracker-core/Cargo.toml` under `[dev-dependencies]` if not already present.

- [ ] **Step 2: Confirm fail**

Run: `cargo test -p tracker-core rotate_`
Expected: compile error — `rotate_log_if_over` not found.

- [ ] **Step 3: Implement**

In `crates/tracker-core/src/paths.rs`:
```rust
/// Rename `path` to `path.1` (overwriting any prior `.1`) when its size exceeds `max_bytes`.
/// Does nothing if the file is missing or under the cap.
pub fn rotate_log_if_over(path: &std::path::Path, max_bytes: u64) -> anyhow::Result<()> {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if meta.len() <= max_bytes {
        return Ok(());
    }
    let rotated = path.with_extension(
        format!("{}.1", path.extension().and_then(|s| s.to_str()).unwrap_or("log"))
    );
    // Overwrite any existing .1 file.
    let _ = std::fs::remove_file(&rotated);
    std::fs::rename(path, &rotated)?;
    Ok(())
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p tracker-core rotate_`
Expected: both pass.

- [ ] **Step 5: Commit**

```
git add crates/tracker-core/Cargo.toml crates/tracker-core/src/paths.rs
git commit -m "feat(core): rotate_log_if_over helper for 1MB terminal.log cap"
```

### Task 1.5: Replace `spawn_detached` with stderr-capturing variant

**Files:**
- Modify: `crates/tracker-core/src/terminal.rs`

- [ ] **Step 1: Write the failing test**

In `crates/tracker-core/src/terminal.rs` inside the existing `#[cfg(test)] mod tests` block:
```rust
#[test]
fn spawn_with_stderr_capture_redirects_to_log() {
    let tmp = tempfile::tempdir().unwrap();
    let log_path = tmp.path().join("terminal.log");
    let mut cmd = Command::new("sh");
    cmd.args(["-c", "echo boom 1>&2; exit 0"]);
    spawn_with_stderr_capture(cmd, &log_path, 1_000_000).unwrap();
    // Child runs async — wait briefly for it to finish and flush.
    std::thread::sleep(std::time::Duration::from_millis(200));
    let content = std::fs::read_to_string(&log_path).unwrap();
    assert!(content.contains("boom"), "expected child stderr to be redirected; log={content}");
}
```

Add `tempfile` to `dev-dependencies` if Task 1.4 didn't already.

- [ ] **Step 2: Confirm fail**

Run: `cargo test -p tracker-core spawn_with_stderr_capture_redirects_to_log`
Expected: compile error — function not found.

- [ ] **Step 3: Implement**

In `crates/tracker-core/src/terminal.rs`, **add** a new function (keep `spawn_detached` for a moment; we'll delete it after the next task swaps callers):
```rust
use crate::paths;

pub fn spawn_with_stderr_capture(
    mut cmd: Command,
    log_path: &Path,
    max_bytes: u64,
) -> Result<()> {
    paths::rotate_log_if_over(log_path, max_bytes)?;
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let err_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(err_file));
    cmd.spawn()?;
    Ok(())
}
```

- [ ] **Step 4: Run test**

Run: `cargo test -p tracker-core spawn_with_stderr_capture_redirects_to_log`
Expected: PASS.

- [ ] **Step 5: Commit**

```
git add crates/tracker-core/src/terminal.rs
git commit -m "feat(core): spawn_with_stderr_capture for diagnosing Launch failures"
```

### Task 1.6: Ghostty running-probe + `open -na` fallback (macOS)

**Files:**
- Modify: `crates/tracker-core/src/terminal.rs`

- [ ] **Step 1: Write the failing test**

Append to the macOS-gated section of the `#[cfg(test)] mod tests` block:
```rust
#[cfg(target_os = "macos")]
#[test]
fn ghostty_build_command_uses_open_when_not_running() {
    // build_command_ghostty_with(running = false) should use `open -na Ghostty`
    let cmd = Terminal::Ghostty
        .build_command_macos(Path::new("/tmp/work"), "claude", false)
        .unwrap();
    let prog = cmd.get_program().to_string_lossy().into_owned();
    assert!(prog.ends_with("open"), "expected `open`; got {prog}");
    let args: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    assert!(args.iter().any(|a| a == "-na"), "missing -na");
    assert!(args.iter().any(|a| a == "Ghostty"), "missing Ghostty app arg");
    assert!(args.iter().any(|a| a.contains("/tmp/work")), "missing cwd pass-through");
}

#[cfg(target_os = "macos")]
#[test]
fn ghostty_build_command_uses_cli_when_running() {
    let cmd = Terminal::Ghostty
        .build_command_macos(Path::new("/tmp/work"), "claude", true)
        .unwrap();
    let prog = cmd.get_program().to_string_lossy().into_owned();
    assert!(prog.contains("ghostty"), "expected ghostty binary; got {prog}");
    let args: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    assert!(args.iter().any(|a| a == "+new-window"), "missing +new-window");
}
```

- [ ] **Step 2: Confirm fail**

Run: `cargo test -p tracker-core ghostty_build_command -- --nocapture`
Expected: compile error — `build_command_macos` not found.

- [ ] **Step 3: Refactor Ghostty branch + add probe**

In `crates/tracker-core/src/terminal.rs`, replace the Ghostty match arm with a call to a new helper. Add:

```rust
#[cfg(target_os = "macos")]
fn ghostty_is_running() -> bool {
    // `pgrep -x ghostty` exits 0 when at least one process matches.
    std::process::Command::new("pgrep")
        .args(["-x", "ghostty"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
```

Rename the existing private `build_command` to take the platform-specific logic through a helper on `Terminal`:

```rust
impl Terminal {
    #[cfg(target_os = "macos")]
    pub(crate) fn build_command_macos(
        self,
        cwd: &Path,
        cmd: &str,
        ghostty_running: bool,
    ) -> Result<Command> {
        let cwd_str = cwd.to_string_lossy().into_owned();
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        match self {
            Terminal::Ghostty if ghostty_running => {
                let bin = self
                    .binary()
                    .ok_or_else(|| anyhow!("ghostty binary not found"))?;
                let mut c = Command::new(bin);
                c.arg("+new-window")
                    .arg(format!("--working-directory={cwd_str}"))
                    .arg(format!("--command={shell} -l -i -c {cmd}"));
                Ok(c)
            }
            Terminal::Ghostty => {
                // Cold-launch: use `open -na` so macOS starts Ghostty.app itself.
                let mut c = Command::new("open");
                c.args(["-na", "Ghostty", "--args"])
                    .arg(format!("--working-directory={cwd_str}"))
                    .arg(format!("--command={shell} -l -i -c {cmd}"));
                Ok(c)
            }
            // …leave all other arms delegating to existing `build_command` logic…
            other => other.build_command(cwd, cmd),
        }
    }
}
```

Then, in the public `launch()` method, replace the Ghostty branch of `build_command(...)` with:

```rust
pub fn launch(self, cwd: &Path, cmd: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let command = match self {
        Terminal::Ghostty => self.build_command_macos(cwd, cmd, ghostty_is_running())?,
        _ => self.build_command(cwd, cmd)?,
    };
    #[cfg(not(target_os = "macos"))]
    let command = self.build_command(cwd, cmd)?;

    let log = paths::terminal_log_path()?;
    spawn_with_stderr_capture(command, &log, 1_024 * 1_024)
}
```

(Remove the old `spawn_detached(...)` call from `launch()` and delete the private `spawn_detached` function — it is now superseded.)

- [ ] **Step 4: Run tests**

Run: `cargo test -p tracker-core ghostty_build_command`
Expected: both pass.

- [ ] **Step 5: Regression-check the full terminal module**

Run: `cargo test -p tracker-core terminal::`
Expected: all existing terminal tests still pass.

- [ ] **Step 6: Commit**

```
git add crates/tracker-core/src/terminal.rs
git commit -m "fix(core): probe Ghostty; cold-launch via `open -na` when not running"
```

### Task 1.7: Rewrite `start_claude` command to record outcome

**Files:**
- Modify: `tracker-app/src-tauri/src/commands.rs`

- [ ] **Step 1: Replace `start_claude`**

Find the existing `start_claude` (around `tracker-app/src-tauri/src/commands.rs:246`). Replace the whole function with:
```rust
#[tauri::command]
pub fn start_claude(state: Shared<'_>, id: i64) -> Result<(), String> {
    let (project_id, path, terminal) = {
        let db = state.db.lock().map_err(err)?;
        let project = db
            .get_project(id)
            .map_err(err)?
            .ok_or_else(|| "project not found".to_string())?;
        let pref = db
            .get_setting(PREFERRED_TERMINAL_KEY)
            .map_err(err)?
            .and_then(|s| Terminal::from_slug(&s));
        let terminal = pref.unwrap_or_else(Terminal::default_installed);
        (project.id, project.path_buf(), terminal)
    };

    let terminal_slug = terminal.slug();

    if !path.exists() {
        let message = format!("project path does not exist: {}", path.display());
        record(state, project_id, LaunchOutcome::PathMissing, Some(message.clone()), terminal_slug);
        return Err(message);
    }

    match terminal.launch(&path, "claude") {
        Ok(()) => {
            record(state, project_id, LaunchOutcome::Ok, None, terminal_slug);
            Ok(())
        }
        Err(e) => {
            let message = format!("{e:#}");
            paths::append_log("terminal.log", &format!("start_claude err: {message}"));
            record(state, project_id, LaunchOutcome::SpawnErr, Some(message.clone()), terminal_slug);
            Err(message)
        }
    }
}

fn record(
    state: Shared<'_>,
    project_id: i64,
    outcome: LaunchOutcome,
    message: Option<String>,
    terminal_slug: &str,
) {
    if let Ok(db) = state.db.lock() {
        let _ = db.record_launch(project_id, outcome, message, Some(terminal_slug));
    }
}
```

At the top of `commands.rs`, add to the imports:
```rust
use tracker_core::db::LaunchOutcome;
```

- [ ] **Step 2: Build**

Run: `cargo check -p tracker-app`
Expected: clean build.

- [ ] **Step 3: Commit**

```
git add tracker-app/src-tauri/src/commands.rs
git commit -m "feat(app): record Launch outcome into launches table"
```

### Task 1.8: Expose `get_last_launch` command

**Files:**
- Modify: `tracker-app/src-tauri/src/commands.rs`
- Modify: `tracker-app/src-tauri/src/lib.rs`
- Modify: `tracker-app/src/api.ts`

- [ ] **Step 1: Add the Rust command**

In `commands.rs` (next to `start_claude`):
```rust
#[tauri::command]
pub fn get_last_launch(state: Shared<'_>, project_id: i64) -> Result<Option<tracker_core::db::LaunchRecord>, String> {
    let db = state.db.lock().map_err(err)?;
    db.last_launch(project_id).map_err(err)
}
```

Register in `tracker-app/src-tauri/src/lib.rs` inside `tauri::generate_handler![...]`:
```rust
commands::get_last_launch,
```

- [ ] **Step 2: Add the TypeScript binding**

In `tracker-app/src/api.ts`, add type and method:
```ts
export type LaunchOutcome = "ok" | "spawn_err" | "child_err" | "path_missing";

export type LaunchRecord = {
  project_id: number;
  attempted_at: string;
  outcome: LaunchOutcome;
  message: string | null;
  terminal_slug: string | null;
};
```

And in the `api` object:
```ts
getLastLaunch: (projectId: number) =>
  invoke<LaunchRecord | null>("get_last_launch", { projectId }),
```

- [ ] **Step 3: Build**

Run: `cd tracker-app && npm run build`
Expected: clean build.

- [ ] **Step 4: Commit**

```
git add tracker-app/src-tauri/src/commands.rs tracker-app/src-tauri/src/lib.rs tracker-app/src/api.ts
git commit -m "feat(app): expose get_last_launch command + ts binding"
```

### Task 1.9: Smoke test the Launch fix end-to-end

**Files:** (manual verification, no code changes)

- [ ] **Step 1: Ensure Ghostty is quit**

Run: `osascript -e 'quit app "Ghostty"' 2>/dev/null; sleep 1; pgrep -x ghostty || echo "ghostty not running"`
Expected: `ghostty not running`.

- [ ] **Step 2: Build and run the dev app**

From `tracker-app/`:
```
npm run tauri dev
```
Expected: app window opens.

- [ ] **Step 3: Click Start on a project**

In the current `ProjectDetail` top bar, click **Start** on any project whose path exists.
Expected: a Ghostty window opens at the project's `cwd` and runs `claude`.

- [ ] **Step 4: Verify log and telemetry**

After clicking Start, run:
```
cat ~/.claude-tracker/logs/terminal.log 2>/dev/null | tail
sqlite3 ~/.claude-tracker/db.sqlite "SELECT outcome, terminal_slug, substr(message,1,60) FROM launches ORDER BY id DESC LIMIT 3;"
```
Expected: latest `launches` row has `outcome = 'ok'`, `terminal_slug = 'ghostty'`, no error in `terminal.log` (file may not exist if the child produced no stderr).

- [ ] **Step 5: Commit a release note for this phase**

No code change; just a marker commit so the history is readable:
```
git commit --allow-empty -m "chore: Phase 1 complete — Launch smoke test passing"
```

---

## Phase 2 · Popover webview + tray toggle

### Task 2.1: Add the `popover` window to `tauri.conf.json`

**Files:**
- Modify: `tracker-app/src-tauri/tauri.conf.json`

- [ ] **Step 1: Replace the `app.windows` array**

Replace the existing single-window entry under `app.windows` with:
```json
"windows": [
  {
    "label": "main",
    "title": "Claude Tracker",
    "width": 1100,
    "height": 720,
    "minWidth": 800,
    "minHeight": 500,
    "resizable": true,
    "fullscreen": false,
    "visible": true
  },
  {
    "label": "popover",
    "url": "popover.html",
    "width": 520,
    "height": 440,
    "resizable": false,
    "decorations": false,
    "transparent": true,
    "alwaysOnTop": true,
    "skipTaskbar": true,
    "focus": true,
    "visible": false,
    "hiddenTitle": true
  }
]
```

- [ ] **Step 2: Build**

Run: `cd tracker-app && npm run tauri dev` (Ctrl+C after it starts — we only need it to validate config parsing).
Expected: no JSON schema error on boot.

- [ ] **Step 3: Commit**

```
git add tracker-app/src-tauri/tauri.conf.json
git commit -m "feat(app): add popover window to tauri config"
```

### Task 2.2: Wire Vite to build `popover.html` as a second entry

**Files:**
- Create: `tracker-app/popover.html`
- Create: `tracker-app/src/popover.tsx`
- Modify: `tracker-app/vite.config.ts`

- [ ] **Step 1: Create `popover.html`**

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Claude Tracker — Launcher</title>
  </head>
  <body class="bg-transparent">
    <div id="popover-root"></div>
    <script type="module" src="/src/popover.tsx"></script>
  </body>
</html>
```

- [ ] **Step 2: Create `src/popover.tsx`**

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import "./index.css";
import { Popover } from "./components/Popover";

const root = document.getElementById("popover-root");
if (!root) throw new Error("popover-root missing");
ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <Popover />
  </React.StrictMode>,
);
```

(The `Popover` component is stubbed in the next task.)

- [ ] **Step 3: Update Vite config**

In `tracker-app/vite.config.ts`, set multi-entry rollup input:
```ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "path";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        popover: resolve(__dirname, "popover.html"),
      },
    },
  },
});
```

- [ ] **Step 4: Stub `Popover` so the build succeeds**

Create `tracker-app/src/components/Popover.tsx`:
```tsx
export function Popover() {
  return <div className="p-4 text-zinc-200">popover</div>;
}
```

- [ ] **Step 5: Build**

Run: `cd tracker-app && npm run build`
Expected: build emits `dist/index.html` AND `dist/popover.html`.

- [ ] **Step 6: Commit**

```
git add tracker-app/popover.html tracker-app/src/popover.tsx tracker-app/src/components/Popover.tsx tracker-app/vite.config.ts
git commit -m "feat(app): popover vite entry + stub"
```

### Task 2.3: Replace the tray submenu with popover toggle

**Files:**
- Modify: `tracker-app/src-tauri/src/tray.rs`
- Modify: `tracker-app/src-tauri/src/commands.rs`
- Modify: `tracker-app/src-tauri/src/lib.rs`

- [ ] **Step 1: Add `toggle_popover` and `hide_popover` commands**

Append to `tracker-app/src-tauri/src/commands.rs`:
```rust
#[tauri::command]
pub fn toggle_popover(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let Some(w) = app.get_webview_window("popover") else {
        return Err("popover window missing".into());
    };
    if w.is_visible().map_err(err)? {
        let _ = w.hide();
    } else {
        position_popover(&app, &w).map_err(err)?;
        let _ = w.show();
        let _ = w.set_focus();
    }
    Ok(())
}

#[tauri::command]
pub fn hide_popover(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("popover") {
        let _ = w.hide();
    }
    Ok(())
}

fn position_popover(app: &tauri::AppHandle, w: &tauri::WebviewWindow) -> anyhow::Result<()> {
    use tauri::{LogicalPosition, Manager, PhysicalPosition};
    // Prefer tray-icon rect when available; fall back to top-right of the
    // primary monitor so we always land somewhere visible.
    if let Some(rect) = app
        .tray_by_id("main-tray")
        .and_then(|t| t.get_rectangle().ok().flatten())
    {
        let target = PhysicalPosition::new(
            rect.position.x as i32 + rect.size.width as i32 / 2 - 260,
            rect.position.y as i32 + rect.size.height as i32 + 6,
        );
        w.set_position(target)?;
    } else if let Some(mon) = w.primary_monitor()? {
        let size = mon.size();
        w.set_position(LogicalPosition::new(size.width as i32 - 540, 40))?;
    }
    Ok(())
}
```

Register both in `tracker-app/src-tauri/src/lib.rs` invoke handler list:
```rust
commands::toggle_popover,
commands::hide_popover,
```

- [ ] **Step 2: Rewrite `tray::install` to toggle popover on left-click**

Replace the body of `install()` in `tracker-app/src-tauri/src/tray.rs`:
```rust
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    let menu = Menu::new(app)?;
    let open_library = MenuItem::with_id(app, "open-library", "Open Library…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    menu.append(&open_library)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&quit)?;

    let _tray = TrayIconBuilder::with_id("main-tray")
        .tooltip("Claude Tracker")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button, button_state, .. } = event {
                if button == tauri::tray::MouseButton::Left
                    && button_state == tauri::tray::MouseButtonState::Up
                {
                    let app = tray.app_handle().clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = crate::commands::toggle_popover(app);
                    });
                }
            }
        })
        .build(app)?;
    Ok(())
}

fn handle_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    use tauri::Manager;
    match event.id.as_ref() {
        "open-library" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
        "quit" => app.exit(0),
        _ => {}
    }
}
```

Remove the now-unused `load_recent` / `short_path` / recent-submenu helpers.

- [ ] **Step 3: Build and smoke-test**

Run: `cd tracker-app && npm run tauri dev`.
Expected: clicking the menu-bar icon shows the stub popover ("popover") near the tray.

- [ ] **Step 4: Commit**

```
git add tracker-app/src-tauri/src/tray.rs tracker-app/src-tauri/src/commands.rs tracker-app/src-tauri/src/lib.rs
git commit -m "feat(app): tray left-click toggles popover"
```

### Task 2.4: Dismiss popover on blur and Esc

**Files:**
- Modify: `tracker-app/src-tauri/src/lib.rs`

- [ ] **Step 1: Add blur-to-hide handler in `setup`**

In `tracker-app/src-tauri/src/lib.rs`, inside the `.setup(|app| { ... })` block, after `tray::install(&app.handle())?;`, add:
```rust
if let Some(popover) = app.get_webview_window("popover") {
    let pop_handle = popover.clone();
    popover.on_window_event(move |event| {
        if let tauri::WindowEvent::Focused(false) = event {
            let _ = pop_handle.hide();
        }
    });
}
```

- [ ] **Step 2: Add Esc handler in the frontend Popover**

Edit `tracker-app/src/components/Popover.tsx`:
```tsx
import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

export function Popover() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        invoke("hide_popover").catch(() => {});
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  return <div className="p-4 text-zinc-200">popover</div>;
}
```

- [ ] **Step 3: Smoke-test**

Run: `npm run tauri dev`. Click tray → popover shows. Press Esc → it hides. Click elsewhere → it hides.

- [ ] **Step 4: Commit**

```
git add tracker-app/src-tauri/src/lib.rs tracker-app/src/components/Popover.tsx
git commit -m "feat(app): dismiss popover on blur + Esc"
```

### Task 2.5: Build `ProjectRow` (shared three-zone row)

**Files:**
- Create: `tracker-app/src/components/ProjectRow.tsx`
- Create: `tracker-app/src/components/ProjectRow.test.tsx`
- Modify: `tracker-app/src/time.ts` — expose `relativeTime(iso, now)` if not already.

- [ ] **Step 1: Check `time.ts`**

Run: `grep -n "relativeTime\|export" tracker-app/src/time.ts`
Expected output tells us whether `relativeTime` exists. If not, add it:
```ts
export function relativeTime(iso: string | null, now: Date = new Date()): string {
  if (!iso) return "never";
  const then = new Date(iso).getTime();
  const diffMs = now.getTime() - then;
  const s = Math.round(diffMs / 1000);
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.round(s / 60)}m`;
  if (s < 86400) return `${Math.round(s / 3600)}h`;
  if (s < 86400 * 30) return `${Math.round(s / 86400)}d`;
  if (s < 86400 * 365) return `${Math.round(s / (86400 * 30))}mo`;
  return `${Math.round(s / (86400 * 365))}y`;
}
```

- [ ] **Step 2: Write the failing test**

Create `tracker-app/src/components/ProjectRow.test.tsx`:
```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { ProjectRow } from "./ProjectRow";
import type { Project } from "../api";

function fakeProject(over: Partial<Project> = {}): Project {
  return {
    id: 1,
    path: "/Users/a/work/x",
    name: "x",
    status: null,
    status_manual: false,
    github_url: null,
    deploy_url: null,
    deploy_platform: null,
    deploy_instructions: null,
    launch_instructions: null,
    deploy_live_lookup: false,
    first_seen_at: "2026-01-01T00:00:00Z",
    last_active_at: new Date(Date.now() - 12 * 60_000).toISOString(),
    sessions_started: 5,
    prompts_count: 10,
    notes: null,
    enrichment_synced_at: null,
    archived_at: null,
    description: null,
    effective_status: "active",
    ...over,
  };
}

describe("ProjectRow", () => {
  it("renders name, relative time, and path", () => {
    render(<ProjectRow project={fakeProject()} expanded={false} onExpand={() => {}} onLaunch={() => {}} onView={() => {}} />);
    expect(screen.getByText("x")).toBeInTheDocument();
    expect(screen.getByText("/Users/a/work/x")).toBeInTheDocument();
    expect(screen.getByText("12m")).toBeInTheDocument();
  });

  it("fires onLaunch when Launch clicked", () => {
    const onLaunch = vi.fn();
    render(<ProjectRow project={fakeProject()} expanded={false} onExpand={() => {}} onLaunch={onLaunch} onView={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: /launch/i }));
    expect(onLaunch).toHaveBeenCalledWith(1);
  });

  it("fires onView when View clicked", () => {
    const onView = vi.fn();
    render(<ProjectRow project={fakeProject()} expanded={false} onExpand={() => {}} onLaunch={() => {}} onView={onView} />);
    fireEvent.click(screen.getByRole("button", { name: /view/i }));
    expect(onView).toHaveBeenCalledWith("/Users/a/work/x");
  });

  it("fires onExpand when left area clicked", () => {
    const onExpand = vi.fn();
    render(<ProjectRow project={fakeProject()} expanded={false} onExpand={onExpand} onLaunch={() => {}} onView={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: /expand/i }));
    expect(onExpand).toHaveBeenCalledWith(1);
  });
});
```

- [ ] **Step 3: Run test — fails**

Run: `cd tracker-app && npm test`
Expected: imports fail (ProjectRow not found).

- [ ] **Step 4: Implement `ProjectRow`**

Create `tracker-app/src/components/ProjectRow.tsx`:
```tsx
import { Project } from "../api";
import { relativeTime } from "../time";

const STATUS_HEX: Record<string, string> = {
  active: "#10b981",
  developing: "#0ea5e9",
  deployed: "#8b5cf6",
  planning: "#f59e0b",
  idle: "#71717a",
  stale: "#ef4444",
  archived: "#52525b",
};

function shortPath(p: string): string {
  const home = "/Users/" + (p.split("/")[2] ?? "");
  return p.startsWith(home) ? "~" + p.slice(home.length) : p;
}

export function ProjectRow({
  project,
  expanded,
  onExpand,
  onLaunch,
  onView,
}: {
  project: Project;
  expanded: boolean;
  onExpand: (id: number) => void;
  onLaunch: (id: number) => void;
  onView: (path: string) => void;
}) {
  const dot = STATUS_HEX[project.effective_status] ?? STATUS_HEX.idle;
  return (
    <div
      className={`flex items-stretch border-b border-zinc-800 ${
        expanded ? "bg-zinc-900/60" : "bg-transparent hover:bg-zinc-900/40"
      }`}
      data-testid={`row-${project.id}`}
    >
      <button
        aria-label="expand"
        onClick={() => onExpand(project.id)}
        className="flex-1 min-w-0 text-left px-3 py-2 border-r border-zinc-800"
      >
        <div className="flex items-center gap-2">
          <span
            className="inline-block h-2 w-2 rounded-full shrink-0"
            style={{ background: dot }}
            aria-hidden
          />
          <span className="flex-1 min-w-0 truncate text-zinc-100 font-semibold">
            {project.name}
          </span>
          <span className="shrink-0 text-[11px] text-zinc-400">
            {relativeTime(project.last_active_at)}
          </span>
        </div>
        <div className="mt-0.5 pl-4 truncate text-[11px] font-mono text-zinc-500">
          {shortPath(project.path)}
        </div>
      </button>
      <button
        aria-label="Launch"
        onClick={() => onLaunch(project.id)}
        className="px-3 bg-emerald-600/90 text-white text-[11px] font-semibold hover:bg-emerald-500 flex flex-col items-center justify-center border-r border-zinc-800"
      >
        <span aria-hidden>▶</span>
        Launch
      </button>
      <button
        aria-label="View"
        onClick={() => onView(project.path)}
        className="px-3 bg-zinc-800 text-zinc-200 text-[11px] font-semibold hover:bg-zinc-700 flex flex-col items-center justify-center"
      >
        <span aria-hidden>◱</span>
        View
      </button>
    </div>
  );
}
```

- [ ] **Step 5: Run tests**

Run: `cd tracker-app && npm test`
Expected: all `ProjectRow` tests pass.

- [ ] **Step 6: Commit**

```
git add tracker-app/src/components/ProjectRow.tsx tracker-app/src/components/ProjectRow.test.tsx tracker-app/src/time.ts
git commit -m "feat(app): ProjectRow three-zone component with tests"
```

### Task 2.6: Build `RowDetail` (read-only variant)

**Files:**
- Create: `tracker-app/src/components/RowDetail.tsx`

- [ ] **Step 1: Create component**

```tsx
import { Project } from "../api";

export function RowDetail({ project, mode }: { project: Project; mode: "popover" | "library" }) {
  const readOnly = mode === "popover";
  return (
    <div className="border-b border-zinc-800 bg-zinc-900/60 px-5 py-3 text-[12px] text-zinc-400">
      {project.description && (
        <p className="mb-2 text-zinc-300">{project.description}</p>
      )}
      {project.github_url && (
        <div className="mb-1">
          <span className="text-zinc-500 uppercase tracking-wider text-[10px] mr-2">github</span>
          <a className="text-zinc-200 hover:underline" href={project.github_url} target="_blank" rel="noreferrer">
            {project.github_url}
          </a>
        </div>
      )}
      <div className="text-[11px] text-zinc-500">
        {project.sessions_started} sessions · {project.prompts_count} prompts · first seen{" "}
        {project.first_seen_at.slice(0, 10)}
      </div>
      {!readOnly && (
        <p className="mt-2 text-[11px] text-zinc-500">editing lives in library — Plan B</p>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Commit**

```
git add tracker-app/src/components/RowDetail.tsx
git commit -m "feat(app): RowDetail read-only detail component"
```

### Task 2.7: Implement the Popover body

**Files:**
- Modify: `tracker-app/src/components/Popover.tsx`

- [ ] **Step 1: Flesh out the Popover**

Replace `tracker-app/src/components/Popover.tsx` with:
```tsx
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { api, Project } from "../api";
import { ProjectRow } from "./ProjectRow";
import { RowDetail } from "./RowDetail";

export function Popover() {
  const [recent, setRecent] = useState<Project[]>([]);
  const [all, setAll] = useState<Project[] | null>(null);
  const [query, setQuery] = useState("");
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [launchErr, setLaunchErr] = useState<string | null>(null);

  const loadRecent = useCallback(() => {
    api.recentActive(5).then(setRecent).catch(() => setRecent([]));
  }, []);

  useEffect(() => {
    loadRecent();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") invoke("hide_popover").catch(() => {});
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [loadRecent]);

  useEffect(() => {
    if (!query.trim()) {
      setAll(null);
      return;
    }
    api.listProjects(false).then(setAll).catch(() => setAll([]));
  }, [query]);

  const shown = useMemo(() => {
    if (!query.trim()) return recent;
    const q = query.trim().toLowerCase();
    return (all ?? []).filter(
      (p) =>
        p.name.toLowerCase().includes(q) ||
        p.path.toLowerCase().includes(q) ||
        (p.github_url ?? "").toLowerCase().includes(q) ||
        (p.notes ?? "").toLowerCase().includes(q),
    );
  }, [recent, all, query]);

  const onLaunch = useCallback(async (id: number) => {
    setLaunchErr(null);
    try {
      await api.startClaude(id);
      invoke("hide_popover").catch(() => {});
    } catch (e) {
      setLaunchErr(String(e));
    }
  }, []);

  const onView = useCallback(async (path: string) => {
    await api.openInFinder(path);
    invoke("hide_popover").catch(() => {});
  }, []);

  const onManage = useCallback(() => {
    invoke("hide_popover").catch(() => {});
    // `show_main_window` is added in Task 2.8; until then rely on the menu-bar "Open Library…" entry.
  }, []);

  return (
    <div className="h-screen w-screen overflow-hidden rounded-xl border border-zinc-800 bg-zinc-950 text-zinc-200 shadow-2xl">
      <header className="flex items-center gap-2 px-3 py-2 border-b border-zinc-800 text-[11px] uppercase tracking-wider text-zinc-500">
        <span className="h-2 w-2 rounded-full bg-emerald-500" />
        Claude Tracker · Recent 5
        <button onClick={onManage} className="ml-auto text-[11px] text-zinc-500 hover:text-zinc-200">
          Manage ↗
        </button>
      </header>
      <div className="flex-1 overflow-y-auto">
        {shown.length === 0 ? (
          <EmptyState hasQuery={Boolean(query.trim())} />
        ) : (
          shown.map((p) => (
            <div key={p.id}>
              <ProjectRow
                project={p}
                expanded={expandedId === p.id}
                onExpand={(id) => setExpandedId(expandedId === id ? null : id)}
                onLaunch={onLaunch}
                onView={onView}
              />
              {expandedId === p.id && <RowDetail project={p} mode="popover" />}
            </div>
          ))
        )}
      </div>
      {launchErr && (
        <div className="border-t border-red-800 bg-red-950/60 px-3 py-2 text-[11px] text-red-300">
          Launch failed: {launchErr}
        </div>
      )}
      <footer className="border-t border-zinc-800 p-2">
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search projects…"
          className="w-full rounded border border-zinc-800 bg-zinc-900 px-2 py-1 text-[12px] text-zinc-200 outline-none focus:border-zinc-600"
        />
      </footer>
    </div>
  );
}

function EmptyState({ hasQuery }: { hasQuery: boolean }) {
  return (
    <div className="p-8 text-center text-[12px] text-zinc-500">
      {hasQuery ? "no matches." : "no projects yet — start a Claude Code session."}
    </div>
  );
}
```

- [ ] **Step 2: Smoke-test**

Run: `npm run tauri dev`. Click tray icon. Expected: popover lists up to 5 recent projects. Clicking a row expands. Clicking Launch opens Ghostty and dismisses popover. Typing in search broadens to all projects.

- [ ] **Step 3: Commit**

```
git add tracker-app/src/components/Popover.tsx
git commit -m "feat(app): wire up popover body with ProjectRow + search"
```

### Task 2.8: `show_main_window` command + wire up "Manage ↗"

**Files:**
- Modify: `tracker-app/src-tauri/src/commands.rs`
- Modify: `tracker-app/src-tauri/src/lib.rs`
- Modify: `tracker-app/src/api.ts`
- Modify: `tracker-app/src/components/Popover.tsx`

- [ ] **Step 1: Add the command**

In `commands.rs`:
```rust
#[tauri::command]
pub fn show_main_window(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
        Ok(())
    } else {
        Err("main window missing".into())
    }
}
```

Register in `lib.rs`:
```rust
commands::show_main_window,
```

- [ ] **Step 2: Expose in `api.ts`**

```ts
showMainWindow: () => invoke<null>("show_main_window"),
```

- [ ] **Step 3: Wire in Popover**

In `Popover.tsx`, replace `onManage`:
```tsx
const onManage = useCallback(() => {
  api.showMainWindow().catch(() => {});
  invoke("hide_popover").catch(() => {});
}, []);
```

- [ ] **Step 4: Smoke-test**

Run: `npm run tauri dev`. Click Manage in popover → main window comes to the front.

- [ ] **Step 5: Commit**

```
git add tracker-app/src-tauri/src/commands.rs tracker-app/src-tauri/src/lib.rs tracker-app/src/api.ts tracker-app/src/components/Popover.tsx
git commit -m "feat(app): show_main_window command + Manage link in popover"
```

### Task 2.9: Hide main window on app boot

Popover is the primary surface. The main window should stay hidden until the user explicitly opens it.

**Files:**
- Modify: `tracker-app/src-tauri/tauri.conf.json`

- [ ] **Step 1: Set main window `visible: false`**

In the `main` window block:
```json
"visible": false
```

- [ ] **Step 2: Smoke-test**

Run: `npm run tauri dev`. Expected: app launches, tray icon appears, no window visible. Tray → popover works. Tray menu → "Open Library…" shows main window.

- [ ] **Step 3: Commit**

```
git add tracker-app/src-tauri/tauri.conf.json
git commit -m "chore(app): main window starts hidden (popover is primary)"
```

---

## Phase 3 · Global hotkey + keyboard nav

### Task 3.1: Add `tauri-plugin-global-shortcut`

**Files:**
- Modify: `tracker-app/src-tauri/Cargo.toml`
- Modify: `tracker-app/src-tauri/src/lib.rs`
- Modify: `tracker-app/src-tauri/capabilities/default.json` (if it exists; otherwise config lives in `tauri.conf.json`)

- [ ] **Step 1: Add the dep**

In `tracker-app/src-tauri/Cargo.toml` under `[dependencies]`:
```toml
tauri-plugin-global-shortcut = "2"
```

- [ ] **Step 2: Register the plugin**

In `lib.rs`, inside the `tauri::Builder::default()` chain, before `.setup(...)`:
```rust
.plugin(tauri_plugin_global_shortcut::Builder::new().build())
```

- [ ] **Step 3: Build**

Run: `cargo check -p tracker-app`
Expected: clean build.

- [ ] **Step 4: Commit**

```
git add tracker-app/src-tauri/Cargo.toml tracker-app/src-tauri/Cargo.lock tracker-app/src-tauri/src/lib.rs
git commit -m "feat(app): add tauri-plugin-global-shortcut"
```

### Task 3.2: Register `⌃⌥C` to toggle popover; persist in settings

**Files:**
- Modify: `tracker-app/src-tauri/src/lib.rs`
- Modify: `tracker-app/src-tauri/src/commands.rs`
- Modify: `tracker-app/src/api.ts`

- [ ] **Step 1: Add setting key and register at setup**

Extend `tracker-app/src-tauri/src/commands.rs` with the key constant (near `PREFERRED_TERMINAL_KEY`):
```rust
pub const LAUNCHER_HOTKEY_KEY: &str = "launcher_hotkey";
pub const DEFAULT_LAUNCHER_HOTKEY: &str = "Ctrl+Alt+KeyC";
```

Add registration helper in `lib.rs` (above `pub fn run`):
```rust
fn register_launcher_hotkey(
    app: &tauri::AppHandle,
    hotkey: &str,
) -> anyhow::Result<()> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let manager = app.global_shortcut();
    let _ = manager.unregister_all();
    let shortcut: tauri_plugin_global_shortcut::Shortcut = hotkey.parse()?;
    let app_handle = app.clone();
    manager.on_shortcut(shortcut, move |_app, _shortcut, _event| {
        let handle = app_handle.clone();
        tauri::async_runtime::spawn(async move {
            let _ = crate::commands::toggle_popover(handle);
        });
    })?;
    Ok(())
}
```

Call it in `.setup`:
```rust
let state = app.state::<Arc<AppState>>();
let stored = state
    .db
    .lock()
    .ok()
    .and_then(|db| db.get_setting(crate::commands::LAUNCHER_HOTKEY_KEY).ok().flatten());
let hotkey = stored.unwrap_or_else(|| crate::commands::DEFAULT_LAUNCHER_HOTKEY.to_string());
if let Err(e) = register_launcher_hotkey(&app.handle(), &hotkey) {
    tracing::warn!("failed to register global shortcut {hotkey}: {e:#}");
}
```

- [ ] **Step 2: Add get/set commands**

In `commands.rs`:
```rust
#[tauri::command]
pub fn get_launcher_hotkey(state: Shared<'_>) -> Result<String, String> {
    let db = state.db.lock().map_err(err)?;
    let stored = db.get_setting(LAUNCHER_HOTKEY_KEY).map_err(err)?;
    Ok(stored.unwrap_or_else(|| DEFAULT_LAUNCHER_HOTKEY.to_string()))
}

#[tauri::command]
pub fn set_launcher_hotkey(
    state: Shared<'_>,
    app: tauri::AppHandle,
    hotkey: String,
) -> Result<(), String> {
    let db = state.db.lock().map_err(err)?;
    db.set_setting(LAUNCHER_HOTKEY_KEY, &hotkey).map_err(err)?;
    drop(db);
    crate::register_launcher_hotkey(&app, &hotkey).map_err(err)
}
```

Register in `lib.rs` handler list:
```rust
commands::get_launcher_hotkey,
commands::set_launcher_hotkey,
```

Also make `register_launcher_hotkey` `pub(crate)` so `commands.rs` can call it.

- [ ] **Step 3: Expose in `api.ts`**

```ts
getLauncherHotkey: () => invoke<string>("get_launcher_hotkey"),
setLauncherHotkey: (hotkey: string) => invoke<null>("set_launcher_hotkey", { hotkey }),
```

- [ ] **Step 4: Smoke-test**

Run: `npm run tauri dev`. Press `Ctrl+Option+C` in any app. Expected: popover toggles.

- [ ] **Step 5: Commit**

```
git add tracker-app/src-tauri/src/lib.rs tracker-app/src-tauri/src/commands.rs tracker-app/src/api.ts
git commit -m "feat(app): register Ctrl+Alt+C hotkey to toggle popover; persist in settings"
```

### Task 3.3: Popover keyboard navigation

**Files:**
- Create: `tracker-app/src/hooks/useKeyboardNav.ts`
- Modify: `tracker-app/src/components/Popover.tsx`
- Create: `tracker-app/src/components/Popover.test.tsx`

- [ ] **Step 1: Write the failing test**

Create `tracker-app/src/components/Popover.test.tsx`:
```tsx
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { Popover } from "./Popover";

vi.mock("../api", async () => {
  const actual = await vi.importActual<typeof import("../api")>("../api");
  return {
    ...actual,
    api: {
      ...actual.api,
      recentActive: vi.fn().mockResolvedValue([
        { id: 1, path: "/tmp/a", name: "a", effective_status: "active",
          first_seen_at: "2026-01-01T00:00:00Z",
          last_active_at: new Date(Date.now() - 60_000).toISOString(),
          sessions_started: 1, prompts_count: 1,
          status: null, status_manual: false, github_url: null, deploy_url: null,
          deploy_platform: null, deploy_instructions: null, launch_instructions: null,
          deploy_live_lookup: false, notes: null, enrichment_synced_at: null,
          archived_at: null, description: null },
        { id: 2, path: "/tmp/b", name: "b", effective_status: "idle",
          first_seen_at: "2026-01-01T00:00:00Z",
          last_active_at: new Date(Date.now() - 3_600_000).toISOString(),
          sessions_started: 1, prompts_count: 1,
          status: null, status_manual: false, github_url: null, deploy_url: null,
          deploy_platform: null, deploy_instructions: null, launch_instructions: null,
          deploy_live_lookup: false, notes: null, enrichment_synced_at: null,
          archived_at: null, description: null },
      ]),
      startClaude: vi.fn().mockResolvedValue(null),
      openInFinder: vi.fn().mockResolvedValue(null),
      listProjects: vi.fn().mockResolvedValue([]),
      showMainWindow: vi.fn().mockResolvedValue(null),
    },
  };
});

describe("Popover keyboard nav", () => {
  it("Enter launches the selected row", async () => {
    const api = (await import("../api")).api;
    render(<Popover />);
    await waitFor(() => expect(screen.getByText("a")).toBeInTheDocument());
    fireEvent.keyDown(window, { key: "ArrowDown" });
    fireEvent.keyDown(window, { key: "Enter" });
    expect(api.startClaude).toHaveBeenCalledWith(2);
  });

  it("Space toggles expand on the selected row", async () => {
    render(<Popover />);
    await waitFor(() => expect(screen.getByText("a")).toBeInTheDocument());
    fireEvent.keyDown(window, { key: " " });
    expect(screen.getByText(/sessions/)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Confirm test fails**

Run: `cd tracker-app && npm test`
Expected: `Popover keyboard nav` tests fail (no keyboard handling yet).

- [ ] **Step 3: Add `useKeyboardNav` hook**

Create `tracker-app/src/hooks/useKeyboardNav.ts`:
```ts
import { useEffect, useState } from "react";

export function useKeyboardNav(
  count: number,
  onEnter: (index: number) => void,
  onCmdEnter: (index: number) => void,
  onSpace: (index: number) => void,
) {
  const [selected, setSelected] = useState(0);

  useEffect(() => {
    if (selected >= count && count > 0) setSelected(count - 1);
  }, [count, selected]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement)?.tagName === "INPUT") return;
      if (count === 0) return;
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelected((s) => Math.min(count - 1, s + 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelected((s) => Math.max(0, s - 1));
      } else if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        onCmdEnter(selected);
      } else if (e.key === "Enter") {
        e.preventDefault();
        onEnter(selected);
      } else if (e.key === " " || e.key === "Space") {
        e.preventDefault();
        onSpace(selected);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [count, selected, onEnter, onCmdEnter, onSpace]);

  return { selected, setSelected };
}
```

- [ ] **Step 4: Wire into Popover**

In `tracker-app/src/components/Popover.tsx`, replace the `return` body's list section to use selection. Import the hook:
```tsx
import { useKeyboardNav } from "../hooks/useKeyboardNav";
```

After the `shown` memo:
```tsx
const onEnter = useCallback((i: number) => {
  const p = shown[i];
  if (p) onLaunch(p.id);
}, [shown, onLaunch]);

const onCmdEnter = useCallback((i: number) => {
  const p = shown[i];
  if (p) onView(p.path);
}, [shown, onView]);

const onSpace = useCallback((i: number) => {
  const p = shown[i];
  if (p) setExpandedId((cur) => (cur === p.id ? null : p.id));
}, [shown]);

const { selected, setSelected } = useKeyboardNav(shown.length, onEnter, onCmdEnter, onSpace);
```

Change the list-item render so rows receive selection state:
```tsx
shown.map((p, i) => (
  <div
    key={p.id}
    onMouseEnter={() => setSelected(i)}
    className={i === selected ? "ring-1 ring-inset ring-emerald-500/40" : ""}
  >
    <ProjectRow
      project={p}
      expanded={expandedId === p.id}
      onExpand={(id) => setExpandedId(expandedId === id ? null : id)}
      onLaunch={onLaunch}
      onView={onView}
    />
    {expandedId === p.id && <RowDetail project={p} mode="popover" />}
  </div>
))
```

Also: `/` should focus the search input. Add a ref:
```tsx
const searchRef = useRef<HTMLInputElement>(null);
useEffect(() => {
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "/" && (e.target as HTMLElement)?.tagName !== "INPUT") {
      e.preventDefault();
      searchRef.current?.focus();
    }
  };
  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}, []);
```

And set `ref={searchRef}` on the search input.

- [ ] **Step 5: Run tests**

Run: `cd tracker-app && npm test`
Expected: keyboard-nav tests pass.

- [ ] **Step 6: Commit**

```
git add tracker-app/src/hooks/useKeyboardNav.ts tracker-app/src/components/Popover.tsx tracker-app/src/components/Popover.test.tsx
git commit -m "feat(app): popover keyboard nav (↑/↓/Enter/Space/⌘↵/slash)"
```

### Task 3.4: Accessibility-permission banner in main window

When `register_launcher_hotkey` errors on first launch (missing Accessibility permission), we should tell the user.

**Files:**
- Modify: `tracker-app/src-tauri/src/lib.rs`
- Modify: `tracker-app/src-tauri/src/commands.rs`
- Modify: `tracker-app/src/api.ts`
- Modify: `tracker-app/src/App.tsx`

- [ ] **Step 1: Capture registration failure into DB**

Above `pub fn run` in `lib.rs`, change the setup block to persist the last-attempt outcome:
```rust
let hotkey_outcome = match register_launcher_hotkey(&app.handle(), &hotkey) {
    Ok(()) => "ok".to_string(),
    Err(e) => {
        tracing::warn!("failed to register global shortcut {hotkey}: {e:#}");
        format!("err: {e:#}")
    }
};
if let Ok(db) = state.db.lock() {
    let _ = db.set_setting("launcher_hotkey_last_outcome", &hotkey_outcome);
}
```

- [ ] **Step 2: Add a command to read it**

In `commands.rs`:
```rust
#[tauri::command]
pub fn get_hotkey_status(state: Shared<'_>) -> Result<String, String> {
    let db = state.db.lock().map_err(err)?;
    Ok(db.get_setting("launcher_hotkey_last_outcome").map_err(err)?.unwrap_or_default())
}
```

Register:
```rust
commands::get_hotkey_status,
```

- [ ] **Step 3: Expose in `api.ts`**

```ts
getHotkeyStatus: () => invoke<string>("get_hotkey_status"),
```

- [ ] **Step 4: Banner in App.tsx**

In `tracker-app/src/App.tsx`, add in the `App()` component:
```tsx
const [hotkeyErr, setHotkeyErr] = useState<string | null>(null);
useEffect(() => {
  api.getHotkeyStatus().then((s) => {
    setHotkeyErr(s.startsWith("err:") ? s.slice(4).trim() : null);
  }).catch(() => {});
}, []);
```

Render (inside the main return, just above the `<header>`):
```tsx
{hotkeyErr && (
  <div className="border-b border-amber-800 bg-amber-950/60 px-4 py-2 text-[11px] text-amber-300">
    Global hotkey couldn't register ({hotkeyErr}). Grant Accessibility permission to
    {" "}
    <strong>Claude Tracker</strong> in System Settings → Privacy &amp; Security → Accessibility, then quit and relaunch.
  </div>
)}
```

- [ ] **Step 5: Smoke-test**

Revoke Accessibility permission (System Settings) and relaunch. Expected: banner shows. Grant permission, relaunch: banner is gone, hotkey works.

- [ ] **Step 6: Commit**

```
git add tracker-app/src-tauri/src/lib.rs tracker-app/src-tauri/src/commands.rs tracker-app/src/api.ts tracker-app/src/App.tsx
git commit -m "feat(app): surface Accessibility-permission banner when hotkey registration fails"
```

---

## Phase 4 · End-to-end verification

### Task 4.1: Documented manual-smoke script

**Files:**
- Create: `docs/superpowers/plans/2026-04-21-launcher-popover-smoke.md`

- [ ] **Step 1: Write the smoke-test doc**

```markdown
# Launcher popover smoke test

Run after each integration merge of Plan A. Human-driven; no automation.

## Prep
- Quit Ghostty: `osascript -e 'quit app "Ghostty"'`
- Remove the db to simulate a fresh install (optional): `rm ~/.claude-tracker/db.sqlite*`
- Have Accessibility permission granted to the dev build

## Steps

1. `cd tracker-app && npm run tauri dev`
2. Confirm a tray icon appears; no app window is visible.
3. Click the tray icon → popover appears under it. Close Ghostty again if it opened anywhere.
4. Popover shows up to 5 rows ordered newest first (or empty state if DB is fresh).
5. Click a row's left area → expands inline with GitHub + session counts.
6. Click the row's **Launch** button → Ghostty opens at the project path and runs `claude`. Popover dismisses.
7. Ghostty still open: click a different row's Launch → new window in the same Ghostty instance.
8. Click **View** → Finder opens to the path. Popover dismisses.
9. Press `Ctrl+Option+C` from any app → popover toggles.
10. Press `↓` then `Enter` in the popover → launches second row.
11. Press `Space` → expands/collapses selected row.
12. Press `⌘↵` → reveals in Finder.
13. Press `Esc` → popover dismisses.
14. Type `claude` in search → list broadens from recent-5 to all matching projects.
15. Click **Manage ↗** → main window comes forward.
16. Run: `sqlite3 ~/.claude-tracker/db.sqlite "SELECT outcome, terminal_slug, datetime(attempted_at), substr(message,1,60) FROM launches ORDER BY id DESC LIMIT 5;"`
    Expected: recent attempts recorded with outcomes matching observed behavior.
17. Run: `cat ~/.claude-tracker/logs/terminal.log | tail`
    Expected: either empty (Ok launches produce no stderr) or an error tail for any observed failures.

## Known-bad scenarios (should NOT regress)
- Quit Ghostty entirely, then Launch → should cold-start Ghostty via `open -na` (not silently do nothing).
- Project path deleted on disk → Launch returns an inline red banner "project path does not exist".
```

- [ ] **Step 2: Commit**

```
git add docs/superpowers/plans/2026-04-21-launcher-popover-smoke.md
git commit -m "docs: manual smoke-test script for launcher popover"
```

---

## Spec coverage (self-check)

| Spec section | Covered by |
|---|---|
| 2 Popover — presentation | Task 2.1, 2.2 |
| 2 Popover — content / rows | Task 2.5, 2.6 |
| 2 Popover — footer / Manage / Empty | Task 2.7, 2.8 |
| 2 Popover — keyboard | Task 3.3 |
| 4 Launch fix — stderr capture | Task 1.5 |
| 4 Launch fix — Ghostty probe | Task 1.6 |
| 4 Launch fix — outcome tracking | Task 1.1, 1.2, 1.7, 1.8 |
| 4 Launch fix — inline banner (existing) | covered by existing ProjectDetail banner; reused in popover in Task 2.7 |
| 5 Hotkey — default + persist | Task 3.1, 3.2 |
| 5 Hotkey — accessibility banner | Task 3.4 |
| §7 Phasing — smoke test | Task 1.9, 4.1 |

**Out of scope for Plan A (deferred to Plan B):**
- Section 3 Library window rethink (inline-expand table, filter pills, gear, "…" menu).
- Expanded-row editing (notes, deploy URL, status override).
- Restyle of Onboarding / FindProjects / ReleaseNotes modals.

Plan B will build on the shared `ProjectRow` + `RowDetail` introduced here.
