//! Ven: the small app (../ven) that keeps Vencord installed and current and starts Discord at sign-in.
//! Setup Hub embeds it, installs it per user, registers it in HKCU\…\Run and runs it once as the
//! signed-in user. It replaces the old AD launcher, which is cleaned up on install.

use crate::{installer, util};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use winreg::enums::*;
use winreg::RegKey;

pub const EXE: &[u8] = include_bytes!(env!("VEN_EXE"));
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

fn install_dir() -> PathBuf {
    PathBuf::from(util::expand_env(r"%LOCALAPPDATA%\Programs\Ven"))
}

fn exe_path() -> PathBuf {
    install_dir().join("Ven.exe")
}

/// Ven's own data: log, status of the last run, its copy of VencordInstallerCli.exe.
fn data_dir() -> PathBuf {
    PathBuf::from(util::expand_env(r"%LOCALAPPDATA%\Ven"))
}

fn discord_dir() -> PathBuf {
    PathBuf::from(util::expand_env(r"%LOCALAPPDATA%\Discord"))
}

pub fn run_command() -> String {
    format!("\"{}\" --startup", exe_path().display())
}

/// "app-1.0.9261" → [1, 0, 9261]
fn app_version(name: &str) -> Option<Vec<u64>> {
    name.strip_prefix("app-")?.split('.').map(|p| p.parse().ok()).collect()
}

/// The newest `app-*` folder (the one Discord runs and Vencord patches).
pub fn newest_app<'a>(names: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    names.into_iter().filter_map(|n| app_version(n).map(|v| (v, n))).max().map(|(_, n)| n)
}

/// Vencord's installer renames the original app.asar to _app.asar when it patches.
fn vencord_patched() -> bool {
    let dir = discord_dir();
    let names: Vec<String> = std::fs::read_dir(&dir).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    newest_app(names.iter().map(String::as_str)).is_some_and(|app| dir.join(app).join(r"resources\_app.asar").is_file())
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LastRun {
    pub time: u64,
    pub vencord_ok: bool,
    pub discord_started: bool,
    pub message: String,
}

#[derive(Serialize)]
pub struct VenState {
    pub installed: bool,
    /// HKCU Run points at this Ven.exe.
    pub autostart: bool,
    pub discord: bool,
    /// The Discord version that runs has Vencord in it.
    pub vencord: bool,
    pub last_run: Option<LastRun>,
    pub path: String,
}

fn run_value() -> Option<String> {
    RegKey::predef(HKEY_CURRENT_USER).open_subkey(RUN_KEY).and_then(|k| k.get_value::<String, _>("Ven")).ok()
}

pub fn state() -> VenState {
    VenState {
        installed: exe_path().is_file(),
        autostart: run_value().is_some_and(|v| v.eq_ignore_ascii_case(&run_command())),
        discord: discord_dir().join("Update.exe").is_file(),
        vencord: vencord_patched(),
        last_run: std::fs::read_to_string(data_dir().join("status.json")).ok().and_then(|s| serde_json::from_str(&s).ok()),
        path: exe_path().display().to_string(),
    }
}

/// The old AD launcher (MyDiscordLauncher, .NET): its files, shortcuts and autostart entry.
fn remove_old_ad() {
    if let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_WRITE) {
        if k.delete_value("MyDiscordLauncher").is_ok() {
            tracing::info!("ven: removed AD's autostart entry");
        }
    }
    for p in [
        util::desktop_dir().join("AD.lnk"),
        util::start_menu_dir().join("AD.lnk"),
    ] {
        let _ = std::fs::remove_file(p);
    }
    for d in [r"%LOCALAPPDATA%\Programs\AD", r"%LOCALAPPDATA%\MyDiscordLauncher"] {
        let d = PathBuf::from(util::expand_env(d));
        if d.exists() && std::fs::remove_dir_all(&d).is_ok() {
            tracing::info!("ven: removed old AD folder {}", d.display());
        }
    }
}

/// Writes Ven.exe, registers it to run at sign-in, adds a Start Menu shortcut and removes AD.
pub async fn install() -> Result<()> {
    remove_old_ad();
    std::fs::create_dir_all(install_dir())?;
    let exe = exe_path();
    std::fs::write(&exe, EXE).with_context(|| format!("cannot write {} (is Ven running? try again in a minute)", exe.display()))?;
    RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY)?.0.set_value("Ven", &run_command())?;
    // A Run entry someone disabled in Task Manager stays disabled; Ven's must be on.
    if let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(APPROVED_KEY, KEY_WRITE) {
        let _ = k.delete_value("Ven");
    }
    util::create_shortcut(&util::start_menu_dir().join("Ven.lnk"), &exe, "").await?;
    tracing::info!("ven: installed to {}", exe.display());
    Ok(())
}

/// Runs Ven as the signed-in user (it starts Discord, which must not inherit our elevation) and
/// returns what it reported.
pub async fn run_now(cancel: &AtomicBool) -> Result<LastRun> {
    let before = std::fs::metadata(data_dir().join("status.json")).and_then(|m| m.modified()).ok();
    let code = installer::run_unelevated(&format!("\"{}\"", exe_path().display()), cancel).await?;
    let status = data_dir().join("status.json");
    let fresh = std::fs::metadata(&status).and_then(|m| m.modified()).ok().is_some_and(|m| Some(m) != before);
    let last: LastRun = fresh
        .then(|| std::fs::read_to_string(&status).ok().and_then(|s| serde_json::from_str(&s).ok()))
        .flatten()
        .with_context(|| format!("Ven exited with code {code} without reporting — see {}", data_dir().join("ven.log").display()))?;
    tracing::info!("ven: run finished ({code}): {}", last.message);
    Ok(last)
}

/// Removes Ven and gives Discord its own autostart back. Vencord stays in Discord.
pub fn remove() -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(k) = hkcu.open_subkey_with_flags(RUN_KEY, KEY_WRITE) {
        let _ = k.delete_value("Ven");
    }
    if let Ok(k) = hkcu.open_subkey_with_flags(APPROVED_KEY, KEY_WRITE) {
        let _ = k.delete_value("Discord");
    }
    let _ = std::fs::remove_file(util::start_menu_dir().join("Ven.lnk"));
    let _ = std::fs::remove_dir_all(install_dir());
    let _ = std::fs::remove_dir_all(data_dir());
    tracing::info!("ven: removed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_app_folder() {
        let dirs = ["app-1.0.9261", "packages", "app-1.0.10012", "app-1.0.999", "Update.exe", "app-x"];
        assert_eq!(newest_app(dirs), Some("app-1.0.10012"));
        assert_eq!(newest_app(["packages"]), None);
    }

    #[test]
    fn embedded_ven_is_a_windows_exe() {
        assert!(EXE.len() > 100_000 && EXE.starts_with(b"MZ"));
    }
}
