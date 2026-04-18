//! Launch a terminal emulator at a project cwd and run a command in it.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, bail, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminal {
    // macOS
    Ghostty,
    TerminalApp,
    // Cross-platform
    WezTerm,
    Alacritty,
    Kitty,
    // Windows
    WindowsTerminal,
    PowerShell,
    Cmd,
}

#[cfg(target_os = "macos")]
const PRIORITY: &[Terminal] = &[
    Terminal::Ghostty,
    Terminal::WezTerm,
    Terminal::Alacritty,
    Terminal::Kitty,
    Terminal::TerminalApp,
];

#[cfg(target_os = "windows")]
const PRIORITY: &[Terminal] = &[
    Terminal::WindowsTerminal,
    Terminal::WezTerm,
    Terminal::Alacritty,
    Terminal::PowerShell,
    Terminal::Cmd,
];

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const PRIORITY: &[Terminal] = &[];

impl Terminal {
    pub fn priority() -> &'static [Terminal] {
        PRIORITY
    }

    pub fn slug(self) -> &'static str {
        match self {
            Terminal::Ghostty => "ghostty",
            Terminal::WezTerm => "wezterm",
            Terminal::Alacritty => "alacritty",
            Terminal::Kitty => "kitty",
            Terminal::TerminalApp => "terminal_app",
            Terminal::WindowsTerminal => "windows_terminal",
            Terminal::PowerShell => "powershell",
            Terminal::Cmd => "cmd",
        }
    }

    pub fn from_slug(s: &str) -> Option<Terminal> {
        match s {
            "ghostty" => Some(Terminal::Ghostty),
            "wezterm" => Some(Terminal::WezTerm),
            "alacritty" => Some(Terminal::Alacritty),
            "kitty" => Some(Terminal::Kitty),
            "terminal_app" => Some(Terminal::TerminalApp),
            "windows_terminal" => Some(Terminal::WindowsTerminal),
            "powershell" => Some(Terminal::PowerShell),
            "cmd" => Some(Terminal::Cmd),
            _ => None,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Terminal::Ghostty => "Ghostty",
            Terminal::WezTerm => "WezTerm",
            Terminal::Alacritty => "Alacritty",
            Terminal::Kitty => "kitty",
            Terminal::TerminalApp => "Terminal.app",
            Terminal::WindowsTerminal => "Windows Terminal",
            Terminal::PowerShell => "PowerShell",
            Terminal::Cmd => "Command Prompt",
        }
    }

    pub fn is_installed(self) -> bool {
        match self {
            Terminal::TerminalApp => cfg!(target_os = "macos"),
            // cmd.exe and PowerShell are always present on Windows
            Terminal::Cmd | Terminal::PowerShell => cfg!(target_os = "windows"),
            other => other.binary().is_some(),
        }
    }

    fn binary(self) -> Option<PathBuf> {
        match self {
            // Driven via AppleScript / built-in shell, no useful CLI binary
            Terminal::TerminalApp | Terminal::Cmd => None,
            #[cfg(target_os = "macos")]
            Terminal::Ghostty => locate_binary("ghostty", "Ghostty.app"),
            #[cfg(target_os = "macos")]
            Terminal::WezTerm => locate_binary("wezterm", "WezTerm.app"),
            #[cfg(target_os = "macos")]
            Terminal::Alacritty => locate_binary("alacritty", "Alacritty.app"),
            #[cfg(target_os = "macos")]
            Terminal::Kitty => locate_binary("kitty", "kitty.app"),
            #[cfg(target_os = "windows")]
            Terminal::WezTerm => which_on_path("wezterm.exe"),
            #[cfg(target_os = "windows")]
            Terminal::Alacritty => which_on_path("alacritty.exe"),
            #[cfg(target_os = "windows")]
            Terminal::WindowsTerminal => which_on_path("wt.exe"),
            #[cfg(target_os = "windows")]
            Terminal::PowerShell => which_on_path("powershell.exe"),
            #[allow(unreachable_patterns)]
            _ => None,
        }
    }

    /// Installed terminals in `PRIORITY` order.
    pub fn detect_all() -> Vec<Terminal> {
        PRIORITY.iter().copied().filter(|t| t.is_installed()).collect()
    }

    /// The best terminal to launch when the user hasn't set a preference.
    pub fn default_installed() -> Terminal {
        Self::detect_all().into_iter().next().unwrap_or_else(|| {
            if cfg!(target_os = "windows") {
                Terminal::Cmd
            } else {
                Terminal::TerminalApp
            }
        })
    }

    pub fn launch(self, cwd: &Path, cmd: &str) -> Result<()> {
        let command = self.build_command(cwd, cmd)?;
        spawn_detached(command)
    }

    fn build_command(self, cwd: &Path, cmd: &str) -> Result<Command> {
        let cwd_str = cwd.to_string_lossy().into_owned();

        // macOS: go via a login + interactive shell so `claude` resolves through
        // the user's normal PATH (.zprofile for login-only, .zshrc for interactive).
        #[cfg(target_os = "macos")]
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

        let c = match self {
            #[cfg(target_os = "macos")]
            Terminal::Ghostty => {
                // `ghostty +new-window` is Ghostty's CLI action: connects to the
                // running instance via IPC and exits without spawning a second GUI.
                // `--command=STRING` is a single argv element that Ghostty
                // shell-splits itself, avoiding the extra-tabs issue we saw when
                // passing `-e SHELL ARGS...` as separate elements.
                let bin = self
                    .binary()
                    .ok_or_else(|| anyhow!("ghostty binary not found"))?;
                let mut c = Command::new(bin);
                c.arg("+new-window")
                    .arg(format!("--working-directory={cwd_str}"))
                    .arg(format!("--command={shell} -l -i -c {cmd}"));
                c
            }
            #[cfg(target_os = "macos")]
            Terminal::WezTerm => {
                let bin = self
                    .binary()
                    .ok_or_else(|| anyhow!("wezterm binary not found"))?;
                let mut c = Command::new(bin);
                c.args(["start", "--cwd"])
                    .arg(&cwd_str)
                    .arg("--")
                    .arg(&shell)
                    .args(["-l", "-i", "-c", cmd]);
                c
            }
            #[cfg(target_os = "macos")]
            Terminal::Alacritty => {
                let bin = self
                    .binary()
                    .ok_or_else(|| anyhow!("alacritty binary not found"))?;
                let mut c = Command::new(bin);
                c.args(["--working-directory", &cwd_str, "-e"])
                    .arg(&shell)
                    .args(["-l", "-i", "-c", cmd]);
                c
            }
            #[cfg(target_os = "macos")]
            Terminal::Kitty => {
                let bin = self
                    .binary()
                    .ok_or_else(|| anyhow!("kitty binary not found"))?;
                let mut c = Command::new(bin);
                c.arg(format!("--directory={cwd_str}"))
                    .arg(&shell)
                    .args(["-l", "-i", "-c", cmd]);
                c
            }
            #[cfg(target_os = "macos")]
            Terminal::TerminalApp => {
                let escaped_cwd = escape_applescript(&cwd_str);
                let escaped_cmd = escape_applescript(cmd);
                let script = format!(
                    r#"tell application "Terminal" to do script "cd \"{escaped_cwd}\" && {escaped_cmd}""#,
                );
                let mut c = Command::new("osascript");
                c.args(["-e", &script]);
                c
            }
            #[cfg(target_os = "windows")]
            Terminal::WindowsTerminal => {
                let bin = self
                    .binary()
                    .ok_or_else(|| anyhow!("Windows Terminal (wt.exe) not found"))?;
                let mut c = Command::new(bin);
                // `--window new` ensures a fresh window rather than a new tab in
                // whatever window happens to be focused.
                c.args([
                    "--window",
                    "new",
                    "new-tab",
                    "--startingDirectory",
                    &cwd_str,
                    "--",
                    "powershell.exe",
                    "-NoExit",
                    "-Command",
                    cmd,
                ]);
                c
            }
            #[cfg(target_os = "windows")]
            Terminal::WezTerm => {
                let bin = self
                    .binary()
                    .ok_or_else(|| anyhow!("wezterm binary not found"))?;
                let mut c = Command::new(bin);
                c.args(["start", "--cwd", &cwd_str, "--", "powershell.exe", "-NoExit", "-Command", cmd]);
                c
            }
            #[cfg(target_os = "windows")]
            Terminal::Alacritty => {
                let bin = self
                    .binary()
                    .ok_or_else(|| anyhow!("alacritty binary not found"))?;
                let mut c = Command::new(bin);
                c.args([
                    "--working-directory",
                    &cwd_str,
                    "-e",
                    "powershell.exe",
                    "-NoExit",
                    "-Command",
                    cmd,
                ]);
                c
            }
            #[cfg(target_os = "windows")]
            Terminal::PowerShell => {
                // Single-quote the path and escape embedded single-quotes.
                let safe_cwd = cwd_str.replace('\'', "''");
                let ps_cmd = format!("Set-Location '{safe_cwd}'; {cmd}");
                let mut c = Command::new("powershell.exe");
                c.args(["-NoExit", "-Command", &ps_cmd]);
                c
            }
            #[cfg(target_os = "windows")]
            Terminal::Cmd => {
                let batch_cmd = format!("cd /d \"{cwd_str}\" && {cmd}");
                let mut c = Command::new("cmd.exe");
                c.args(["/K", &batch_cmd]);
                c
            }
            #[allow(unreachable_patterns)]
            _ => bail!("terminal {:?} is not supported on this platform", self),
        };
        Ok(c)
    }
}

#[cfg(target_os = "macos")]
fn locate_binary(bin: &str, app_bundle: &str) -> Option<PathBuf> {
    if let Some(p) = which_on_path(bin) {
        return Some(p);
    }
    let rel = format!("{app_bundle}/Contents/MacOS/{bin}");
    let sys = Path::new("/Applications").join(&rel);
    if sys.is_file() {
        return Some(sys);
    }
    if let Some(home) = dirs::home_dir() {
        let user = home.join("Applications").join(&rel);
        if user.is_file() {
            return Some(user);
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn which_on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        let sep = if cfg!(target_os = "windows") { ';' } else { ':' };
        paths.to_string_lossy().split(sep).find_map(|p| {
            let candidate = Path::new(p).join(name);
            candidate.is_file().then_some(candidate)
        })
    })
}

fn spawn_detached(mut cmd: Command) -> Result<()> {
    cmd.stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null());
    cmd.spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_roundtrip() {
        for t in Terminal::priority() {
            assert_eq!(Terminal::from_slug(t.slug()), Some(*t));
        }
    }

    #[test]
    fn priority_order_stable() {
        let p = Terminal::priority();
        #[cfg(target_os = "macos")]
        {
            assert_eq!(p.first().copied(), Some(Terminal::Ghostty));
            assert_eq!(p.last().copied(), Some(Terminal::TerminalApp));
        }
        #[cfg(target_os = "windows")]
        {
            assert_eq!(p.first().copied(), Some(Terminal::WindowsTerminal));
            assert_eq!(p.last().copied(), Some(Terminal::Cmd));
        }
    }

    #[test]
    fn unknown_slug_is_none() {
        assert!(Terminal::from_slug("iterm").is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn terminal_app_always_detected_on_macos() {
        assert!(Terminal::TerminalApp.is_installed());
        let detected = Terminal::detect_all();
        assert!(detected.contains(&Terminal::TerminalApp));
        assert!(!detected.is_empty());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn cmd_always_detected_on_windows() {
        assert!(Terminal::Cmd.is_installed());
        assert!(Terminal::PowerShell.is_installed());
        let detected = Terminal::detect_all();
        assert!(detected.contains(&Terminal::Cmd));
    }

    #[cfg(target_os = "macos")]
    fn args_of(cmd: &Command) -> Vec<String> {
        cmd.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn build_command_ghostty() {
        if !Terminal::Ghostty.is_installed() {
            return;
        }
        let cmd = Terminal::Ghostty
            .build_command(Path::new("/tmp/x"), "claude")
            .unwrap();
        let prog = cmd.get_program().to_string_lossy().into_owned();
        assert!(prog.contains("ghostty"), "expected ghostty binary, got: {prog}");
        let args = args_of(&cmd);
        assert!(args.iter().any(|a| a == "+new-window"), "missing +new-window");
        assert!(
            args.iter().any(|a| a == "--working-directory=/tmp/x"),
            "missing --working-directory"
        );
        let command_arg = args
            .iter()
            .find(|a| a.starts_with("--command="))
            .expect("--command= arg missing");
        assert!(
            command_arg.ends_with(" -l -i -c claude"),
            "unexpected --command value: {command_arg}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn build_command_wezterm() {
        if !Terminal::WezTerm.is_installed() {
            return;
        }
        let cmd = Terminal::WezTerm
            .build_command(Path::new("/tmp/y"), "claude")
            .unwrap();
        let args = args_of(&cmd);
        assert!(args.iter().any(|a| a == "start"));
        assert!(args.iter().any(|a| a == "--cwd"));
        assert!(args.iter().any(|a| a == "/tmp/y"));
        assert_eq!(args.last().map(String::as_str), Some("claude"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn build_command_terminal_app_escapes_cwd() {
        let cmd = Terminal::TerminalApp
            .build_command(Path::new("/tmp/has \"quote\""), "claude")
            .unwrap();
        let args = args_of(&cmd);
        assert_eq!(cmd.get_program(), "osascript");
        let script = &args[1];
        assert!(script.contains("\\\"quote\\\""));
        assert!(script.contains("claude"));
    }
}
