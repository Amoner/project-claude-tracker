//! Deploy-platform detection (always on) and live deploy-URL lookup (opt-in).

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub enum Platform {
    Vercel,
    Netlify,
    Fly,
    Railway,
    GithubActions,
    Docker,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Vercel => "vercel",
            Platform::Netlify => "netlify",
            Platform::Fly => "fly",
            Platform::Railway => "railway",
            Platform::GithubActions => "github-actions",
            Platform::Docker => "docker",
        }
    }
}

/// Detect the most specific deploy platform from files on disk.
pub fn detect_platform(path: &Path) -> Option<Platform> {
    if path.join("vercel.json").exists() || path.join(".vercel").exists() {
        return Some(Platform::Vercel);
    }
    if path.join("netlify.toml").exists() {
        return Some(Platform::Netlify);
    }
    if path.join("fly.toml").exists() {
        return Some(Platform::Fly);
    }
    if path.join("railway.json").exists() || path.join("railway.toml").exists() {
        return Some(Platform::Railway);
    }
    let workflows = path.join(".github/workflows");
    if workflows.exists() {
        if let Ok(rd) = std::fs::read_dir(&workflows) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_lowercase();
                if name.contains("deploy") || name.contains("release") || name.contains("publish")
                {
                    return Some(Platform::GithubActions);
                }
            }
        }
    }
    if path.join("Dockerfile").exists() {
        return Some(Platform::Docker);
    }
    None
}

/// Run the platform's CLI to retrieve the live production URL. Each shell-out
/// has a hard timeout so we never hang Claude Code or the app.
pub fn live_lookup_url(platform: Platform, path: &Path) -> Option<String> {
    let timeout = Duration::from_secs(5);
    match platform {
        Platform::Vercel => vercel_url(path, timeout),
        Platform::Netlify => netlify_url(path, timeout),
        Platform::Fly => fly_url(path, timeout),
        _ => None,
    }
}

fn vercel_url(path: &Path, timeout: Duration) -> Option<String> {
    let mut cmd = Command::new("vercel");
    cmd.arg("ls").arg("--json").current_dir(path);
    let out = run_with_timeout(cmd, timeout)?;
    if !out.status.success() {
        return None;
    }
    let txt = String::from_utf8(out.stdout).ok()?;
    // Try to extract the first url-looking token from the JSON response.
    find_first_https_url(&txt)
}

fn netlify_url(path: &Path, timeout: Duration) -> Option<String> {
    let mut cmd = Command::new("netlify");
    cmd.arg("status").arg("--json").current_dir(path);
    let out = run_with_timeout(cmd, timeout)?;
    if !out.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    json.get("siteData")
        .and_then(|v| v.get("url"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            json.get("url")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
}

fn fly_url(path: &Path, timeout: Duration) -> Option<String> {
    let mut cmd = Command::new("fly");
    cmd.arg("status").arg("--json").current_dir(path);
    let out = run_with_timeout(cmd, timeout)?;
    if !out.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let hostname = json.get("Hostname").and_then(|v| v.as_str())?;
    Some(format!("https://{hostname}"))
}

/// Run a command and kill it if it doesn't finish within `timeout`.
/// Returns `None` on spawn failure, timeout, or wait error.
fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Option<std::process::Output> {
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    let mut child = cmd.spawn().ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    }
    child.wait_with_output().ok()
}

fn find_first_https_url(text: &str) -> Option<String> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"https://[a-zA-Z0-9./\-_]+").unwrap());
    re.find(text).map(|m| m.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_first_url() {
        let sample = r#"[{"url":"https://foo.vercel.app","created":123}]"#;
        assert_eq!(
            find_first_https_url(sample).unwrap(),
            "https://foo.vercel.app"
        );
    }

    #[test]
    fn detect_from_vercel_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("vercel.json"), "{}").unwrap();
        assert!(matches!(detect_platform(dir.path()), Some(Platform::Vercel)));
    }

    #[test]
    fn detect_from_github_workflow() {
        let dir = tempfile::tempdir().unwrap();
        let wf = dir.path().join(".github/workflows");
        std::fs::create_dir_all(&wf).unwrap();
        std::fs::write(wf.join("deploy.yml"), "").unwrap();
        assert!(matches!(
            detect_platform(dir.path()),
            Some(Platform::GithubActions)
        ));
    }
}
