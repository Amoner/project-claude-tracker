//! Cross-platform "open" helpers. Kept here so every caller — Tauri
//! commands, tray menu, CLI — routes through the same per-OS dispatch
//! instead of reinventing it.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

pub fn open_path(path: &Path) -> Result<()> {
    spawn_opener(path.as_os_str().to_string_lossy().as_ref())
}

pub fn open_url(url: &str) -> Result<()> {
    spawn_opener(url)
}

fn spawn_opener(target: &str) -> Result<()> {
    let mut cmd = platform_opener(target);
    cmd.spawn()
        .with_context(|| format!("spawning OS opener for {target}"))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn platform_opener(target: &str) -> Command {
    let mut cmd = Command::new("open");
    cmd.arg(target);
    cmd
}

#[cfg(target_os = "linux")]
fn platform_opener(target: &str) -> Command {
    let mut cmd = Command::new("xdg-open");
    cmd.arg(target);
    cmd
}

#[cfg(target_os = "windows")]
fn platform_opener(target: &str) -> Command {
    let mut cmd = Command::new("cmd");
    cmd.args(["/C", "start", "", target]);
    cmd
}
