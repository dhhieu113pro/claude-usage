//! Self-update: find the latest GitHub release, download it, verify its SHA-256
//! and hand the final replace-and-relaunch to a small PowerShell helper.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Output};

pub const REPO: &str = "dhhieu113pro/rings-for-claude-code";

#[derive(Debug)]
pub struct Release {
    pub tag: String,
    pub zip_url: String,
    pub sha_url: String,
}

/// Release asset suffix for the running architecture.
pub fn arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "win-arm64"
    } else {
        "win-x64"
    }
}

/// "v1.2.3" / "1.2" -> (1, 2, 3) / (1, 2, 0); anything non-numeric is None.
pub fn parse_version(tag: &str) -> Option<(u32, u32, u32)> {
    let t = tag.trim().trim_start_matches(['v', 'V']);
    let mut parts = t.split('.');
    let mut next = |required: bool| match parts.next() {
        Some(p) => p.parse::<u32>().ok(),
        None if required => None,
        None => Some(0),
    };
    let v = (next(true)?, next(false)?, next(false)?);
    parts.next().is_none().then_some(v)
}

pub fn is_newer(latest_tag: &str, current: &str) -> bool {
    match (parse_version(latest_tag), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, b) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

/// First token of a `<hex>  <name>` checksum file, lowercased; None unless 64 hex chars.
pub fn parse_sha_file(text: &str) -> Option<String> {
    let tok = text.split_whitespace().next()?;
    (tok.len() == 64 && tok.bytes().all(|b| b.is_ascii_hexdigit())).then(|| tok.to_ascii_lowercase())
}

/// Reads the GitHub "latest release" JSON and picks this architecture's zip + checksum assets.
pub fn parse_release(json: &str, arch: &str) -> Result<Release, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("Unexpected response from GitHub: {e}"))?;
    let tag = v["tag_name"]
        .as_str()
        .ok_or("GitHub did not return a release (rate limited or no releases yet).")?
        .to_string();
    let zip_name = format!("claude-usage-widget-{tag}-{arch}.zip");
    let sha_name = format!("{zip_name}.sha256");
    let find = |name: &str| {
        v["assets"].as_array().and_then(|a| {
            a.iter()
                .find(|x| x["name"].as_str() == Some(name))
                .and_then(|x| x["browser_download_url"].as_str())
                .map(str::to_string)
        })
    };
    match (find(&zip_name), find(&sha_name)) {
        (Some(zip_url), Some(sha_url)) => Ok(Release { tag, zip_url, sha_url }),
        _ => Err(format!("Release {tag} has no {arch} download.")),
    }
}

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn run(cmd: &mut Command) -> Result<Output, String> {
    let out = cmd
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Could not run {}: {e}", cmd.get_program().to_string_lossy()))?;
    if out.status.success() {
        Ok(out)
    } else {
        Err(format!(
            "{} failed: {}",
            cmd.get_program().to_string_lossy(),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn curl() -> Command {
    let mut c = Command::new("curl.exe");
    c.args(["-sSL", "--fail", "-H", "User-Agent: claude-usage-widget"]);
    c
}

pub fn fetch_latest() -> Result<Release, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let out = run(curl().arg(url)).map_err(|e| format!("Could not reach GitHub. {e}"))?;
    parse_release(&String::from_utf8_lossy(&out.stdout), arch())
}

pub struct Ready {
    pub dir: PathBuf,
    pub new_exe: PathBuf,
}

/// Ok when the zip's SHA-256 matches the hash in the checksum file's text.
pub fn verify_zip(zip: &[u8], sha_text: &str) -> Result<(), String> {
    let expected = parse_sha_file(sha_text).ok_or("The checksum file is empty or malformed.")?;
    let actual = sha256_hex(zip);
    if actual == expected {
        Ok(())
    } else {
        Err(format!("Checksum mismatch (expected {expected}, got {actual}). The download was discarded."))
    }
}

/// Downloads, verifies and unpacks a release; nothing installed is touched.
pub fn download_and_verify(r: &Release) -> Result<Ready, String> {
    let safe_tag: String = r.tag.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')).collect();
    let dir = std::env::temp_dir().join(format!("claude-usage-update-{safe_tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;

    let result = (|| {
        let zip = dir.join("update.zip");
        let sha = dir.join("update.zip.sha256");
        run(curl().arg("-o").arg(&zip).arg(&r.zip_url))?;
        run(curl().arg("-o").arg(&sha).arg(&r.sha_url))?;
        let bytes = std::fs::read(&zip).map_err(|e| e.to_string())?;
        verify_zip(&bytes, &std::fs::read_to_string(&sha).map_err(|e| e.to_string())?)?;

        // Absolute path so a GNU tar earlier on PATH can't shadow the Windows one.
        let root = PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()));
        run(Command::new(root.join("System32").join("tar.exe")).arg("-xf").arg(&zip).arg("-C").arg(&dir))?;
        let new_exe = dir
            .join(format!("claude-usage-widget-{}-{}", r.tag, arch()))
            .join("claude-usage-widget.exe");
        if new_exe.is_file() {
            Ok(new_exe)
        } else {
            Err("The downloaded package does not contain claude-usage-widget.exe.".to_string())
        }
    })();

    match result {
        Ok(new_exe) => Ok(Ready { dir, new_exe }),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dir);
            Err(e)
        }
    }
}

fn helper_script() -> &'static str {
    r#"param([int]$ProcId, [string]$Target, [string]$NewExe, [string]$TempDir)
$old = "${Target}.old"
Wait-Process -Id $ProcId -Timeout 30 -ErrorAction SilentlyContinue
try {
    $ErrorActionPreference = 'Stop'
    Remove-Item -LiteralPath $old -Force -ErrorAction SilentlyContinue
    Rename-Item -LiteralPath $Target -NewName (Split-Path $old -Leaf)
    Copy-Item -LiteralPath $NewExe -Destination $Target
    Start-Process -FilePath $Target
    Remove-Item -LiteralPath $old -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $TempDir -Recurse -Force -ErrorAction SilentlyContinue
} catch {
    if (Test-Path -LiteralPath $old) {
        Remove-Item -LiteralPath $Target -Force -ErrorAction SilentlyContinue
        Rename-Item -LiteralPath $old -NewName (Split-Path $Target -Leaf)
    }
    Start-Process -FilePath $Target
}
"#
}

/// Starts the hidden helper that waits for this process to exit, swaps the exe and relaunches.
/// The caller must exit right after this returns Ok.
pub fn start_replace_helper(ready: &Ready) -> Result<(), String> {
    let target = std::env::current_exe().map_err(|e| e.to_string())?;
    let ps1 = ready.dir.join("update.ps1");
    std::fs::write(&ps1, helper_script()).map_err(|e| format!("Could not write the update helper: {e}"))?;
    Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File"])
        .arg(&ps1)
        .arg("-ProcId")
        .arg(std::process::id().to_string())
        .arg("-Target")
        .arg(&target)
        .arg("-NewExe")
        .arg(&ready.new_exe)
        .arg("-TempDir")
        .arg(&ready.dir)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not start the update helper: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parsing() {
        assert_eq!(parse_version("v0.4.0"), Some((0, 4, 0)));
        assert_eq!(parse_version("0.4"), Some((0, 4, 0)));
        assert_eq!(parse_version("nightly"), None);
        assert_eq!(parse_version("1.2.3-beta"), None);
    }

    #[test]
    fn newer_compare() {
        assert!(is_newer("v0.4.0", "0.3.0"));
        assert!(!is_newer("0.3.0", "0.3.0"));
        assert!(!is_newer("0.2.9", "0.3.0"));
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(!is_newer("nightly", "0.3.0"));
    }

    #[test]
    fn sha256_vectors() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(
            sha256_hex(&vec![b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn sha_file_parsing() {
        let h = "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD";
        let text = format!("{h}  claude-usage-widget-v0.4.0-win-x64.zip\r\n");
        assert_eq!(parse_sha_file(&text), Some(h.to_lowercase()));
        assert_eq!(parse_sha_file(""), None);
        assert_eq!(parse_sha_file("zzz  file"), None);
    }

    fn release_json(archs: &[&str]) -> String {
        let mut assets = Vec::new();
        for a in archs {
            for ext in ["zip", "zip.sha256"] {
                let name = format!("claude-usage-widget-v0.4.0-{a}.{ext}");
                assets.push(format!(
                    r#"{{"name":"{name}","browser_download_url":"https://example.test/{name}"}}"#
                ));
            }
        }
        format!(r#"{{"tag_name":"v0.4.0","assets":[{}]}}"#, assets.join(","))
    }

    #[test]
    fn release_parsing() {
        let r = parse_release(&release_json(&["win-x64", "win-arm64"]), "win-arm64").unwrap();
        assert_eq!(r.tag, "v0.4.0");
        assert_eq!(r.zip_url, "https://example.test/claude-usage-widget-v0.4.0-win-arm64.zip");
        assert_eq!(r.sha_url, "https://example.test/claude-usage-widget-v0.4.0-win-arm64.zip.sha256");

        let err = parse_release(&release_json(&["win-x64"]), "win-arm64").unwrap_err();
        assert!(err.contains("win-arm64"));

        assert!(parse_release(r#"{"message":"rate limit"}"#, "win-x64").is_err());
    }
}

#[cfg(test)]
mod install_tests {
    use super::*;

    #[test]
    fn helper_script_contains_rollback() {
        let s = helper_script();
        for needle in ["param(", "$ProcId", "$Target", "$NewExe", "$TempDir", "Wait-Process", "Rename-Item", ".old", "Start-Process", "catch"] {
            assert!(s.contains(needle), "missing {needle}");
        }
        // The catch block must put the old exe back.
        let catch = &s[s.find("catch").unwrap()..];
        assert!(catch.contains("Rename-Item") && catch.contains("Start-Process"));
    }

    #[test]
    fn bad_checksum_is_rejected() {
        let zip = b"abc";
        let good = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  x.zip";
        assert!(verify_zip(zip, good).is_ok());
        assert!(verify_zip(zip, &good.to_uppercase()).is_ok());
        assert!(verify_zip(b"abd", good).is_err());
        assert!(verify_zip(zip, "").is_err());
    }
}
