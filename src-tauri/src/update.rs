//! Keeping things current: Setup Hub itself (GitHub Releases of its own repo) and the catalog apps
//! it installed (winget's upgrade list, or the GitHub release for apps that come from there).

use crate::catalog::{self, Item, Source};
use crate::{detect, download, installer, sig, util};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// Where Setup Hub's own releases are published (README: "Download the latest release").
pub const REPO: &str = "reyjendiz/setup-hub";

/// "1.10.0" > "1.9.2"; a "v" prefix and "-beta" style suffixes are ignored, missing parts count as 0.
pub fn is_newer(latest: &str, current: &str) -> bool {
    let parts = |s: &str| -> Vec<u64> {
        let core = s.trim().trim_start_matches(['v', 'V']).split(['-', '+', ' ']).next().unwrap_or("");
        core.split('.').map(|x| x.parse().unwrap_or(0)).collect()
    };
    let (a, b) = (parts(latest), parts(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

// ---------- Setup Hub itself ----------

#[derive(Debug, Clone, Serialize)]
pub struct SelfUpdate {
    pub version: String,
    pub url: String,
    pub sha256: Option<String>,
    pub notes_url: String,
}

/// The portable exe in a release: `setup-hub.exe` or `setup-hub-1.2.0.exe` (not the NSIS `…-setup.exe`).
pub fn pick_exe(assets: &[catalog::GhAsset]) -> Option<&catalog::GhAsset> {
    let re = regex::Regex::new(r"(?i)^setup-hub(-v?[0-9][0-9.]*)?\.exe$").unwrap();
    assets.iter().find(|a| re.is_match(&a.name))
}

pub async fn check_self(token: Option<&str>) -> Result<Option<SelfUpdate>> {
    let rel = match catalog::github_latest(REPO, token).await {
        Ok(r) => r,
        Err(e) if e.is::<catalog::NoRelease>() => return Ok(None),
        Err(e) => return Err(e),
    };
    let version = rel.tag_name.trim_start_matches(['v', 'V']).to_string();
    if !is_newer(&version, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }
    let a = pick_exe(&rel.assets).with_context(|| format!("release {} has no setup-hub.exe", rel.tag_name))?;
    Ok(Some(SelfUpdate {
        version,
        url: a.browser_download_url.clone(),
        sha256: a.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")).map(str::to_string),
        notes_url: format!("https://github.com/{REPO}/releases/tag/{}", rel.tag_name),
    }))
}

fn old_exe(exe: &Path) -> PathBuf {
    exe.with_file_name(format!("{}.old.exe", exe.file_stem().unwrap_or_default().to_string_lossy()))
}

/// Downloads the new exe (hash or signature verified), swaps it in place of the running one —
/// Windows lets a running exe be renamed, not overwritten — and returns the path to restart.
pub async fn apply_self(u: &SelfUpdate, cancel: &AtomicBool, progress: &(dyn Fn(Option<f64>) + Send + Sync)) -> Result<PathBuf> {
    let hosts = vec!["github.com".to_string(), "githubusercontent.com".to_string()];
    let dir = util::cache_dir().join("self-update");
    let new = download::fetch(&util::http(hosts), &u.url, &dir, cancel, &|done, total| {
        progress(total.filter(|t| *t > 0).map(|t| done as f64 / t as f64 * 100.0));
    })
    .await?;
    let v = sig::verify(&new, u.sha256.as_deref(), None).await?;
    tracing::info!("self-update {}: {v}", u.version);
    let exe = std::env::current_exe()?;
    let old = old_exe(&exe);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).with_context(|| format!("can't replace {} (is the folder writable?)", exe.display()))?;
    if let Err(e) = std::fs::copy(&new, &exe) {
        let _ = std::fs::rename(&old, &exe);
        bail!("could not write the new version: {e}");
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(exe)
}

/// Starts `exe` once this process has exited (the single-instance guard would otherwise hand the
/// new window straight back to us). The child inherits our elevation, so there's no second UAC prompt.
pub fn restart_into(exe: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("cmd.exe")
        .creation_flags(util::CREATE_NO_WINDOW)
        .raw_arg(format!("/c ping -n 3 127.0.0.1 >nul & start \"\" \"{}\"", exe.display()))
        .spawn()?;
    Ok(())
}

/// Removes the previous exe left behind by an update.
pub fn cleanup_old() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(old_exe(&exe));
    }
}

// ---------- installed apps ----------

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AppUpdate {
    pub id: String,
    pub installed: String,
    pub available: String,
}

/// Finds catalog winget ids in `winget upgrade` output and reads the two version columns after them.
/// Matching the id token (not column offsets) keeps it working whatever the table's language or width.
pub fn parse_winget_upgrades(out: &str, ids: &[&str]) -> HashMap<String, (String, String)> {
    let mut found = HashMap::new();
    for line in out.split(['\r', '\n']) {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let Some(pos) = tokens.iter().position(|t| ids.iter().any(|id| id.eq_ignore_ascii_case(t))) else { continue };
        // "< 3.2" / "> 1.0" in the version column split into two tokens.
        let vers: Vec<&str> = tokens[pos + 1..].iter().copied().filter(|t| *t != "<" && *t != ">").take(2).collect();
        if let [cur, avail] = vers[..] {
            if avail.starts_with(|c: char| c.is_ascii_digit()) {
                let id = ids.iter().find(|id| id.eq_ignore_ascii_case(tokens[pos])).unwrap();
                found.insert(id.to_string(), (cur.to_string(), avail.to_string()));
            }
        }
    }
    found
}

/// Newer versions of installed catalog apps: winget's upgrade list for winget apps, the latest GitHub
/// release for apps installed from there. Apps whose source has no version (direct links) are skipped.
pub async fn check_apps(items: &[Item], token: Option<&str>) -> Vec<AppUpdate> {
    let its = items.to_vec();
    let installed = tokio::task::spawn_blocking(move || detect::detect_all(&its)).await.unwrap_or_default();
    let mut out = Vec::new();

    let winget: Vec<&Item> = items.iter().filter(|i| i.winget_id.is_some() && installed.contains_key(&i.id)).collect();
    let mut via_winget = false;
    if !winget.is_empty() && installer::winget_available().await {
        via_winget = true;
        let args = ["upgrade", "--include-unknown", "--source", "winget", "--accept-source-agreements", "--disable-interactivity"];
        match util::run("winget.exe", &args).await {
            Ok((_, text)) => {
                let ids: Vec<&str> = winget.iter().filter_map(|i| i.winget_id.as_deref()).collect();
                let found = parse_winget_upgrades(&text, &ids);
                for it in &winget {
                    if let Some((cur, avail)) = found.get(it.winget_id.as_deref().unwrap()) {
                        out.push(AppUpdate { id: it.id.clone(), installed: cur.clone(), available: avail.clone() });
                    }
                }
            }
            Err(e) => tracing::warn!("winget upgrade failed: {e:#}"),
        }
    }

    for it in items.iter().filter(|i| matches!(i.source, Source::GithubRelease { .. }) && !(via_winget && i.winget_id.is_some())) {
        let Some(cur) = installed.get(&it.id).filter(|v| !v.is_empty()) else { continue };
        match catalog::resolve(it, token).await {
            Ok(r) => {
                if let Some(latest) = r.version.filter(|v| is_newer(v, cur)) {
                    out.push(AppUpdate { id: it.id.clone(), installed: cur.clone(), available: latest });
                }
            }
            Err(e) => tracing::warn!("{}: update check failed: {e:#}", it.id),
        }
    }
    tracing::info!("update check: {} app(s) can be updated", out.len());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(is_newer("1.2.0", "1.1.0"));
        assert!(is_newer("v1.10.0", "1.9.9"));
        assert!(is_newer("0.5.0", "0.4.0.0"));
        assert!(!is_newer("0.5.0", "0.5.0.0"));
        assert!(!is_newer("1.1.0", "1.1.0"));
        assert!(!is_newer("1.2.0-beta.1", "1.2.0"));
        assert!(!is_newer("2.54.0", "2.55.0.5"));
    }

    #[test]
    fn release_exe_pick() {
        let a = |n: &str| catalog::GhAsset { name: n.into(), browser_download_url: format!("https://x/{n}"), digest: None, size: 1 };
        let assets = vec![a("Setup Hub_1.2.0_x64-setup.exe"), a("setup-hub.exe"), a("SHA256SUMS.txt")];
        assert_eq!(pick_exe(&assets).unwrap().name, "setup-hub.exe");
        assert_eq!(pick_exe(&[a("setup-hub-1.2.0.exe")]).unwrap().name, "setup-hub-1.2.0.exe");
        assert!(pick_exe(&[a("Setup Hub_1.2.0_x64-setup.exe")]).is_none());
        assert_eq!(old_exe(Path::new(r"C:\Tools\setup-hub.exe")), Path::new(r"C:\Tools\setup-hub.old.exe"));
    }

    #[test]
    fn winget_upgrade_table() {
        let out = "\r   - \r   \\ \r\nName                       Id                    Version      Available    Source\r\n\
-----------------------------------------------------------------------------------\r\n\
Google Chrome              Google.Chrome         141.0.7390   155.0.8059.40 winget\r\n\
Microsoft Visual C++ 2015… Microsoft.VCRedist.2… 14.38.33135  14.44.35211  winget\r\n\
Discord                    Discord.Discord       < 1.0.9261   1.0.9300     winget\r\n\
Node.js (LTS)              OpenJS.NodeJS.LTS     24.19.0      24.20.0      winget\r\n\
3 upgrades available.\r\n";
        let f = parse_winget_upgrades(out, &["Google.Chrome", "Discord.Discord", "openjs.nodejs.lts", "Valve.Steam"]);
        assert_eq!(f.len(), 3);
        assert_eq!(f["Google.Chrome"], ("141.0.7390".to_string(), "155.0.8059.40".to_string()));
        assert_eq!(f["Discord.Discord"], ("1.0.9261".to_string(), "1.0.9300".to_string()));
        assert_eq!(f["openjs.nodejs.lts"].1, "24.20.0");
        // Russian headers, same rows.
        let ru = "Имя      ИД             Версия   Доступно Источник\r\n---\r\nSteam    Valve.Steam    2.10.91  2.11.0   winget\r\n";
        assert_eq!(parse_winget_upgrades(ru, &["Valve.Steam"])["Valve.Steam"].1, "2.11.0");
        assert!(parse_winget_upgrades("No installed package found matching input criteria.", &["Valve.Steam"]).is_empty());
    }
}
