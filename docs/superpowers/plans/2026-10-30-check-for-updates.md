# Check for Updates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a tray menu item "Check for updates" that finds the latest GitHub release, downloads and SHA-256-verifies it in Rust, then replaces the running exe and relaunches.

**Architecture:** New `src/update.rs` holds pure, unit-tested logic (version compare, SHA-256, release JSON parsing) plus thin IO wrappers around `curl.exe` and `tar.exe`. `main.rs` adds the menu item and runs the flow on a background thread with `MessageBoxW` dialogs. Only the final replace + relaunch is a hidden PowerShell helper script, started after the update is verified and extracted.

**Tech Stack:** Rust 2021, `windows` crate (already a dependency), `serde_json` (already), Windows built-ins `curl.exe`, `tar.exe`, `powershell.exe`. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-10-30-check-for-updates-design.md`

## Global Constraints

- No new Rust dependencies.
- Repo for releases: `dhhieu113pro/claude-usage`; API `https://api.github.com/repos/dhhieu113pro/claude-usage/releases/latest`.
- Asset names: `claude-usage-widget-<tag>-<arch>.zip` and `claude-usage-widget-<tag>-<arch>.zip.sha256`, `<arch>` is `win-x64` or `win-arm64` (matches `.github/workflows/release.yml`). The zip contains a folder `claude-usage-widget-<tag>-<arch>/` holding `claude-usage-widget.exe`.
- The `.sha256` file format is `<hex>  <zipname>` (written by the workflow).
- Installed exe is untouched until the update is downloaded, verified and extracted.
- Replace via rename-to-`*.old` then copy; on copy failure rename back and relaunch the old exe.
- Match `main.rs` style: unsafe Win32 inline, sparse comments, `let _ =` for ignored results.

## Review Focus

- Tag `v0.4.0` vs `0.4.0`, and short or non-numeric tags (`0.4`, `nightly`): compare numerically after stripping `v`; unparseable tag is treated as "not newer", never a crash (Task 1).
- Release lacks the asset for this arch (e.g. no arm64 zip): clear error dialog, nothing installed (Task 1).
- `.sha256` file with uppercase hash, trailing newline, CRLF, or empty content: accepted when hash matches ignoring case, rejected when empty (Task 1).
- Offline / curl missing / GitHub rate-limited (API returns JSON without `tag_name`): error dialog, app keeps running (Task 2).
- Exe path containing spaces or apostrophes, and double-clicking the menu item while a check is running: paths go to the helper as separate arguments (never string-interpolated into the script), and an `AtomicBool` guard ignores a second concurrent run (Tasks 2, 3).

---

### Task 1: Pure update logic (`src/update.rs`)

**Files:**
- Create: `src/update.rs`
- Modify: `src/main.rs:3-6` (add `mod update;`)

**Interfaces:**
- Produces:
  - `pub const REPO: &str = "dhhieu113pro/claude-usage";`
  - `pub fn parse_version(tag: &str) -> Option<(u32, u32, u32)>` (strips leading `v`/`V`; missing minor/patch parts default to 0; non-numeric -> `None`)
  - `pub fn is_newer(latest_tag: &str, current: &str) -> bool` (`false` if either is unparseable)
  - `pub fn sha256_hex(data: &[u8]) -> String` (lowercase hex)
  - `pub fn parse_sha_file(text: &str) -> Option<String>` (first whitespace-separated token, lowercased; `None` if empty or not 64 hex chars)
  - `pub struct Release { pub tag: String, pub zip_url: String, pub sha_url: String }`
  - `pub fn parse_release(json: &str, arch: &str) -> Result<Release, String>` (reads `tag_name` and `assets[].name`/`browser_download_url`; `Err` with a user-readable message when `tag_name` is missing or the arch's zip or `.sha256` asset is absent)
  - `pub fn arch() -> &'static str` (`"win-arm64"` if `cfg!(target_arch = "aarch64")`, else `"win-x64"`)

- [ ] **Step 1: Write failing tests** in `#[cfg(test)] mod tests` at the bottom of `src/update.rs`:
  - `version_parsing`: `parse_version("v0.4.0") == Some((0,4,0))`, `"0.4" -> Some((0,4,0))`, `"nightly" -> None`, `"1.2.3-beta" -> None`.
  - `newer_compare`: `is_newer("v0.4.0","0.3.0")`, `!is_newer("0.3.0","0.3.0")`, `!is_newer("0.2.9","0.3.0")`, `is_newer("0.10.0","0.9.0")`, `!is_newer("nightly","0.3.0")`.
  - `sha256_vectors`: `sha256_hex(b"")` == `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`; `sha256_hex(b"abc")` == `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad`; a 1,000,000-byte `b'a'` input == `cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0` (covers multi-block).
  - `sha_file_parsing`: `"ABC...64hex  claude-usage-widget-v0.4.0-win-x64.zip\r\n"` returns the lowercased hash; `""` and `"zzz  file"` return `None`.
  - `release_parsing`: JSON with `tag_name: "v0.4.0"` and four assets (x64/arm64 zip + sha) -> `parse_release(json,"win-arm64")` returns the arm64 URLs; JSON with only x64 assets and arch `win-arm64` -> `Err` mentioning `win-arm64`; JSON `{"message":"rate limit"}` -> `Err`.

- [ ] **Step 2: Run to verify failure:** `cargo test update:: ` -> compile error / FAIL (functions undefined).

- [ ] **Step 3: Implement the functions above.** Hand-written SHA-256 (standard FIPS 180-4: 64 round constants, padding, big-endian length), no crate. Use `serde_json::Value` for release parsing. Add `mod update;` to `main.rs`; add `#![allow(dead_code)]`-free code (functions get used in Task 2/3, a temporary `#[allow(dead_code)]` on the module declaration is acceptable until Task 3).

- [ ] **Step 4: Run to verify pass:** `cargo test update::` -> all pass.

- [ ] **Step 5: Commit**

```bash
git add src/update.rs src/main.rs docs/superpowers
git commit -m "Add update logic: version compare, SHA-256, release parsing"
```

---

### Task 2: Download, verify, extract, helper (`src/update.rs`)

**Files:**
- Modify: `src/update.rs`

**Interfaces:**
- Consumes: Task 1 (`parse_release`, `arch`, `sha256_hex`, `parse_sha_file`, `Release`, `REPO`).
- Produces:
  - `pub fn fetch_latest() -> Result<Release, String>` (runs `curl.exe -sSL --fail -H "User-Agent: claude-usage-widget" <api url>`, then `parse_release(&stdout, arch())`)
  - `pub struct Ready { pub dir: std::path::PathBuf, pub new_exe: std::path::PathBuf }`
  - `pub fn download_and_verify(r: &Release) -> Result<Ready, String>` (creates a fresh dir under `std::env::temp_dir()` named `claude-usage-update-<tag>`, removing any stale one first; downloads zip and `.sha256` with `curl.exe -sSL --fail -o`; compares `sha256_hex(zip bytes)` with `parse_sha_file`; mismatch or unparseable sha file -> delete dir and `Err("checksum mismatch ...")`; extracts with `tar.exe -xf <zip> -C <dir>`; `new_exe` is `<dir>/claude-usage-widget-<tag>-<arch>/claude-usage-widget.exe`, `Err` if it does not exist)
  - `pub fn start_replace_helper(ready: &Ready) -> Result<(), String>` (writes `update.ps1` into `ready.dir`; spawns `powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File <ps1> -ProcId <pid> -Target <current_exe> -NewExe <new_exe> -TempDir <dir>` with `CREATE_NO_WINDOW` (0x0800_0000, same as `usage.rs`); paths are separate `Command::args`, never interpolated into the script)
- All `Command`s use `CREATE_NO_WINDOW` via `std::os::windows::process::CommandExt::creation_flags`.

- [ ] **Step 1: Write failing tests** (pure parts only; network is not tested):
  - `helper_script_contains_rollback`: expose `fn helper_script() -> &'static str` and assert it contains `Wait-Process`, `Rename-Item`, `.old`, `Start-Process`, and a `catch` block that renames `.old` back. Assert it declares `param(` with `ProcId`, `Target`, `NewExe`, `TempDir`.
  - `bad_checksum_is_rejected`: split verification into `fn verify_zip(zip: &[u8], sha_text: &str) -> Result<(), String>`; assert OK for matching hash (incl. uppercase), `Err` for a different hash, `Err` for empty sha text.

- [ ] **Step 2: Run to verify failure:** `cargo test update::` -> FAIL.

- [ ] **Step 3: Implement** `helper_script`, `verify_zip`, and the three public functions. Helper script flow: `Wait-Process -Id $ProcId -ErrorAction SilentlyContinue` (up to ~30s), `Rename-Item $Target "$Target.old"`, `try { Copy-Item $NewExe $Target; Start-Process $Target; Remove-Item "$Target.old"; Remove-Item $TempDir -Recurse } catch { Rename-Item "$Target.old" $Target; Start-Process $Target }`. Use `-LiteralPath` everywhere.

- [ ] **Step 4: Run to verify pass:** `cargo test update::` -> pass.

- [ ] **Step 5: Commit**

```bash
git add src/update.rs
git commit -m "Add update download, checksum verify, extract and replace helper"
```

---

### Task 3: Menu wiring, dialogs, README

**Files:**
- Modify: `src/main.rs` (consts at ~`:34-40`, `show_menu` at `:477-520`)
- Modify: `README.md`

**Interfaces:**
- Consumes: Task 2 (`fetch_latest`, `download_and_verify`, `start_replace_helper`), Task 1 (`is_newer`).
- Produces: `const ID_UPDATE: usize = 7;`, `static UPDATING: AtomicBool`, `fn check_for_updates(hwnd: HWND)`.

- [ ] **Step 1: Add menu item and handler.** Insert `Check for updates` between "Settings" and "Quit" (`ID_UPDATE`, `w!("Check for updates")`). `ID_UPDATE => check_for_updates(hwnd)`.

- [ ] **Step 2: Implement `check_for_updates(hwnd: HWND)`:** if `UPDATING.swap(true)` was already true, return. Spawn a thread (pass `hwnd.0 as isize` like `spawn_worker`) that:
  1. `fetch_latest()`; on `Err` show error `MessageBoxW` (MB_OK | MB_ICONERROR).
  2. If `!is_newer(&r.tag, env!("CARGO_PKG_VERSION"))` show "You're up to date (v<current>)." (MB_ICONINFORMATION).
  3. Else `MessageBoxW` MB_YESNO "Update to <tag>? The app will restart."; on No, stop.
  4. `download_and_verify`; on `Err` show error dialog (message includes checksum failures).
  5. `start_replace_helper`; on `Err` show error dialog. On success `PostMessageW(Some(HWND(hwnd as _)), WM_CLOSE, ...)` so the app exits and the helper takes over.
  Always `UPDATING.store(false)` on every non-exit path. Dialog title: `Usage Rings for Claude Code`.

- [ ] **Step 3: Build:** `cargo build --release` -> succeeds, no warnings (remove any temporary `allow(dead_code)`).

- [ ] **Step 4: Manual verify** (document the result in the commit/PR):
  - Run the exe; menu shows "Check for updates".
  - With the current latest release equal to the running version -> "up to date" dialog.
  - Temporarily build with `Cargo.toml` version lowered (e.g. `0.0.1`) and click: Yes -> app closes, relaunches as the new version, no `.old` file or temp dir left; No -> app keeps running.
  - Disconnect network -> error dialog, app keeps running.
  - Corrupt-check: edit the downloaded sha via a local test run is not required; covered by `bad_checksum_is_rejected`.
  Revert the temporary version change.

- [ ] **Step 5: README:** add a short "Updating" section: tray menu -> Check for updates; requires Windows 10/11 built-in `curl`, `tar`, PowerShell; downloads are verified against the release's `.sha256`.

- [ ] **Step 6: Commit**

```bash
git add src/main.rs README.md
git commit -m "Add Check for updates tray menu item"
```
