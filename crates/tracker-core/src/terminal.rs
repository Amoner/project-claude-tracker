//! Launch a terminal emulator at a project cwd and run a command in it.
//! macOS-only for v1 — non-macOS builds return an explicit error.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, bail, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminal {
    Ghostty,
    WezTerm,
    Alacritty,
    Kitty,
    TerminalApp,
}

const PRIORITY: &[Terminal] = &[
    Terminal::Ghostty,
    Terminal::WezTerm,
    Terminal::Alacritty,
    Terminal::Kitty,
    Terminal::TerminalApp,
];

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
        }
    }

    pub fn from_slug(s: &str) -> Option<Terminal> {
        match s {
            "ghostty" => Some(Terminal::Ghostty),
            "wezterm" => Some(Terminal::WezTerm),
            "alacritty" => Some(Terminal::Alacritty),
            "kitty" => Some(Terminal::Kitty),
            "terminal_app" => Some(Terminal::TerminalApp),
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
        }
    }

    pub fn is_installed(self) -> bool {
        match self {
            Terminal::TerminalApp => cfg!(target_os = "macos"),
            other => other.binary().is_some(),
        }
    }

    /// Locate the terminal's own CLI binary — `$PATH` first, then the macOS
    /// app bundle's `Contents/MacOS/<bin>`. Returns `None` for Terminal.app
    /// (which has no useful CLI; it's driven via AppleScript).
    fn binary(self) -> Option<PathBuf> {
        let (bin, bundle) = match self {
            Terminal::Ghostty => ("ghostty", "Ghostty.app"),
            Terminal::WezTerm => ("wezterm", "WezTerm.app"),
            Terminal::Alacritty => ("alacritty", "Alacritty.app"),
            Terminal::Kitty => ("kitty", "kitty.app"),
            Terminal::TerminalApp => return None,
        };
        locate_binary(bin, bundle)
    }

    /// Installed terminals in `PRIORITY` order.
    pub fn detect_all() -> Vec<Terminal> {
        PRIORITY.iter().copied().filter(|t| t.is_installed()).collect()
    }

    /// The best terminal to launch when the user hasn't set a preference.
    pub fn default_installed() -> Terminal {
        Self::detect_all()
            .into_iter()
            .next()
            .unwrap_or(Terminal::TerminalApp)
    }

    pub fn launch(self, cwd: &Path, cmd: &str) -> Result<()> {
        let command = self.build_command(cwd, cmd)?;
        spawn_detached(command)
    }

    fn build_command(self, cwd: &Path, cmd: &str) -> Result<Command> {
        if !cfg!(target_os = "macos") {
            bail!("terminal launch is only supported on macOS (v1)");
        }
        let cwd_str = cwd.to_string_lossy().into_owned();
        // Always go via a login + interactive shell so `claude` resolves
        // through the user's normal PATH (npm global, Homebrew, etc.).
        // .zprofile handles login-only setups; .zshrc covers interactive ones.
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        let c = match self {
            Terminal::Ghostty => {
                // `ghostty +new-window` is Ghostty's CLI action: it connects to
                // the running instance via IPC and exits without spawning a
                // second GUI process. `open -na` was forcing a new instance that
                // also opened its own default tab, producing 3 tabs total.
                //
                // `--command=STRING` is a single argv element that Ghostty
                // shell-splits itself; passing `-e SHELL ARGS...` as separate
                // elements caused extra tabs when the trailing args were
                // misinterpreted as independent +new-window flags.
                let bin = self
                    .binary()
                    .ok_or_else(|| anyhow!("ghostty binary not found"))?;
                let mut c = Command::new(bin);
                c.arg("+new-window")
                    .arg(format!("--working-directory={cwd_str}"))
                    .arg(format!("--command={shell} -l -i -c {cmd}"));
                c
            }
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
            Terminal::TerminalApp => {
                // Terminal.app has no "open in dir with command" flag; drive it
                // through AppleScript. Escape `\` and `"` in both the path and
                // the command so the script literal stays intact.
                let escaped_cwd = escape_applescript(&cwd_str);
                let escaped_cmd = escape_applescript(cmd);
                let script = format!(
                    r#"tell application "Terminal" to do script "cd \"{escaped_cwd}\" && {escaped_cmd}""#,
                );
                let mut c = Command::new("osascript");
                c.args(["-e", &script]);
                c
            }
        };
        Ok(c)
    }
}

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

fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn which_on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        paths.to_string_lossy().split(':').find_map(|p| {
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
        assert_eq!(p.first().copied(), Some(Terminal::Ghostty));
        assert_eq!(p.last().copied(), Some(Terminal::TerminalApp));
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
