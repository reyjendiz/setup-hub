use crate::util;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const EMBEDDED: &str = include_str!("../catalog.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub version: u32,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct I18n {
    pub en: String,
    #[serde(default)]
    pub ru: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Source {
    Winget,
    Direct { url: String },
    GithubRelease { repo: String, asset_regex: String },
    Scrape { page: String, regex: String },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Detect {
    pub display_name: Option<String>,
    pub path: Option<String>,
    pub appx: Option<String>,
    /// Exact uninstall key ("HKLM\SOFTWARE\…\Uninstall\{GUID}"), recorded by snapshotting the first install.
    #[serde(default)]
    pub uninstall_key: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Silent,
    DownloadOnly,
    Interactive,
    Extract,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ZipSpec {
    /// Regex of the installer inside the zip to run.
    pub run: Option<String>,
    /// Portable tool: extract here instead of running an installer.
    pub extract_to: Option<String>,
    pub shortcut: Option<String>,
    #[serde(default)]
    pub launch: bool,
}

/// What an app takes over from Windows once installed (see defaults.rs).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Defaults {
    /// Value name under RegisteredApplications; its Capabilities supply the ProgIds.
    pub app: Option<String>,
    /// File types to make default. Only the ones the app itself registered are used.
    #[serde(default)]
    pub types: Vec<String>,
    /// Hand the Print Screen key to the app instead of Snipping Tool.
    #[serde(default)]
    pub print_screen: bool,
    /// Exe in the install folder to start (unelevated) after install: a hotkey tool only works while running.
    pub start: Option<String>,
}

/// Never handed to another app by a (remote) catalog, even if the app registered them.
const PROTECTED_TYPES: [&str; 10] = ["exe", "com", "msi", "lnk", "scr", "pif", "cpl", "url", "html", "htm"];

fn validate_defaults(id: &str, d: &Defaults) -> Result<()> {
    if !d.types.is_empty() && d.app.is_none() {
        bail!("{id}: defaults.types needs defaults.app");
    }
    if let Some(app) = &d.app {
        if app.is_empty() || !app.chars().all(|c| c.is_ascii_alphanumeric() || " ._-".contains(c)) {
            bail!("{id}: bad defaults.app");
        }
    }
    let ext = regex::Regex::new(r"^\.[a-z0-9][a-z0-9-]{0,15}$").unwrap();
    for t in &d.types {
        let bare = t.trim_start_matches('.');
        if !ext.is_match(t) || PROTECTED_TYPES.contains(&bare) || crate::installer::SCRIPT_EXTS.contains(&bare) {
            bail!("{id}: defaults type {t} is not allowed");
        }
    }
    if let Some(s) = &d.start {
        if s.contains(['/', '\\', ':']) || !s.to_ascii_lowercase().ends_with(".exe") {
            bail!("{id}: defaults.start must be an .exe file name");
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: I18n,
    pub winget_id: Option<String>,
    pub source: Source,
    #[serde(default)]
    pub silent_args: Vec<String>,
    #[serde(default)]
    pub detect: Detect,
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    /// Expected substring of the Authenticode signer (case-insensitive).
    pub publisher: Option<String>,
    pub zip: Option<ZipSpec>,
    #[serde(default)]
    pub needs_reboot: bool,
    #[serde(default)]
    pub interactive: bool,
    #[serde(default)]
    pub unelevated: bool,
    pub note: Option<I18n>,
    #[serde(default)]
    pub mode: Mode,
    /// winget source: "winget" (default) or "msstore".
    #[serde(default)]
    pub winget_source: Option<String>,
    #[serde(default)]
    pub defaults: Option<Defaults>,
    // Runtime-only flags for My Apps entries. skip_deserializing: a (remote) catalog can never set them.
    #[serde(skip_deserializing, default)]
    pub custom: bool,
    #[serde(skip_deserializing, default)]
    pub reviewed: bool,
    #[serde(skip_deserializing, default)]
    pub allow_unsigned: bool,
    #[serde(skip_deserializing, default)]
    pub allow_http: bool,
}

pub fn parse(json: &str) -> Result<Catalog> {
    let c: Catalog = serde_json::from_str(json)?;
    for it in &c.items {
        if !matches!(it.winget_source.as_deref(), None | Some("winget") | Some("msstore")) {
            bail!("{}: winget_source must be winget or msstore", it.id);
        }
        // Validate at the trust boundary: a remote catalog must not smuggle in http:// or off-list hosts.
        if let Source::Direct { url } = &it.source {
            let u = reqwest::Url::parse(url).with_context(|| format!("{}: bad url", it.id))?;
            if u.scheme() != "https" || !util::host_allowed(u.host_str().unwrap_or(""), &it.allowed_hosts) {
                bail!("{}: url not https or host not in allowed_hosts", it.id);
            }
        }
        if let Source::Scrape { page, regex } = &it.source {
            let u = reqwest::Url::parse(page)?;
            if u.scheme() != "https" || !util::host_allowed(u.host_str().unwrap_or(""), &it.allowed_hosts) {
                bail!("{}: scrape page not allowed", it.id);
            }
            regex::Regex::new(regex)?;
        }
        if let Source::GithubRelease { asset_regex, .. } = &it.source {
            regex::Regex::new(asset_regex)?;
        }
        if let Some(d) = &it.defaults {
            validate_defaults(&it.id, d)?;
        }
    }
    Ok(c)
}

fn cached_path() -> std::path::PathBuf {
    util::app_dir().join("catalog.cached.json")
}

/// Remote (if configured and reachable) → last good remote copy → embedded.
pub async fn load(remote_url: &str) -> (Catalog, &'static str) {
    if !remote_url.is_empty() {
        match fetch_remote(remote_url).await {
            Ok(c) => return (c, "remote"),
            Err(e) => tracing::warn!("remote catalog failed: {e:#}"),
        }
        if let Ok(c) = std::fs::read_to_string(cached_path()).map_err(anyhow::Error::from).and_then(|s| parse(&s)) {
            return (c, "cached");
        }
    }
    (parse(EMBEDDED).expect("embedded catalog is valid"), "embedded")
}

pub async fn fetch_remote(url: &str) -> Result<Catalog> {
    let u = reqwest::Url::parse(url)?;
    if u.scheme() != "https" {
        bail!("catalog URL must be https");
    }
    let host = u.host_str().unwrap_or("").to_string();
    let text = util::http(vec![host, "githubusercontent.com".into()])
        .get(url)
        .timeout(Duration::from_secs(8))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let c = parse(&text)?;
    let _ = std::fs::write(cached_path(), &text);
    Ok(c)
}

#[derive(Debug, Clone, Serialize)]
pub struct Resolved {
    pub url: String,
    pub sha256: Option<String>,
    pub version: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct GhAsset {
    pub name: String,
    pub browser_download_url: String,
    pub digest: Option<String>,
    #[serde(default)]
    pub size: u64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct GhRelease {
    pub tag_name: String,
    pub assets: Vec<GhAsset>,
}

static GH_CACHE: Mutex<Option<HashMap<String, (Instant, GhRelease)>>> = Mutex::new(None);

/// Picks the first asset matching `re`, skipping updater/signature/source artifacts.
fn select_asset<'a>(assets: &'a [GhAsset], re: &str) -> Option<&'a GhAsset> {
    let re = regex::Regex::new(re).ok()?;
    let junk = regex::Regex::new(r"(?i)\.(blockmap|sig|sha256|asc|yml|json)$|source code").unwrap();
    assets.iter().find(|a| re.is_match(&a.name) && !junk.is_match(&a.name))
}

pub struct NoRelease;
impl std::fmt::Display for NoRelease {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("No release published yet")
    }
}
impl std::fmt::Debug for NoRelease {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}
impl std::error::Error for NoRelease {}

pub async fn github_latest(repo: &str, token: Option<&str>) -> Result<GhRelease> {
    if let Some((t, r)) = GH_CACHE.lock().unwrap().get_or_insert_with(HashMap::new).get(repo) {
        if t.elapsed() < Duration::from_secs(600) {
            return Ok(r.clone());
        }
    }
    let mut req = util::http(vec!["api.github.com".into()])
        .get(format!("https://api.github.com/repos/{repo}/releases/latest"))
        .header("Accept", "application/vnd.github+json");
    if let Some(t) = token.filter(|t| !t.is_empty()) {
        req = req.bearer_auth(t);
    }
    let resp = req.send().await?;
    match resp.status().as_u16() {
        404 => return Err(NoRelease.into()),
        403 | 429 => bail!("GitHub rate limit hit — add a token in Settings or try again in an hour"),
        _ => {}
    }
    let rel: GhRelease = resp.error_for_status()?.json().await?;
    GH_CACHE.lock().unwrap().get_or_insert_with(HashMap::new).insert(repo.to_string(), (Instant::now(), rel.clone()));
    Ok(rel)
}

pub async fn resolve(item: &Item, gh_token: Option<&str>) -> Result<Resolved> {
    match &item.source {
        Source::Winget => Err(anyhow!("{} is only available through winget", item.name)),
        Source::Direct { url } => Ok(Resolved { url: url.clone(), sha256: None, version: None }),
        Source::Scrape { page, regex } => {
            let body = util::http(item.allowed_hosts.clone())
                .get(page)
                .timeout(Duration::from_secs(20))
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;
            let url = scrape_url(&body, regex)
                .ok_or_else(|| anyhow!("download link not found on {page} — the vendor page layout changed"))?;
            Ok(Resolved { url, sha256: None, version: None })
        }
        Source::GithubRelease { repo, asset_regex } => {
            let rel = github_latest(repo, gh_token).await?;
            let a = select_asset(&rel.assets, asset_regex)
                .ok_or_else(|| anyhow!("release {} of {repo} has no Windows asset matching {asset_regex}", rel.tag_name))?;
            Ok(Resolved {
                url: a.browser_download_url.clone(),
                sha256: a.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")).map(str::to_string),
                version: Some(rel.tag_name.trim_start_matches('v').to_string()),
            })
        }
    }
}

fn scrape_url(body: &str, re: &str) -> Option<String> {
    regex::Regex::new(re).ok()?.find(body).map(|m| m.as_str().replace("&amp;", "&"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(n: &str) -> GhAsset {
        GhAsset { name: n.into(), browser_download_url: format!("https://x/{n}"), digest: Some("sha256:ab".into()), size: 1 }
    }

    #[test]
    fn embedded_catalog_parses_and_is_consistent() {
        let c = parse(EMBEDDED).unwrap();
        assert!(c.items.len() >= 18);
        let mut ids: Vec<_> = c.items.iter().map(|i| &i.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), c.items.len(), "duplicate ids");
        for i in &c.items {
            if matches!(i.source, Source::Winget) {
                assert!(i.winget_id.is_some(), "{} winget source without id", i.id);
            } else {
                assert!(!i.allowed_hosts.is_empty(), "{} has no allowed_hosts", i.id);
            }
        }
    }

    #[test]
    fn remote_catalog_rejects_off_list_host() {
        let bad = EMBEDDED.replace("https://download.scdn.co/SpotifySetup.exe", "https://evil.example/x.exe");
        assert!(parse(&bad).is_err());
        let http = EMBEDDED.replace("https://download.scdn.co/", "http://download.scdn.co/");
        assert!(parse(&http).is_err());
        // Safety flags are runtime-only: a catalog that tries to set them is ignored.
        let sneaky = EMBEDDED.replace(r#""id": "spotify","#, r#""id": "spotify", "allow_unsigned": true, "reviewed": true, "allow_http": true,"#);
        let c = parse(&sneaky).unwrap();
        let s = c.items.iter().find(|i| i.id == "spotify").unwrap();
        assert!(!s.allow_unsigned && !s.reviewed && !s.allow_http && !s.custom);
    }

    #[test]
    fn asset_selection() {
        let assets = vec![
            a("CompressO_3.0.0_x64.exe.sig"),
            a("CompressO_3.0.0_x64.app.tar.gz"),
            a("CompressO_3.0.0_x64.exe"),
            a("CompressO_3.0.0_x64.dmg"),
        ];
        assert_eq!(select_asset(&assets, r"_x64\.exe$").unwrap().name, "CompressO_3.0.0_x64.exe");

        let pdf = vec![a("pdfcraft-0.4.0-windows-arm64.msi"), a("pdfcraft-0.4.0-windows-x64.msi"), a("SHA256SUMS.txt")];
        assert_eq!(select_asset(&pdf, r"windows-x64\.msi$").unwrap().name, "pdfcraft-0.4.0-windows-x64.msi");

        let blk = vec![a("App-Setup.exe.blockmap"), a("App-Setup.exe")];
        assert_eq!(select_asset(&blk, r"-Setup\.exe").unwrap().name, "App-Setup.exe");
        assert!(select_asset(&blk, r"\.msi$").is_none());
    }

    #[test]
    fn scrape_jsonp_and_html() {
        let c = parse(EMBEDDED).unwrap();
        let re = |id: &str| match &c.items.iter().find(|i| i.id == id).unwrap().source {
            Source::Scrape { regex, .. } => regex.clone(),
            _ => unreachable!(),
        };
        let jsonp = "getDlUrl({'dlUrl': 'https://download.booster.gearupportal.com/9273/GearUP-3.4.1-win.exe'});";
        assert_eq!(scrape_url(jsonp, &re("gearup")).unwrap(), "https://download.booster.gearupportal.com/9273/GearUP-3.4.1-win.exe");
        let html = r#"<a href="https://avamodmanager.com/downloads/AVAModManager-v0.9.4-setup.exe">Download</a>"#;
        assert_eq!(scrape_url(html, &re("ava")).unwrap(), "https://avamodmanager.com/downloads/AVAModManager-v0.9.4-setup.exe");
        assert!(scrape_url("<html>redesigned</html>", &re("ava")).is_none());
    }
}
