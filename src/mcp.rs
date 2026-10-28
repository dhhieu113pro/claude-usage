//! Central MCP server list, synced into each profile's `.claude.json`.
//!
//! The central list lives next to the widget config. A server is "on" for a profile when its
//! name is present in that profile's `mcpServers`; toggling copies or removes the entry.
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone)]
pub struct Server {
    pub name: String,
    /// `stdio` or `http`.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// Keys we do not model (e.g. 	ools), kept so imported servers round-trip unchanged.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

#[derive(Serialize, Deserialize, Default)]
struct Store {
    #[serde(default)]
    servers: Vec<Server>,
}

fn store_path() -> PathBuf {
    crate::config::config_path().with_file_name("mcp.json")
}

pub fn list() -> Vec<Server> {
    std::fs::read_to_string(store_path())
        .ok()
        .and_then(|t| serde_json::from_str::<Store>(&t).ok())
        .map(|s| s.servers)
        .unwrap_or_default()
}

fn save_list(servers: Vec<Server>) -> Result<(), String> {
    let text = serde_json::to_string_pretty(&Store { servers }).map_err(|e| e.to_string())?;
    write_atomic(&store_path(), &text)
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

fn validate(s: &Server) -> Result<(), String> {
    if s.name.trim().is_empty() || s.name.chars().any(char::is_control) {
        return Err("name is required".into());
    }
    match s.kind.as_str() {
        "stdio" if s.command.trim().is_empty() => Err("command is required".into()),
        "stdio" => Ok(()),
        "http" if s.url.starts_with("http://") || s.url.starts_with("https://") => Ok(()),
        "http" => Err("url must start with http:// or https://".into()),
        _ => Err("type must be stdio or http".into()),
    }
}

/// The entry Claude Code expects under `mcpServers`.
fn to_claude(s: &Server) -> Value {
    let mut v = to_claude_base(s);
    for (k, x) in &s.extra {
        v[k] = x.clone();
    }
    v
}

fn to_claude_base(s: &Server) -> Value {
    if s.kind == "http" {
        let mut v = json!({ "type": "http", "url": s.url });
        if !s.headers.is_empty() {
            v["headers"] = json!(s.headers);
        }
        v
    } else {
        let mut v = json!({ "type": "stdio", "command": s.command, "args": s.args });
        if !s.env.is_empty() {
            v["env"] = json!(s.env);
        }
        v
    }
}

fn string_map(v: Option<&Value>) -> BTreeMap<String, String> {
    let Some(Value::Object(m)) = v else { return BTreeMap::new() };
    m.iter()
        .map(|(k, x)| (k.clone(), x.as_str().map(String::from).unwrap_or_else(|| x.to_string())))
        .collect()
}

/// Converts a profile's mcpServers entry; a missing 	ype means stdio.
fn from_claude(name: &str, v: &Value) -> Option<Server> {
    let obj = v.as_object()?;
    let kind = match obj.get("type").and_then(Value::as_str) {
        Some("http") => "http",
        Some("stdio") | None => "stdio",
        Some(_) => return None,
    };
    let known = ["type", "command", "args", "env", "url", "headers"];
    Some(Server {
        name: name.to_string(),
        kind: kind.into(),
        command: obj.get("command").and_then(Value::as_str).unwrap_or_default().into(),
        args: match obj.get("args") {
            Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
            _ => Vec::new(),
        },
        env: string_map(obj.get("env")),
        url: obj.get("url").and_then(Value::as_str).unwrap_or_default().into(),
        headers: string_map(obj.get("headers")),
        extra: obj.iter().filter(|(k, _)| !known.contains(&k.as_str())).map(|(k, x)| (k.clone(), x.clone())).collect(),
    })
}

/// Adds the profile's servers to the central list; names already in the list are kept as they
/// are. Returns the names that were added.
pub fn import(dir: &Path) -> Result<Vec<String>, String> {
    let root = read_profile(dir)?;
    let Some(Value::Object(found)) = root.get("mcpServers") else { return Ok(Vec::new()) };
    let mut servers = list();
    let mut added = Vec::new();
    for (name, v) in found {
        if servers.iter().any(|s| &s.name == name) {
            continue;
        }
        if let Some(s) = from_claude(name, v).filter(|s| validate(s).is_ok()) {
            servers.push(s);
            added.push(name.clone());
        }
    }
    if !added.is_empty() {
        save_list(servers)?;
    }
    Ok(added)
}

fn claude_json(dir: &Path) -> PathBuf {
    dir.join(".claude.json")
}

/// Reads a profile's `.claude.json`; a missing file is an empty object, a corrupt one is an error
/// so it is never overwritten.
fn read_profile(dir: &Path) -> Result<Map<String, Value>, String> {
    let path = claude_json(dir);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(m)) => Ok(m),
        _ => Err(format!("{} is not a valid JSON object", path.display())),
    }
}

fn write_profile(dir: &Path, root: Map<String, Value>) -> Result<(), String> {
    let text = serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| e.to_string())?;
    write_atomic(&claude_json(dir), &text)
}

/// Runs `f` on the profile's `mcpServers` map and saves the file if `f` returns true.
fn edit_profile(dir: &Path, f: impl FnOnce(&mut Map<String, Value>) -> bool) -> Result<(), String> {
    let mut root = read_profile(dir)?;
    let entry = root.entry("mcpServers").or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(servers) = entry else {
        return Err("mcpServers is not an object".into());
    };
    if f(servers) {
        write_profile(dir, root)?;
    }
    Ok(())
}

/// Names enabled in a profile, or the reason it cannot be read.
pub fn enabled(dir: &Path) -> Result<Vec<String>, String> {
    let root = read_profile(dir)?;
    Ok(match root.get("mcpServers") {
        Some(Value::Object(m)) => m.keys().cloned().collect(),
        _ => Vec::new(),
    })
}

pub fn set_enabled(dir: &Path, name: &str, on: bool) -> Result<(), String> {
    let server = list().into_iter().find(|s| s.name == name).ok_or("unknown server")?;
    edit_profile(dir, |m| {
        if on {
            m.insert(name.to_string(), to_claude(&server));
        } else {
            m.shift_remove(name);
        }
        true
    })
}

/// Creates or updates a server. `original` is the previous name when editing, so a rename moves
/// the entry in every profile that has it enabled.
pub fn upsert(original: Option<&str>, server: Server, profile_dirs: &[PathBuf]) -> Result<(), String> {
    validate(&server)?;
    let mut servers = list();
    let old = original.unwrap_or(&server.name);
    let exists = servers.iter().any(|s| s.name == server.name);
    if exists && (original.is_none() || server.name != old) {
        return Err(format!("'{}' already exists", server.name));
    }
    match servers.iter_mut().find(|s| s.name == old) {
        Some(slot) => *slot = server.clone(),
        None if original.is_some() => return Err("unknown server".into()),
        None => servers.push(server.clone()),
    }
    save_list(servers)?;
    for dir in profile_dirs {
        // Profiles that are unreadable are skipped here; the UI surfaces their error.
        let _ = edit_profile(dir, |m| {
            if !m.contains_key(old) {
                return false;
            }
            m.shift_remove(old);
            m.insert(server.name.clone(), to_claude(&server));
            true
        });
    }
    Ok(())
}

/// Deletes a server from the list and from every profile that has it enabled.
pub fn delete(name: &str, profile_dirs: &[PathBuf]) -> Result<(), String> {
    let mut servers = list();
    servers.retain(|s| s.name != name);
    save_list(servers)?;
    for dir in profile_dirs {
        let _ = edit_profile(dir, |m| m.shift_remove(name).is_some());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // The store path comes from APPDATA, which is process-wide, so tests run one at a time.
    static LOCK: Mutex<()> = Mutex::new(());

    struct Env {
        root: PathBuf,
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl Env {
        fn new(tag: &str) -> Env {
            let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let root = std::env::temp_dir().join(format!("mcp-test-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            std::env::set_var("APPDATA", root.join("appdata"));
            Env { root, _guard: guard }
        }

        fn profile(&self, name: &str) -> PathBuf {
            let dir = self.root.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            dir
        }
    }

    impl Drop for Env {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn stdio(name: &str) -> Server {
        Server {
            name: name.into(),
            kind: "stdio".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "pkg".into()],
            env: BTreeMap::from([("TOKEN".to_string(), "abc".to_string())]),
            url: String::new(),
            headers: BTreeMap::new(),
            extra: Map::new(),
        }
    }

    fn http(name: &str) -> Server {
        Server {
            name: name.into(),
            kind: "http".into(),
            command: String::new(),
            args: vec![],
            env: BTreeMap::new(),
            url: "https://example.com/mcp".into(),
            headers: BTreeMap::from([("Authorization".to_string(), "Bearer x".to_string())]),
            extra: Map::new(),
        }
    }

    fn read(dir: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(claude_json(dir)).unwrap()).unwrap()
    }

    #[test]
    fn validate_rejects_bad_input() {
        assert!(validate(&stdio("ok_name-1")).is_ok());
        assert!(validate(&http("remote")).is_ok());
        assert!(validate(&stdio("")).is_err());
        assert!(validate(&stdio("Bravo - mssql-mcp")).is_ok());
        assert!(validate(&stdio("  ")).is_err());
        let mut s = stdio("a");
        s.command = "  ".into();
        assert!(validate(&s).is_err());
        let mut h = http("a");
        h.url = "ftp://x".into();
        assert!(validate(&h).is_err());
        let mut t = stdio("a");
        t.kind = "sse".into();
        assert!(validate(&t).is_err());
    }

    #[test]
    fn claude_entry_shapes() {
        assert_eq!(
            to_claude(&stdio("a")),
            json!({ "type": "stdio", "command": "npx", "args": ["-y", "pkg"], "env": { "TOKEN": "abc" } })
        );
        let mut s = stdio("a");
        s.env.clear();
        assert!(to_claude(&s).get("env").is_none());
        assert_eq!(
            to_claude(&http("a")),
            json!({ "type": "http", "url": "https://example.com/mcp", "headers": { "Authorization": "Bearer x" } })
        );
    }

    #[test]
    fn toggle_keeps_other_keys_and_order() {
        let env = Env::new("toggle");
        let dir = env.profile("p1");
        std::fs::write(
            claude_json(&dir),
            r#"{"zeta":1,"mcpServers":{"mine":{"type":"stdio","command":"x"}},"alpha":2}"#,
        )
        .unwrap();
        upsert(None, stdio("srv"), &[dir.clone()]).unwrap();

        set_enabled(&dir, "srv", true).unwrap();
        let v = read(&dir);
        let keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["zeta", "mcpServers", "alpha"]);
        assert!(v["mcpServers"]["mine"].is_object());
        assert_eq!(v["mcpServers"]["srv"]["command"], "npx");
        assert_eq!(enabled(&dir).unwrap(), ["mine", "srv"]);

        set_enabled(&dir, "srv", false).unwrap();
        assert_eq!(enabled(&dir).unwrap(), ["mine"]);
    }

    #[test]
    fn enable_creates_missing_claude_json() {
        let env = Env::new("create");
        let dir = env.profile("fresh");
        upsert(None, http("remote"), &[]).unwrap();
        assert_eq!(enabled(&dir).unwrap(), Vec::<String>::new());
        set_enabled(&dir, "remote", true).unwrap();
        assert_eq!(read(&dir)["mcpServers"]["remote"]["url"], "https://example.com/mcp");
    }

    #[test]
    fn unknown_server_cannot_be_enabled() {
        let env = Env::new("unknown");
        let dir = env.profile("p");
        assert!(set_enabled(&dir, "nope", true).is_err());
        assert!(!claude_json(&dir).exists());
    }

    #[test]
    fn corrupt_claude_json_is_not_overwritten() {
        let env = Env::new("corrupt");
        let dir = env.profile("bad");
        std::fs::write(claude_json(&dir), "{ not json").unwrap();
        upsert(None, stdio("srv"), &[]).unwrap();
        assert!(enabled(&dir).is_err());
        assert!(set_enabled(&dir, "srv", true).is_err());
        assert_eq!(std::fs::read_to_string(claude_json(&dir)).unwrap(), "{ not json");
    }

    #[test]
    fn create_rejects_duplicate_name() {
        let _env = Env::new("dup");
        upsert(None, stdio("a"), &[]).unwrap();
        assert!(upsert(None, stdio("a"), &[]).is_err());
        upsert(None, stdio("b"), &[]).unwrap();
        assert!(upsert(Some("b"), stdio("a"), &[]).is_err());
        assert_eq!(list().len(), 2);
    }

    #[test]
    fn edit_and_rename_propagate_to_enabled_profiles_only() {
        let env = Env::new("rename");
        let (on, off) = (env.profile("on"), env.profile("off"));
        upsert(None, stdio("old"), &[on.clone(), off.clone()]).unwrap();
        set_enabled(&on, "old", true).unwrap();

        let mut renamed = stdio("new");
        renamed.command = "uvx".into();
        upsert(Some("old"), renamed, &[on.clone(), off.clone()]).unwrap();

        assert_eq!(list().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["new"]);
        assert_eq!(enabled(&on).unwrap(), ["new"]);
        assert_eq!(read(&on)["mcpServers"]["new"]["command"], "uvx");
        assert!(enabled(&off).unwrap().is_empty());
    }

    #[test]
    fn delete_removes_everywhere_but_keeps_unmanaged() {
        let env = Env::new("delete");
        let dir = env.profile("p");
        std::fs::write(claude_json(&dir), r#"{"mcpServers":{"mine":{"type":"stdio","command":"x"}}}"#).unwrap();
        upsert(None, stdio("srv"), &[dir.clone()]).unwrap();
        set_enabled(&dir, "srv", true).unwrap();

        delete("srv", &[dir.clone()]).unwrap();
        assert!(list().is_empty());
        assert_eq!(enabled(&dir).unwrap(), ["mine"]);
    }

    #[test]
    fn import_reads_profile_servers_and_keeps_extra_keys() {
        let env = Env::new("import");
        let src = env.profile("src");
        let dst = env.profile("dst");
        std::fs::write(
            claude_json(&src),
            r#"{"mcpServers":{
                "Bravo - mssql-mcp":{"tools":["a","b"],"type":"stdio","command":"dotnet","args":["x"],"env":{"K":"v"}},
                "legacy":{"command":"node","args":["s.js"]},
                "remote":{"type":"http","url":"https://e.com/mcp"},
                "weird":{"type":"sse","url":"https://e.com/sse"}}}"#,
        )
        .unwrap();
        upsert(None, stdio("legacy"), &[]).unwrap();

        let added = import(&src).unwrap();
        assert_eq!(added, ["Bravo - mssql-mcp", "remote"]);
        assert_eq!(import(&src).unwrap(), Vec::<String>::new());

        // existing 'legacy' untouched, untyped entries import as stdio, sse is skipped
        let all = list();
        assert_eq!(all.len(), 3);
        assert_eq!(all.iter().find(|s| s.name == "legacy").unwrap().command, "npx");

        // extra keys survive a round trip into another profile
        set_enabled(&dst, "Bravo - mssql-mcp", true).unwrap();
        let entry = &read(&dst)["mcpServers"]["Bravo - mssql-mcp"];
        assert_eq!(entry["tools"], json!(["a", "b"]));
        assert_eq!(entry["env"]["K"], "v");
    }
}