# Check for updates (tray menu) - design

## Intent
Let the user update the widget from the tray without visiting GitHub: check the latest release, download it, verify it, replace the running exe and relaunch.

## Goals
- New tray menu item "Check for updates" (in `show_menu`, `src/main.rs`).
- Compare the latest GitHub release tag with the compiled-in version (`CARGO_PKG_VERSION`).
- Up to date: message box says so. Newer: dialog "Update to vX.Y.Z?"; on Yes download, verify, swap, relaunch.
- Pick the asset for the running architecture (`win-x64` / `win-arm64`), matching the release workflow names `claude-usage-widget-<tag>-<arch>.zip` + `.zip.sha256`.

## Non-goals
- No automatic/background update checks.
- No release-notes page, no downgrade/channel selection.
- No new Rust dependencies.

## Anti-goals
- A failed or tampered download must never touch the installed exe.
- Never leave the user with no running app after a failed swap.

## Constraints
- A running exe cannot be overwritten on Windows; it can be renamed.
- Repo: `dhhieu113pro/claude-usage`. Assumes releases are publicly downloadable without auth (verify when building).
- App currently has no HTTP client.

## Approaches
- **A. Windows built-ins (chosen):** Rust drives everything up to "ready to install": `curl.exe` for the GitHub API and downloads, `tar.exe` to unzip, and a small in-Rust SHA-256 to verify. Only the final replace + relaunch is a hidden PowerShell helper. No dependencies, small binary. Relies on curl/tar/PowerShell being present (true on Win 10/11).
- B. `ureq` + `zip` crates: in-process download, but adds dependencies/size and still needs a helper for the swap.
- C. WinHTTP via `windows` crate: no new crates but the most unsafe code and hand-rolled parsing.

## Decided flow
1. Menu click -> run `curl.exe -s` against `/repos/<owner>/<repo>/releases/latest`, parse `tag_name` with `serde_json`. Do it off the UI thread so the tray stays responsive.
2. Compare versions (numeric semver, strip leading `v`). Not newer -> "You're up to date (vX)".
3. Newer -> Yes/No dialog. On Yes, download the zip and its `.sha256` (via `curl.exe`) to a fresh temp dir.
4. Compute SHA256 of the zip in Rust (small hand-written implementation, unit-tested against known vectors) and compare with the `.sha256` contents (case-insensitive). Mismatch -> error dialog, delete temp files, stop.
5. Extract the zip in Rust code via `tar.exe -xf` and locate the new exe. The update is now "ready"; nothing installed has changed yet.
6. Only now start the hidden PowerShell helper, then exit the app. The helper only does the replace: wait for the app process to exit, rename the current exe to `*.old`, copy the new exe into place, start it, delete `*.old` and the temp dir. If the copy fails, rename `*.old` back and relaunch the old exe.
7. Any network/parse/IO error -> error dialog; installed exe untouched.

## Left to the builder
- Exact message/dialog wording, temp directory location, how the exe's own path/PID is passed to the script, and the version-compare helper. Keep naming and comment density consistent with `main.rs`.
- README: add a short "Updating" note.
