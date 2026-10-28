//! Creates a Claude Code profile: config folder, launcher command and widget entry.
//!
//! Mirrors `new-claude-profile.bat`. The `CLAUDE_PROFILE_ROOT`, `CLAUDE_BIN_DIR` and `CLAUDE_EXE`
//! environment variables override the default locations, as they do for the script.
use crate::{config, mcp};
use std::path::{Path, PathBuf};

const DEFAULT_SETTINGS: &str = r#"{
  "model": "sonnet",
  "modelSettings": {
    "claude-opus-5": {
      "effortLevel": "medium"
    }
  },
  "theme": "dark"
}
"#;

pub struct Created {
    pub dir: PathBuf,
    pub launcher: PathBuf,
    pub notes: Vec<String>,
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn bin_dir() -> PathBuf {
    env_path("CLAUDE_BIN_DIR").unwrap_or_else(|| config::home().join(".local").join("bin"))
}

fn profile_root() -> PathBuf {
    env_path("CLAUDE_PROFILE_ROOT").unwrap_or_else(|| config::home().join("claude-profiles"))
}

fn claude_exe() -> Option<PathBuf> {
    if let Some(p) = env_path("CLAUDE_EXE") {
        return p.is_file().then_some(p);
    }
    let default = bin_dir().join("claude.exe");
    if default.is_file() {
        return Some(default);
    }
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var).map(|d| d.join("claude.exe")).find(|p| p.is_file())
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.chars().all(|c| c == '.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn on_path(dir: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().trim_end_matches(['\\', '/']).to_ascii_lowercase();
    std::env::var_os("PATH")
        .is_some_and(|v| std::env::split_paths(&v).any(|p| norm(&p) == norm(dir)))
}

/// Creates the profile and switches on the given shared MCP servers for it. An existing folder,
/// `settings.json` or launcher is kept as it is.
pub fn create(name: &str, servers: &[String]) -> Result<Created, String> {
    if !valid_name(name) {
        return Err("name may only use letters, digits, '-', '_' and '.'".into());
    }
    let exe = claude_exe().ok_or("claude.exe not found (set CLAUDE_EXE or put it on PATH)")?;
    let dir = profile_root().join(name);
    let bin = bin_dir();
    let launcher = bin.join(format!("{name}.cmd"));
    let mut notes = Vec::new();

    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let settings = dir.join("settings.json");
    if !settings.exists() {
        std::fs::write(&settings, DEFAULT_SETTINGS).map_err(|e| format!("{}: {e}", settings.display()))?;
    }

    std::fs::create_dir_all(&bin).map_err(|e| format!("{}: {e}", bin.display()))?;
    if launcher.exists() {
        notes.push(format!("Launcher {} already exists, kept.", launcher.display()));
    } else {
        let text = format!(
            "@echo off\r\nsetlocal\r\nset \"CLAUDE_CONFIG_DIR={}\"\r\n\"{}\" %*\r\nendlocal\r\n",
            dir.display(),
            exe.display()
        );
        std::fs::write(&launcher, text).map_err(|e| format!("{}: {e}", launcher.display()))?;
    }
    if !on_path(&bin) {
        notes.push(format!("{} is not on PATH; add it to run '{name}' from a terminal.", bin.display()));
    }

    for server in servers {
        if let Err(e) = mcp::set_enabled(&dir, server, true) {
            notes.push(format!("MCP '{server}': {e}"));
        }
    }

    let mut cfg = config::load();
    if !cfg.profiles.iter().any(|p| p.name == name) {
        cfg.profiles.push(config::ProfileConfig {
            name: name.to_string(),
            command: Some(name.to_string()),
            config_dir: Some(dir.display().to_string()),
        });
        config::save(&cfg)?;
    }
    Ok(Created { dir, launcher, notes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(valid_name("claude-la"));
        assert!(valid_name("work_2.x"));
        assert!(!valid_name(""));
        assert!(!valid_name(".."));
        assert!(!valid_name("a b"));
        assert!(!valid_name("a\\b"));
    }

    #[test]
    fn creates_profile_launcher_and_widget_entry() {
        let _g = crate::mcp::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!("profile-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let exe = root.join("claude.exe");
        std::fs::write(&exe, "").unwrap();
        std::env::set_var("APPDATA", root.join("appdata"));
        std::env::set_var("CLAUDE_PROFILE_ROOT", root.join("profiles"));
        std::env::set_var("CLAUDE_BIN_DIR", root.join("bin"));
        std::env::set_var("CLAUDE_EXE", &exe);

        let made = create("claude-new", &[]).unwrap();
        assert!(made.dir.join("settings.json").is_file());
        let launcher = std::fs::read_to_string(&made.launcher).unwrap();
        assert!(launcher.contains(&format!("CLAUDE_CONFIG_DIR={}", made.dir.display())));
        assert!(launcher.contains("claude.exe\" %*"));

        let cfg = config::load();
        let p = cfg.profiles.iter().find(|p| p.name == "claude-new").unwrap();
        assert_eq!(config::resolve_dir(p), made.dir);

        // running again keeps everything and does not duplicate the widget entry
        std::fs::write(made.dir.join("settings.json"), "{\"theme\":\"light\"}").unwrap();
        let again = create("claude-new", &[]).unwrap();
        assert!(again.notes.iter().any(|n| n.contains("already exists")));
        assert_eq!(config::load().profiles.iter().filter(|p| p.name == "claude-new").count(), 1);
        assert_eq!(std::fs::read_to_string(made.dir.join("settings.json")).unwrap(), "{\"theme\":\"light\"}");

        assert!(create("../evil", &[]).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
