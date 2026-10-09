//! AD = the user's MyDiscordLauncher (F:\AD\VencordLauncher): a .NET 8 WinForms app that downloads
//! VencordInstallerCli, re-patches Discord after updates and then starts it. Framework-dependent,
//! so it needs the .NET 8 Desktop Runtime.

use crate::{catalog, download, sig, util};
use anyhow::{Context, Result};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

pub const EXE: &[u8] = include_bytes!("../resources/AD/VencordLauncher/out/MyDiscordLauncher.exe");
const EXE_NAME: &str = "MyDiscordLauncher.exe";

fn install_dir() -> PathBuf {
    PathBuf::from(util::expand_env(r"%LOCALAPPDATA%\Programs\AD"))
}

/// Same paths Program.cs uses, so AD's own UI shows these steps as done.
fn data_dir() -> PathBuf {
    PathBuf::from(util::expand_env(r"%LOCALAPPDATA%\MyDiscordLauncher"))
}

pub fn discord_update_exe() -> PathBuf {
    PathBuf::from(util::expand_env(r"%LOCALAPPDATA%\Discord\Update.exe"))
}

pub fn dotnet8_desktop_present() -> bool {
    let base = PathBuf::from(util::expand_env(r"%ProgramFiles%\dotnet\shared\Microsoft.WindowsDesktop.App"));
    std::fs::read_dir(base)
        .map(|d| d.flatten().any(|e| e.file_name().to_string_lossy().starts_with("8.")))
        .unwrap_or(false)
}

#[derive(Serialize)]
pub struct AdState {
    pub installed: bool,
    pub discord: bool,
    pub dotnet: bool,
    pub vencord_cli: bool,
    pub path: String,
}

pub fn state() -> AdState {
    let exe = install_dir().join(EXE_NAME);
    AdState {
        installed: exe.exists(),
        discord: discord_update_exe().exists(),
        dotnet: dotnet8_desktop_present(),
        vencord_cli: data_dir().join("VencordInstallerCli.exe").exists(),
        path: exe.display().to_string(),
    }
}

/// Fetches VencordInstallerCli.exe from the latest GitHub release, verified against GitHub's SHA-256 digest.
pub async fn fetch_vencord_cli(cancel: &AtomicBool) -> Result<()> {
    let item = catalog::Item {
        id: "vencord-cli".into(),
        name: "Vencord CLI".into(),
        category: "".into(),
        description: Default::default(),
        winget_id: None,
        source: catalog::Source::GithubRelease { repo: "Vencord/Installer".into(), asset_regex: r"^VencordInstallerCli\.exe$".into() },
        silent_args: vec![],
        detect: Default::default(),
        allowed_hosts: vec!["github.com".into(), "githubusercontent.com".into()],
        publisher: None,
        zip: None,
        needs_reboot: false,
        interactive: false,
        unelevated: false,
        note: None,
        mode: Default::default(),
        winget_source: None,
        custom: false,
        reviewed: false,
        allow_unsigned: false,
        allow_http: false,
    };
    let r = catalog::resolve(&item, crate::settings::get_secret("github_token").as_deref()).await?;
    let dir = util::cache_dir().join("vencord-cli");
    let f = download::fetch(&util::http(item.allowed_hosts.clone()), &r.url, &dir, cancel, &|_, _| {}).await?;
    sig::verify(&f, r.sha256.as_deref(), None).await?;
    std::fs::create_dir_all(data_dir())?;
    std::fs::copy(&f, data_dir().join("VencordInstallerCli.exe"))?;
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

/// Copies AD out of this binary, pre-seeds its Discord path and Vencord CLI, adds shortcuts.
pub async fn install(cancel: &AtomicBool) -> Result<String> {
    let dir = install_dir();
    std::fs::create_dir_all(&dir)?;
    let exe = dir.join(EXE_NAME);
    std::fs::write(&exe, EXE).with_context(|| format!("cannot write {}", exe.display()))?;
    std::fs::create_dir_all(data_dir())?;
    let upd = discord_update_exe();
    if upd.exists() && !data_dir().join("discord_path.txt").exists() {
        std::fs::write(data_dir().join("discord_path.txt"), upd.display().to_string())?;
    }
    if let Err(e) = fetch_vencord_cli(cancel).await {
        tracing::warn!("AD: Vencord CLI prefetch failed, AD can download it itself: {e:#}");
    }
    util::create_shortcut(&util::desktop_dir().join("AD.lnk"), &exe, "").await?;
    util::create_shortcut(&util::start_menu_dir().join("AD.lnk"), &exe, "").await?;
    tracing::info!("AD installed to {}", exe.display());
    Ok(exe.display().to_string())
}
