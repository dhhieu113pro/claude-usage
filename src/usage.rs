use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const TIMEOUT: Duration = Duration::from_secs(60);

/// Percent *used* (0..=100) for the 5-hour session and the 7-day window.
pub struct Usage {
    pub session_used: Option<f32>,
    pub weekly_used: Option<f32>,
    /// Date the weekly window resets, as printed by `/usage`, e.g. "Oct 29".
    pub weekly_reset: Option<String>,
}

/// Runs `claude -p "/usage"` against the profile's config dir and parses the text output.
pub fn fetch(config_dir: &Path) -> Result<Usage, String> {
    let mut child = Command::new("claude")
        .args(["-p", "/usage"])
        .env("CLAUDE_CONFIG_DIR", config_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|_| "no claude".to_string())?;

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > TIMEOUT => {
                let _ = child.kill();
                return Err("timeout".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(_) => return Err("bad output".into()),
        }
    }
    let out = child.wait_with_output().map_err(|_| "bad output".to_string())?;
    parse(&String::from_utf8_lossy(&out.stdout))
}

fn parse(text: &str) -> Result<Usage, String> {
    let mut session = None;
    let mut weekly = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("Current session:") {
            session = Some(rest);
        } else if let Some(rest) = line.strip_prefix("Current week (all models):") {
            weekly = Some(rest);
        }
    }

    if session.is_none() && weekly.is_none() {
        let lower = text.to_lowercase();
        return Err(if lower.contains("expired") || lower.contains("/login") {
            "token expired".into()
        } else {
            "bad output".into()
        });
    }
    Ok(Usage {
        session_used: session.and_then(percent),
        weekly_used: weekly.and_then(percent),
        weekly_reset: weekly.and_then(reset_date),
    })
}

/// " 93% used · resets Oct 28, 9pm (Asia/Bangkok)" -> 93.0
fn percent(s: &str) -> Option<f32> {
    s.split_once('%')?.0.trim().parse().ok()
}

/// " 93% used · resets Oct 28, 9pm (Asia/Bangkok)" -> "Oct 28"
fn reset_date(s: &str) -> Option<String> {
    let after = s.split_once("resets ")?.1;
    let date = after.split([',', '(']).next()?.trim();
    (!date.is_empty()).then(|| date.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_usage_output() {
        let text = "Current session: 0% used · resets Oct 27, 2:20pm (Asia/Bangkok)\nCurrent week (all models): 93% used · resets Oct 28, 9pm (Asia/Bangkok)\n";
        let u = parse(text).unwrap();
        assert_eq!(u.session_used, Some(0.0));
        assert_eq!(u.weekly_used, Some(93.0));
        assert_eq!(u.weekly_reset.as_deref(), Some("Oct 28"));
    }

    #[test]
    fn rejects_unknown_output() {
        assert_eq!(parse("hello").err().as_deref(), Some("bad output"));
    }
}
