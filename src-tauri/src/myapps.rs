//! "My Apps": user-added programs. Entries are stored in %APPDATA%\SetupHub\my_apps.json, converted to
//! catalog `Item`s and installed by the same engine as the built-in catalog.

use crate::catalog::{self, Detect, GhAsset, I18n, Item, Mode, Source, ZipSpec};
use crate::installer::{self, Kind, Outcome};
use crate::{detect, drive, settings, sig, util};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const SCHEMA_VERSION: u32 = 1;
const INSTALLER_EXTS: [&str; 8] = ["exe", "msi", "msix", "msixbundle", "appx", "appxbundle", "zip", "7z"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MySource {
    /// Direct file link; `hosts` = registrable domains seen while following its redirects.
    Url { url: String, #[serde(default)] hosts: Vec<String> },
    /// GitHub repo: "latest" is re-resolved on every install.
    Github { repo: String, asset_regex: String },
    /// winget package; `store` is "winget" or "msstore".
    Winget { id: String, #[serde(default = "default_store")] store: String },
}

fn default_store() -> String {
    "winget".into()
}
fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MyApp {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub publisher: Option<String>,
    pub source: MySource,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub silent_args: Vec<String>,
    #[serde(default)]
    pub detect: Detect,
    #[serde(default = "yes")]
    pub include_in_install_all: bool,
    /// Unix seconds.
    #[serde(default)]
    pub added_at: u64,
    #[serde(default)]
    pub extract_to: Option<String>,
    /// Exe (relative to extract_to) that gets a Start Menu shortcut.
    #[serde(default)]
    pub shortcut: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub file_type: Option<String>,
    // Filled after the first download:
    #[serde(default)]
    pub installer_type: Option<Kind>,
    #[serde(default)]
    pub signature: Option<String>,
    /// Authenticode signer; pinned so a later file signed by someone else fails.
    #[serde(default)]
    pub signer: Option<String>,
    #[serde(default)]
    pub reviewed: bool,
    #[serde(default)]
    pub allow_unsigned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Store {
    pub version: u32,
    pub apps: Vec<MyApp>,
}

// ---------- persistence ----------

pub fn path() -> PathBuf {
    let d = PathBuf::from(std::env::var("APPDATA").unwrap_or_else(|_| ".".into())).join("SetupHub");
    let _ = std::fs::create_dir_all(&d);
    d.join("my_apps.json")
}

pub fn load() -> Vec<MyApp> {
    let p = path();
    let Ok(text) = std::fs::read_to_string(&p) else { return Vec::new() };
    match parse_store(&text, true) {
        Ok(s) => s.apps,
        Err(e) => {
            // Keep the broken file for the user instead of silently overwriting it on the next save.
            let bak = p.with_extension(format!("invalid-{}.json", now()));
            tracing::error!("my_apps.json invalid ({e:#}); moved to {}", bak.display());
            let _ = std::fs::rename(&p, bak);
            Vec::new()
        }
    }
}

pub fn save(apps: &[MyApp]) -> Result<()> {
    let s = Store { version: SCHEMA_VERSION, apps: apps.to_vec() };
    let tmp = path().with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&s)?)?;
    std::fs::rename(tmp, path())?;
    Ok(())
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Parses and validates a store. `allow_http` relaxes only the URL scheme rule.
pub fn parse_store(text: &str, allow_http: bool) -> Result<Store> {
    let s: Store = serde_json::from_str(text).context("not a My Apps list (invalid JSON)")?;
    if s.version != SCHEMA_VERSION {
        bail!("unsupported My Apps schema version {} (expected {SCHEMA_VERSION})", s.version);
    }
    let mut ids = std::collections::HashSet::new();
    for a in &s.apps {
        validate(a, allow_http)?;
        if !ids.insert(&a.id) {
            bail!("duplicate id {}", a.id);
        }
    }
    Ok(s)
}

pub fn validate(a: &MyApp, allow_http: bool) -> Result<()> {
    let id_re = regex::Regex::new(r"^[a-z0-9][a-z0-9-]{0,63}$").unwrap();
    if !id_re.is_match(&a.id) {
        bail!("invalid id '{}'", a.id);
    }
    if a.name.trim().is_empty() || a.name.len() > 120 {
        bail!("{}: name must be 1–120 characters", a.id);
    }
    match &a.source {
        MySource::Url { url, .. } => check_url(url, allow_http).with_context(|| a.id.clone())?,
        MySource::Github { repo, asset_regex } => {
            if !regex::Regex::new(r"^[\w.-]+/[\w.-]+$").unwrap().is_match(repo) {
                bail!("{}: invalid GitHub repo '{repo}'", a.id);
            }
            regex::Regex::new(asset_regex).with_context(|| format!("{}: bad asset regex", a.id))?;
        }
        MySource::Winget { id, store } => {
            if !regex::Regex::new(r"^[\w.+-]{2,128}$").unwrap().is_match(id) || !matches!(store.as_str(), "winget" | "msstore") {
                bail!("{}: invalid winget id/source", a.id);
            }
        }
    }
    for x in &a.silent_args {
        if x.len() > 300 || x.contains(['\n', '\r', '&', '|', '<', '>', '^']) {
            bail!("{}: argument '{x}' contains characters that are not allowed", a.id);
        }
    }
    if let Some(s) = &a.shortcut {
        if s.contains("..") || Path::new(s).is_absolute() {
            bail!("{}: shortcut must be a path inside the extract folder", a.id);
        }
    }
    Ok(())
}

fn check_url(url: &str, allow_http: bool) -> Result<()> {
    let u = reqwest::Url::parse(url).context("not a valid URL")?;
    match u.scheme() {
        "https" => {}
        "http" if allow_http => {}
        "http" => bail!("plain HTTP links are blocked (enable \"Allow plain HTTP links\" in Settings)"),
        s => bail!("unsupported scheme {s}"),
    }
    if installer::is_script(u.path()) {
        bail!("scripts are never run from links");
    }
    Ok(())
}

/// "download.cdn.viber.com" → "viber.com" (CDN hosts rotate; the registrable domain stays).
pub fn base_domain(host: &str) -> String {
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() <= 2 || host.parse::<std::net::IpAddr>().is_ok() {
        return host.to_string();
    }
    labels[labels.len() - 2..].join(".")
}

impl MyApp {
    pub fn to_item(&self, allow_http: bool) -> Item {
        let (source, winget_id, winget_source, hosts) = match &self.source {
            MySource::Url { url, hosts } => {
                let mut h = hosts.clone();
                if let Some(host) = reqwest::Url::parse(url).ok().and_then(|u| u.host_str().map(base_domain)) {
                    h.push(host);
                }
                (Source::Direct { url: url.clone() }, None, None, h)
            }
            MySource::Github { repo, asset_regex } => (
                Source::GithubRelease { repo: repo.clone(), asset_regex: asset_regex.clone() },
                None,
                None,
                vec!["github.com".into(), "githubusercontent.com".into()],
            ),
            MySource::Winget { id, store } => (Source::Winget, Some(id.clone()), Some(store.clone()), vec![]),
        };
        Item {
            id: self.id.clone(),
            name: self.name.clone(),
            category: "custom".into(),
            description: I18n { en: self.publisher.clone().unwrap_or_default(), ru: self.publisher.clone().unwrap_or_default() },
            winget_id,
            source,
            silent_args: if self.mode == Mode::Interactive { vec![] } else { self.silent_args.clone() },
            detect: self.detect.clone(),
            allowed_hosts: hosts,
            publisher: self.signer.clone(),
            zip: (self.mode == Mode::Extract).then(|| ZipSpec {
                run: None,
                extract_to: Some(self.extract_to.clone().unwrap_or_else(|| default_extract_dir(&self.name))),
                shortcut: self.shortcut.clone(),
                launch: false,
            }),
            needs_reboot: false,
            interactive: self.mode == Mode::Interactive,
            unelevated: false,
            note: None,
            mode: self.mode,
            winget_source,
            custom: true,
            reviewed: self.reviewed || matches!(self.source, MySource::Winget { .. }),
            allow_unsigned: self.allow_unsigned,
            allow_http,
        }
    }
}

pub fn default_extract_dir(name: &str) -> String {
    format!(r"%LOCALAPPDATA%\Programs\{}", crate::download::sanitize(name))
}

pub fn new_id(name: &str, taken: &[String]) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let base = format!("my-{}", if slug.is_empty() { "app".into() } else { slug.chars().take(40).collect::<String>() });
    let mut id = base.clone();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

// ---------- link classification ----------

#[derive(Debug, PartialEq)]
pub enum LinkKind {
    Github { repo: String },
    Drive { file: Option<String>, folder: Option<String> },
    Store(String),
    Script,
    Url(String),
    WingetId(String),
    Text(String),
    Invalid,
}

pub fn classify(input: &str) -> LinkKind {
    let s = input.trim();
    if s.is_empty() {
        return LinkKind::Invalid;
    }
    let store_id = regex::Regex::new(r"(?i)^(9[0-9A-Z]{11}|XP[0-9A-Z]{12})$").unwrap();
    if store_id.is_match(s) {
        return LinkKind::Store(s.to_uppercase());
    }
    let looks_like_url = s.contains("://") || (!s.contains(' ') && s.contains('/') && s.split('/').next().is_some_and(|h| h.contains('.')));
    if looks_like_url {
        let full = if s.contains("://") { s.to_string() } else { format!("https://{s}") };
        let Ok(u) = reqwest::Url::parse(&full) else { return LinkKind::Invalid };
        if !matches!(u.scheme(), "http" | "https") {
            return LinkKind::Invalid;
        }
        if installer::is_script(u.path()) {
            return LinkKind::Script;
        }
        let host = u.host_str().unwrap_or("").to_lowercase();
        let segs: Vec<&str> = u.path_segments().map(|p| p.filter(|x| !x.is_empty()).collect()).unwrap_or_default();
        if (host == "github.com" || host == "www.github.com") && segs.len() >= 2 && !u.path().contains("/releases/download/") {
            return LinkKind::Github { repo: format!("{}/{}", segs[0], segs[1].trim_end_matches(".git")) };
        }
        if host == "drive.google.com" || host == "drive.usercontent.google.com" || host == "docs.google.com" {
            let q = |k: &str| u.query_pairs().find(|(n, _)| n == k).map(|(_, v)| v.to_string());
            if let Some(i) = segs.iter().position(|x| *x == "folders") {
                return LinkKind::Drive { file: None, folder: segs.get(i + 1).map(|x| x.to_string()) };
            }
            if let Some(i) = segs.iter().position(|x| *x == "d") {
                return LinkKind::Drive { file: segs.get(i + 1).map(|x| x.to_string()), folder: None };
            }
            if let Some(id) = q("id") {
                return LinkKind::Drive { file: Some(id), folder: None };
            }
        }
        if host.ends_with("microsoft.com") {
            if let Some(id) = segs.iter().find(|x| store_id.is_match(x)) {
                return LinkKind::Store(id.to_uppercase());
            }
        }
        return LinkKind::Url(full);
    }
    if regex::Regex::new(r"^[A-Za-z0-9][\w+-]*(\.[\w+-]+)+$").unwrap().is_match(s) {
        return LinkKind::WingetId(s.to_string());
    }
    LinkKind::Text(s.to_string())
}

// ---------- analysis ----------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WingetMatch {
    pub name: String,
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Analysis {
    Ready { input: String, entry: MyApp },
    WingetChoices { input: String, matches: Vec<WingetMatch> },
    PageLinks { input: String, page: String, links: Vec<String> },
    Drive { input: String, file: Option<String>, folder: Option<String>, name: Option<String> },
    Error { input: String, message: String, hint: Option<String>, open_url: Option<String> },
}

fn err(input: &str, message: impl Into<String>, hint: Option<&str>, open_url: Option<String>) -> Analysis {
    Analysis::Error { input: input.into(), message: message.into(), hint: hint.map(str::to_string), open_url }
}

fn blank(name: &str, source: MySource) -> MyApp {
    MyApp {
        id: String::new(),
        name: name.to_string(),
        icon: None,
        publisher: None,
        source,
        mode: Mode::Silent,
        silent_args: vec![],
        detect: Detect::default(),
        include_in_install_all: true,
        added_at: now(),
        extract_to: None,
        shortcut: None,
        size: None,
        version: None,
        file_type: None,
        installer_type: None,
        signature: None,
        signer: None,
        reviewed: false,
        allow_unsigned: false,
    }
}

/// Default mode/args from the file extension; exe args are suggested after the first download.
fn apply_type_defaults(e: &mut MyApp, file_name: &str) {
    let ext = installer::ext_of(file_name);
    e.file_type = Some(ext.clone());
    match ext.as_str() {
        "msi" => e.silent_args = installer::suggested_args(Kind::Msi),
        "zip" | "7z" => {
            e.mode = Mode::Extract;
            e.extract_to = Some(default_extract_dir(&e.name));
        }
        _ => {}
    }
}

pub async fn analyze(input: &str, allow_http: bool, gh_token: Option<&str>) -> Analysis {
    match analyze_inner(input, allow_http, gh_token).await {
        Ok(a) => a,
        Err(e) => err(input, format!("{e:#}"), None, None),
    }
}

async fn analyze_inner(input: &str, allow_http: bool, gh_token: Option<&str>) -> Result<Analysis> {
    let input = input.trim();
    Ok(match classify(input) {
        LinkKind::Invalid => err(input, "Not a link, winget ID or app name", Some("Paste an https:// link, a GitHub repo, a winget ID like Vendor.App, or a name to search"), None),
        LinkKind::Script => err(input, "Scripts (.bat, .cmd, .ps1, .vbs, .js, .hta, .reg) are never run from links", Some("Download it yourself and inspect it before running"), None),
        LinkKind::Drive { file, folder } => {
            let name = match &file {
                Some(id) => drive::file_name(id).await.ok(),
                None => None,
            };
            Analysis::Drive { input: input.into(), file, folder, name }
        }
        LinkKind::Github { repo } => analyze_github(input, &repo, gh_token).await?,
        LinkKind::Store(id) => analyze_winget(input, &id, "msstore").await?.unwrap_or_else(|| {
            err(input, format!("Microsoft Store product {id} was not found"), Some("Check the product ID in the Store link"), None)
        }),
        LinkKind::WingetId(id) => match analyze_winget(input, &id, "winget").await? {
            Some(a) => a,
            None => search(input).await?,
        },
        LinkKind::Text(_) => search(input).await?,
        LinkKind::Url(url) => analyze_url(input, &url, allow_http).await?,
    })
}

async fn analyze_github(input: &str, repo: &str, token: Option<&str>) -> Result<Analysis> {
    let rel = match catalog::github_latest(repo, token).await {
        Ok(r) => r,
        Err(e) if e.is::<catalog::NoRelease>() => {
            return Ok(err(input, format!("{repo} has no published release yet"), None, Some(format!("https://github.com/{repo}"))))
        }
        Err(e) => return Err(e),
    };
    let Some(a) = pick_windows_asset(&rel.assets) else {
        return Ok(err(
            input,
            format!("Release {} of {repo} has no Windows installer or archive", rel.tag_name),
            Some("Pick a direct file link from the release page instead"),
            Some(format!("https://github.com/{repo}/releases/latest")),
        ));
    };
    let owner = repo.split('/').next().unwrap_or("");
    let name = repo.split('/').nth(1).unwrap_or(repo);
    let mut e = blank(name, MySource::Github { repo: repo.into(), asset_regex: asset_regex(&a.name, &rel.tag_name) });
    e.publisher = Some(owner.into());
    e.icon = Some(format!("https://github.com/{owner}.png?size=96"));
    e.size = Some(a.size);
    e.version = Some(rel.tag_name.trim_start_matches('v').into());
    apply_type_defaults(&mut e, &a.name);
    Ok(Analysis::Ready { input: input.into(), entry: e })
}

/// Picks the best Windows x64 asset: .msi > *setup*.exe > .msix > .exe > .zip > .7z.
pub fn pick_windows_asset(assets: &[GhAsset]) -> Option<&GhAsset> {
    let junk = regex::Regex::new(r"(?i)\.(blockmap|sig|sha\d*|asc|ya?ml|json|txt|pdb|xml|dmg|appimage|deb|rpm|apk|pkg|flatpak|zsync|tar|gz|xz|zst|bz2)$|debug|symbols|source").unwrap();
    let other_arch = regex::Regex::new(r"(?i)arm64|aarch64|[-_.]arm[-_.]|[-_.](x86|i386|i686|win32|32-?bit)[-_.]").unwrap();
    let prerelease = regex::Regex::new(r"(?i)preview|beta|nightly|alpha|canary").unwrap();
    let rank = |n: &str| -> Option<u8> {
        let l = n.to_lowercase();
        match installer::ext_of(&l).as_str() {
            "msi" => Some(0),
            "exe" if l.contains("setup") || l.contains("install") => Some(1),
            "msix" | "msixbundle" | "appx" | "appxbundle" => Some(2),
            "exe" => Some(3),
            "zip" if l.contains("win") || l.contains("portable") || l.contains("x64") => Some(4),
            "zip" => Some(5),
            "7z" => Some(6),
            _ => None,
        }
    };
    let cands: Vec<&GhAsset> = assets.iter().filter(|a| !junk.is_match(&a.name) && rank(&a.name).is_some()).collect();
    let x64: Vec<&GhAsset> = cands.iter().copied().filter(|a| !other_arch.is_match(&a.name)).collect();
    let pool = if x64.is_empty() { cands } else { x64 };
    pool.into_iter().min_by_key(|a| (prerelease.is_match(&a.name), rank(&a.name).unwrap()))
}

/// "CompressO_3.0.0_x64.exe" + tag "3.0.0" → `^CompressO_[0-9][0-9.]*_x64\.exe$`, so the next release still matches.
pub fn asset_regex(name: &str, tag: &str) -> String {
    let v = tag.trim_start_matches(['v', 'V']);
    for ver in [v.to_string(), v.replace('.', "_")] {
        if !ver.is_empty() {
            if let Some(i) = name.find(&ver) {
                return format!("^{}[0-9][0-9._]*{}$", regex::escape(&name[..i]), regex::escape(&name[i + ver.len()..]));
            }
        }
    }
    format!("^{}$", regex::escape(name))
}

async fn winget_text(args: &[&str]) -> Result<(i32, String)> {
    util::run("winget.exe", args).await
}

/// `winget show` → entry, or None if the id doesn't exist.
async fn analyze_winget(input: &str, id: &str, store: &str) -> Result<Option<Analysis>> {
    if !installer::ensure_winget().await {
        bail!("winget is not available on this PC");
    }
    let (code, out) = winget_text(&["show", "--id", id, "-e", "--source", store, "--accept-source-agreements", "--disable-interactivity"]).await?;
    if code != 0 {
        return Ok(None);
    }
    let Some((name, real_id, publisher, version)) = parse_winget_show(&out) else { return Ok(None) };
    let mut e = blank(&name, MySource::Winget { id: real_id, store: store.into() });
    e.publisher = publisher;
    e.version = version;
    e.file_type = Some(if store == "msstore" { "Microsoft Store".into() } else { "winget".into() });
    e.reviewed = true;
    Ok(Some(Analysis::Ready { input: input.into(), entry: e }))
}

fn clean_lines(text: &str) -> Vec<String> {
    text.lines().map(|l| l.rsplit('\r').next().unwrap_or(l).trim_end().to_string()).collect()
}

/// (name, id, publisher, version) from `winget show` (first line "Found <Name> [<Id>]" in any UI language).
pub fn parse_winget_show(text: &str) -> Option<(String, String, Option<String>, Option<String>)> {
    let lines = clean_lines(text);
    let head = regex::Regex::new(r"^\S+\s+(.+?)\s+\[([^\]\s]+)\]\s*$").unwrap();
    let (name, id) = lines.iter().find_map(|l| head.captures(l.trim()).map(|c| (c[1].to_string(), c[2].to_string())))?;
    let field = |keys: &str| {
        let re = regex::Regex::new(&format!(r"(?i)^\s*(?:{keys}):\s*(.+)$")).unwrap();
        lines.iter().find_map(|l| re.captures(l).map(|c| c[1].trim().to_string()))
    };
    Some((name, id, field("Publisher|Издатель"), field("Version|Версия")))
}

/// Parses `winget search` table output using the header's column offsets (works in any UI language).
pub fn parse_winget_table(text: &str) -> Vec<WingetMatch> {
    let lines = clean_lines(text);
    let Some(sep) = lines.iter().position(|l| l.trim().len() > 10 && l.trim().chars().all(|c| c == '-')) else { return vec![] };
    let Some(header) = sep.checked_sub(1).and_then(|i| lines.get(i)) else { return vec![] };
    let hc: Vec<char> = header.chars().collect();
    let starts: Vec<usize> = (0..hc.len()).filter(|&i| hc[i] != ' ' && (i == 0 || hc[i - 1] == ' ')).collect();
    if starts.len() < 3 {
        return vec![];
    }
    let col = |row: &[char], n: usize| -> String {
        let a = starts[n].min(row.len());
        let b = starts.get(n + 1).copied().unwrap_or(row.len()).min(row.len());
        row[a..b].iter().collect::<String>().trim().to_string()
    };
    lines[sep + 1..]
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let r: Vec<char> = l.chars().collect();
            WingetMatch { name: col(&r, 0), id: col(&r, 1), version: col(&r, 2) }
        })
        .filter(|m| !m.id.is_empty() && !m.id.contains('…') && !m.id.contains(' '))
        .take(8)
        .collect()
}

async fn search(input: &str) -> Result<Analysis> {
    if !installer::ensure_winget().await {
        bail!("winget is not available on this PC");
    }
    let (_, out) = winget_text(&["search", input, "--source", "winget", "--accept-source-agreements", "--disable-interactivity"]).await?;
    let matches = parse_winget_table(&out);
    Ok(if matches.is_empty() {
        err(input, format!("No winget package matches \"{input}\""), Some("Paste the download link or the vendor's page instead"), None)
    } else {
        Analysis::WingetChoices { input: input.into(), matches }
    })
}

struct Head {
    final_url: String,
    hosts: Vec<String>,
    content_type: String,
    len: Option<u64>,
    file_name: String,
}

async fn head(url: &str, allow_http: bool) -> Result<Head> {
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let s2 = seen.clone();
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::custom(move |a| {
            let ok = a.url().scheme() == "https" || (allow_http && a.url().scheme() == "http");
            if !ok {
                return a.error("redirect to a plain-HTTP link");
            }
            if let Some(h) = a.url().host_str() {
                s2.lock().unwrap().push(base_domain(h));
            }
            if a.previous().len() > 10 { a.error("too many redirects") } else { a.follow() }
        }))
        .build()?;
    let mut r = client.head(url).send().await?;
    if !r.status().is_success() {
        // Many CDNs reject HEAD; a 1-byte ranged GET gives the same headers.
        r = client.get(url).header("Range", "bytes=0-0").send().await?;
    }
    let r = r.error_for_status()?;
    let h = r.headers();
    let len = h
        .get("content-range")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit('/').next()?.parse().ok())
        .or_else(|| r.content_length().filter(|l| *l > 1));
    let final_url = r.url().to_string();
    let disp = h.get("content-disposition").and_then(|v| v.to_str().ok()).and_then(crate::download::parse_disposition);
    let from_url = r.url().path_segments().and_then(|mut s| s.next_back()).map(str::to_string).unwrap_or_default();
    let mut hosts = seen.lock().unwrap().clone();
    hosts.sort();
    hosts.dedup();
    Ok(Head {
        content_type: h.get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_lowercase(),
        file_name: crate::download::sanitize(&disp.unwrap_or(from_url)),
        final_url,
        hosts,
        len,
    })
}

async fn analyze_url(input: &str, url: &str, allow_http: bool) -> Result<Analysis> {
    if let Err(e) = check_url(url, allow_http) {
        let hint = url.starts_with("http://").then_some("Settings → Allow plain HTTP links");
        return Ok(err(input, format!("{e:#}"), hint, None));
    }
    let h = head(url, allow_http).await?;
    if installer::is_script(&h.file_name) {
        return Ok(err(input, "This link downloads a script — scripts are never run from links", None, None));
    }
    if INSTALLER_EXTS.contains(&installer::ext_of(&h.file_name).as_str()) {
        let host = reqwest::Url::parse(url)?.host_str().map(base_domain).unwrap_or_default();
        let mut e = blank(&pretty_name(&h.file_name), MySource::Url { url: url.into(), hosts: h.hosts });
        e.publisher = Some(host);
        e.size = h.len;
        apply_type_defaults(&mut e, &h.file_name);
        return Ok(Analysis::Ready { input: input.into(), entry: e });
    }
    if h.content_type.starts_with("text/html") {
        let body = util::http_opts(vec![reqwest::Url::parse(&h.final_url)?.host_str().map(base_domain).unwrap_or_default()], allow_http)
            .get(&h.final_url)
            .timeout(Duration::from_secs(20))
            .send()
            .await?
            .text()
            .await?;
        let links = page_links(&h.final_url, &body[..body.len().min(4 << 20)], allow_http);
        return Ok(if links.is_empty() {
            err(input, "No Windows download links found on this page", Some("Open the page and copy the direct download link"), Some(h.final_url))
        } else {
            Analysis::PageLinks { input: input.into(), page: h.final_url, links }
        });
    }
    Ok(err(input, format!("This link is not an installer (type: {})", if h.content_type.is_empty() { "unknown" } else { &h.content_type }), Some("Use a direct link to an .exe, .msi, .msix, .zip or .7z"), Some(url.into())))
}

/// Candidate Windows download links on an HTML page, absolute and deduplicated.
pub fn page_links(base: &str, html: &str, allow_http: bool) -> Vec<String> {
    let Ok(base) = reqwest::Url::parse(base) else { return vec![] };
    let re = regex::Regex::new(r#"(?i)href\s*=\s*["']([^"'#]+)["']"#).unwrap();
    let mut out: Vec<String> = Vec::new();
    for c in re.captures_iter(html) {
        let Ok(u) = base.join(c[1].trim().replace("&amp;", "&").as_str()) else { continue };
        let ok_scheme = u.scheme() == "https" || (allow_http && u.scheme() == "http");
        let ext = installer::ext_of(u.path());
        if ok_scheme && INSTALLER_EXTS.contains(&ext.as_str()) && !out.contains(&u.to_string()) {
            out.push(u.to_string());
        }
        if out.len() >= 20 {
            break;
        }
    }
    out
}

/// "AVAModManager-v0.9.4-setup.exe" → "AVAModManager", "ViberSetup.msi" → "Viber".
pub fn pretty_name(file: &str) -> String {
    let stem = Path::new(file).file_stem().and_then(|s| s.to_str()).unwrap_or(file).to_string();
    let noise = regex::Regex::new(r"(?i)^(v?\d+([._]\d+)*|setup|installer|install|x64|x86|win64|win32|amd64|arm64|windows|win|full|latest|portable|release)$").unwrap();
    let words: Vec<&str> = stem.split(['_', '-', ' ', '.']).filter(|w| !w.is_empty() && !noise.is_match(w)).collect();
    let mut name = if words.is_empty() { stem.clone() } else { words.join(" ") };
    let suffix = regex::Regex::new(r"(?i)(setup|installer)$").unwrap();
    if name.len() > 7 {
        name = suffix.replace(&name, "").trim().to_string();
    }
    let mut c = name.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => stem,
    }
}

// ---------- after the first download ----------

pub struct Inspection {
    pub kind: Kind,
    pub status: String,
    pub signer: Option<String>,
    pub icon: Option<String>,
    pub size: u64,
}

pub async fn inspect(file: &Path) -> Result<Inspection> {
    let kind = installer::detect_kind(file);
    let (status, signer) = if matches!(kind, Kind::Zip | Kind::SevenZip) {
        ("Archive".to_string(), None)
    } else {
        let s = sig::authenticode(file).await?;
        (s.status, s.publisher)
    };
    let icon = if installer::ext_of(&file.to_string_lossy()) == "exe" { exe_icon(file).await.ok() } else { None };
    Ok(Inspection { kind, status, signer, icon, size: std::fs::metadata(file).map(|m| m.len()).unwrap_or(0) })
}

/// The exe's own icon as a PNG data URL (local; no network).
async fn exe_icon(file: &Path) -> Result<String> {
    let p = file.display().to_string().replace('\'', "''");
    let v = util::ps_json(&format!(
        "Add-Type -AssemblyName System.Drawing;$i=[System.Drawing.Icon]::ExtractAssociatedIcon('{p}');$b=$i.ToBitmap();$m=New-Object IO.MemoryStream;$b.Save($m,[System.Drawing.Imaging.ImageFormat]::Png);ConvertTo-Json ([Convert]::ToBase64String($m.ToArray()))"
    ))
    .await?;
    Ok(format!("data:image/png;base64,{}", v.as_str().ok_or_else(|| anyhow!("no icon"))?))
}

// ---------- Check links ----------

#[derive(Debug, Clone, Serialize, Default)]
pub struct LinkStatus {
    pub error: Option<String>,
    /// Latest release version (GitHub entries), for "Update available".
    pub latest: Option<String>,
}

pub async fn check(app: &MyApp, allow_http: bool, token: Option<&str>) -> LinkStatus {
    let item = app.to_item(allow_http);
    let r = match &app.source {
        MySource::Winget { .. } => return LinkStatus::default(),
        MySource::Github { .. } => catalog::resolve(&item, token).await.map(|r| r.version),
        MySource::Url { url, .. } => head(url, allow_http).await.and_then(|h| {
            let host = reqwest::Url::parse(&h.final_url)?.host_str().unwrap_or("").to_string();
            if util::host_allowed(&host, &item.allowed_hosts) {
                Ok(None)
            } else {
                Err(anyhow!("now redirects to {host}, which is not where this link pointed before — use Change link"))
            }
        }),
    };
    match r {
        Ok(latest) => LinkStatus { error: None, latest },
        Err(e) => LinkStatus { error: Some(format!("{e:#}")), latest: None },
    }
}

// ---------- Uninstall ----------

/// "MsiExec.exe /I{GUID}" → "{GUID}" (uninstall through msiexec /x quietly instead).
pub fn msi_product(cmd: &str) -> Option<String> {
    regex::Regex::new(r"(?i)msiexec(?:\.exe)?\s+/[ix]\s*(\{[0-9a-f-]{36}\})").unwrap().captures(cmd).map(|c| c[1].to_uppercase())
}

pub async fn uninstall(app: &MyApp, cancel: &AtomicBool) -> Result<Outcome> {
    if let MySource::Winget { id, store } = &app.source {
        let (code, out) = util::run("winget.exe", &["uninstall", "--id", id, "-e", "--source", store, "--silent", "--accept-source-agreements", "--disable-interactivity"]).await?;
        return installer::exit_outcome(code).with_context(|| out.lines().last().unwrap_or("").to_string());
    }
    if let Some(prefix) = &app.detect.appx {
        let name = prefix.trim_end_matches('_').replace('\'', "''");
        util::ps_json(&format!("Get-AppxPackage -Name '{name}' | Remove-AppxPackage -ErrorAction Stop; ConvertTo-Json $true")).await?;
        return Ok(Outcome::Ok);
    }
    let key = app.detect.uninstall_key.as_deref().ok_or_else(|| anyhow!("no uninstall information recorded for {} (install it once through Setup Hub)", app.name))?;
    let e = detect::uninstall_entries()
        .into_iter()
        .find(|e| e.key.eq_ignore_ascii_case(key))
        .ok_or_else(|| anyhow!("{} is not installed", app.name))?;
    if let Some(guid) = msi_product(&e.uninstall) {
        let (c, _) = util::run("msiexec.exe", &["/x", &guid, "/qn", "/norestart"]).await?;
        return installer::exit_outcome(c);
    }
    let line = if e.quiet_uninstall.is_empty() { e.uninstall.clone() } else { e.quiet_uninstall.clone() };
    if line.trim().is_empty() {
        bail!("{} has no uninstall command", app.name);
    }
    // The registry string is a full command line; cmd /c runs it exactly as Windows' Apps settings would.
    let mut child = tokio::process::Command::new("cmd.exe").raw_arg(format!("/c \"{line}\"")).spawn()?;
    loop {
        tokio::select! {
            st = child.wait() => return installer::exit_outcome(st?.code().unwrap_or(-1)),
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) { let _ = child.kill().await; bail!(crate::download::Cancelled); }
            }
        }
    }
}

/// Imported entries must be re-reviewed on this PC: signatures and "install anyway" are not trusted from a file.
pub fn sanitize_import(mut apps: Vec<MyApp>, existing: &[MyApp]) -> Vec<MyApp> {
    let mut taken: Vec<String> = existing.iter().map(|a| a.id.clone()).collect();
    apps.retain(|a| !existing.iter().any(|e| e.source == a.source));
    for a in &mut apps {
        if !matches!(a.source, MySource::Winget { .. }) {
            a.reviewed = false;
        }
        a.allow_unsigned = false;
        a.signature = None;
        if taken.contains(&a.id) {
            a.id = new_id(&a.name, &taken);
        }
        taken.push(a.id.clone());
    }
    apps
}

/// Turns a Google Drive file link into its direct download URL so a list can be kept in Drive.
pub fn list_url(url: &str) -> String {
    match classify(url) {
        LinkKind::Drive { file: Some(id), .. } => drive::file_url(&id),
        _ => url.trim().to_string(),
    }
}

pub fn allow_http() -> bool {
    settings::load().allow_http
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(n: &str) -> GhAsset {
        GhAsset { name: n.into(), browser_download_url: format!("https://github.com/o/r/releases/download/1/{n}"), digest: None, size: 10 }
    }

    #[test]
    fn link_classification() {
        assert_eq!(classify("https://github.com/M2Team/NanaZip"), LinkKind::Github { repo: "M2Team/NanaZip".into() });
        assert_eq!(classify("github.com/ONLYOFFICE/DesktopEditors/releases"), LinkKind::Github { repo: "ONLYOFFICE/DesktopEditors".into() });
        assert_eq!(classify("https://github.com/a/b.git"), LinkKind::Github { repo: "a/b".into() });
        assert!(matches!(classify("https://github.com/a/b/releases/download/v1/x.exe"), LinkKind::Url(_)));
        assert_eq!(classify("https://drive.google.com/file/d/ABC_12/view?usp=sharing"), LinkKind::Drive { file: Some("ABC_12".into()), folder: None });
        assert_eq!(classify("https://drive.google.com/drive/folders/XYZ?usp=sharing"), LinkKind::Drive { file: None, folder: Some("XYZ".into()) });
        assert_eq!(classify("https://drive.google.com/open?id=QQ"), LinkKind::Drive { file: Some("QQ".into()), folder: None });
        assert_eq!(classify("9NBLGGH4NNS1"), LinkKind::Store("9NBLGGH4NNS1".into()));
        assert_eq!(classify("https://apps.microsoft.com/detail/9nblggh4nns1?hl=en-us"), LinkKind::Store("9NBLGGH4NNS1".into()));
        assert_eq!(classify("https://apps.microsoft.com/store/detail/whatsapp/9NKSQGP7F2NH"), LinkKind::Store("9NKSQGP7F2NH".into()));
        assert_eq!(classify("XP89DCGQ3K6VLD"), LinkKind::Store("XP89DCGQ3K6VLD".into()));
        assert_eq!(classify("https://example.com/tool.ps1"), LinkKind::Script);
        assert_eq!(classify("https://example.com/run.BAT?x=1"), LinkKind::Script);
        assert!(matches!(classify("https://download.cdn.viber.com/desktop/windows/ViberSetup.msi"), LinkKind::Url(_)));
        assert!(matches!(classify("http://example.com/a.exe"), LinkKind::Url(_)));
        assert_eq!(classify("M2Team.NanaZip"), LinkKind::WingetId("M2Team.NanaZip".into()));
        assert_eq!(classify("7zip.7zip"), LinkKind::WingetId("7zip.7zip".into()));
        assert_eq!(classify("visual studio code"), LinkKind::Text("visual studio code".into()));
        assert_eq!(classify("obs"), LinkKind::Text("obs".into()));
        assert_eq!(classify("   "), LinkKind::Invalid);
        assert_eq!(classify("ftp://x.org/a.exe"), LinkKind::Invalid);
    }

    #[test]
    fn github_asset_selection() {
        let nana = [
            "NanaZipPreview_7.0.1843.0.msixbundle", "NanaZipPreview_7.0.1843.0.xml", "NanaZipPreview_7.0.1843.0_Binaries.zip",
            "NanaZip_7.0.1843.0.msixbundle", "NanaZip_7.0.1843.0.xml", "NanaZip_7.0.1843.0_Binaries.zip", "NanaZip_7.0.1843.0_DebugSymbols.zip",
        ]
        .map(asset);
        assert_eq!(pick_windows_asset(&nana).unwrap().name, "NanaZip_7.0.1843.0.msixbundle");

        let oo = [
            "DesktopEditors-x86_64.AppImage", "DesktopEditors_arm64.exe", "DesktopEditors_arm64.msi", "DesktopEditors_x64.exe",
            "DesktopEditors_x64.msi", "DesktopEditors_x64.zip", "DesktopEditors_x86.exe", "DesktopEditors_x86.msi", "ONLYOFFICE-x86_64.dmg",
        ]
        .map(asset);
        assert_eq!(pick_windows_asset(&oo).unwrap().name, "DesktopEditors_x64.msi");

        let cmp = ["CompressO_3.0.0_aarch64.dmg", "CompressO_3.0.0_amd64.deb", "CompressO_3.0.0_x64.exe", "CompressO_3.0.0_x64.exe.sig", "latest.json"].map(asset);
        assert_eq!(pick_windows_asset(&cmp).unwrap().name, "CompressO_3.0.0_x64.exe");

        let pdf = ["pdfcraft-0.4.0-windows-arm64.msi", "pdfcraft-0.4.0-windows-x64-portable.zip", "pdfcraft-0.4.0-windows-x64.msi", "pdfcraft-0.4.0-windows-x86.msi", "SHA256SUMS.txt"].map(asset);
        assert_eq!(pick_windows_asset(&pdf).unwrap().name, "pdfcraft-0.4.0-windows-x64.msi");

        let ap = ["AutoPara-1.10.0-Setup.exe", "AutoPara-1.10.0.apk", "AutoPara-1.10.0.dmg"].map(asset);
        assert_eq!(pick_windows_asset(&ap).unwrap().name, "AutoPara-1.10.0-Setup.exe");

        assert!(pick_windows_asset(&["tool-linux.tar.gz", "tool.dmg"].map(asset)).is_none());
    }

    #[test]
    fn asset_regex_survives_new_versions() {
        let re = regex::Regex::new(&asset_regex("CompressO_3.0.0_x64.exe", "3.0.0")).unwrap();
        assert!(re.is_match("CompressO_3.1.12_x64.exe"));
        assert!(!re.is_match("CompressO_3.1.12_aarch64.exe"));
        let re = regex::Regex::new(&asset_regex("AutoPara-1.10.0-Setup.exe", "v1.10.0")).unwrap();
        assert!(re.is_match("AutoPara-2.0.0-Setup.exe"));
        let re = regex::Regex::new(&asset_regex("app_1_2_3_setup.exe", "v1.2.3")).unwrap();
        assert!(re.is_match("app_1_3_0_setup.exe"));
        assert_eq!(asset_regex("DesktopEditors_x64.msi", "v9.4.0"), r"^DesktopEditors_x64\.msi$");
    }

    #[test]
    fn winget_parsing() {
        let show = "\r-\r\\\r \rFound NanaZip [M2Team.NanaZip]\nVersion: 7.0.1843.0\nPublisher: Kenji Mouri\nDescription: x\n";
        let (n, id, p, v) = parse_winget_show(show).unwrap();
        assert_eq!((n.as_str(), id.as_str(), p.as_deref(), v.as_deref()), ("NanaZip", "M2Team.NanaZip", Some("Kenji Mouri"), Some("7.0.1843.0")));
        let ru = "Найдено Telegram Desktop [Telegram.TelegramDesktop]\nВерсия: 7.2.9\nИздатель: Telegram FZ-LLC\n";
        assert_eq!(parse_winget_show(ru).unwrap().2.as_deref(), Some("Telegram FZ-LLC"));
        assert!(parse_winget_show("No package found matching input criteria.").is_none());

        let table = "\r   - \r\nName               Id                         Version   Source\n-------------------------------------------------------------\nOBS Studio         OBSProject.OBSStudio       31.1.2    winget\nOBS Studio Beta    OBSProject.OBSStudio.Beta  32.0.0    winget\nStreamlabs OBS     Streamlabs.StreamlabsOBS…  1.0       winget\n";
        let m = parse_winget_table(table);
        assert_eq!(m.len(), 2, "{m:?}");
        assert_eq!(m[0], WingetMatch { name: "OBS Studio".into(), id: "OBSProject.OBSStudio".into(), version: "31.1.2".into() });
        let ru = "Имя      ИД                Версия  Источник\n--------------------------------------------\nПрилож   Vendor.App        1.0     winget\n";
        assert_eq!(parse_winget_table(ru)[0].id, "Vendor.App");
        assert!(parse_winget_table("No package found matching input criteria.").is_empty());
    }

    #[test]
    fn page_link_extraction() {
        let html = r#"<a href="/dl/App-2.1-setup.exe">Win</a> <a href='https://cdn.x.com/App.dmg'>Mac</a>
            <a href="https://cdn.x.com/App-2.1-win64.zip">zip</a> <a href="/dl/App-2.1-setup.exe">dup</a>
            <a href="http://insecure.x.com/a.msi">http</a> <a href="/run.ps1">script</a> <a href="/about">about</a>"#;
        let l = page_links("https://x.com/download/", html, false);
        assert_eq!(l, vec!["https://x.com/dl/App-2.1-setup.exe", "https://cdn.x.com/App-2.1-win64.zip"]);
        assert_eq!(page_links("https://x.com/", html, true).len(), 3);
    }

    #[test]
    fn names_and_domains() {
        assert_eq!(pretty_name("AVAModManager-v0.9.4-setup.exe"), "AVAModManager");
        assert_eq!(pretty_name("ViberSetup.msi"), "Viber");
        assert_eq!(pretty_name("CompressO_3.0.0_x64.exe"), "CompressO");
        assert_eq!(pretty_name("obs-studio-31.1.2-windows-x64.zip"), "Obs studio");
        assert_eq!(pretty_name("setup.exe"), "Setup");
        assert_eq!(base_domain("download.cdn.viber.com"), "viber.com");
        assert_eq!(base_domain("github.com"), "github.com");
        assert_eq!(msi_product("MsiExec.exe /I{19465C24-3D5D-4327-B99F-3CC0A1D38151}").unwrap(), "{19465C24-3D5D-4327-B99F-3CC0A1D38151}");
        assert!(msi_product(r#""C:\Program Files\X\unins000.exe""#).is_none());
        assert_eq!(new_id("Visual Studio Code!", &[]), "my-visual-studio-code");
        assert_eq!(new_id("Viber", &["my-viber".into()]), "my-viber-2");
    }

    fn sample() -> Vec<MyApp> {
        let mut a = blank("Viber", MySource::Url { url: "https://download.cdn.viber.com/desktop/windows/ViberSetup.msi".into(), hosts: vec!["viber.com".into()] });
        a.id = "my-viber".into();
        a.silent_args = vec!["/qn".into(), "/norestart".into()];
        a.reviewed = true;
        a.allow_unsigned = true;
        let mut b = blank("NanaZip", MySource::Winget { id: "M2Team.NanaZip".into(), store: "winget".into() });
        b.id = "my-nanazip".into();
        let mut c = blank("CompressO", MySource::Github { repo: "codeforreal1/compressO".into(), asset_regex: r"^CompressO_[0-9][0-9._]*_x64\.exe$".into() });
        c.id = "my-compresso".into();
        c.mode = Mode::Extract;
        c.shortcut = Some(r"bin\app.exe".into());
        vec![a, b, c]
    }

    #[test]
    fn schema_validation() {
        let ok = serde_json::to_string(&Store { version: 1, apps: sample() }).unwrap();
        assert_eq!(parse_store(&ok, false).unwrap().apps.len(), 3);

        let bad = |f: &dyn Fn(&mut Vec<MyApp>)| {
            let mut v = sample();
            f(&mut v);
            parse_store(&serde_json::to_string(&Store { version: 1, apps: v }).unwrap(), false).is_err()
        };
        assert!(bad(&|v| v[0].source = MySource::Url { url: "http://x.com/a.exe".into(), hosts: vec![] }));
        assert!(bad(&|v| v[0].source = MySource::Url { url: "https://x.com/a.ps1".into(), hosts: vec![] }));
        assert!(bad(&|v| v[1].id = "my-viber".into()));
        assert!(bad(&|v| v[0].id = "Bad Id".into()));
        assert!(bad(&|v| v[0].name = " ".into()));
        assert!(bad(&|v| v[0].silent_args = vec!["/S & calc".into()]));
        assert!(bad(&|v| v[2].shortcut = Some(r"..\..\evil.exe".into())));
        assert!(bad(&|v| v[1].source = MySource::Winget { id: "a".into(), store: "evilstore".into() }));
        assert!(parse_store(r#"{"version":2,"apps":[]}"#, false).is_err());
        assert!(parse_store("not json", false).is_err());
        // http allowed only when the setting is on
        let mut v = sample();
        v[0].source = MySource::Url { url: "http://x.com/a.exe".into(), hosts: vec![] };
        assert!(parse_store(&serde_json::to_string(&Store { version: 1, apps: v }).unwrap(), true).is_ok());
    }

    #[test]
    fn export_import_round_trip() {
        let apps = sample();
        let exported = serde_json::to_string_pretty(&Store { version: SCHEMA_VERSION, apps: apps.clone() }).unwrap();
        let back = parse_store(&exported, false).unwrap().apps;
        assert_eq!(back, apps);

        // Importing onto a fresh PC: everything kept, but trust is reset.
        let imported = sanitize_import(back.clone(), &[]);
        assert_eq!(imported.len(), 3);
        assert!(!imported[0].reviewed && !imported[0].allow_unsigned);
        assert_eq!(imported[0].silent_args, apps[0].silent_args);
        // Importing onto a PC that already has Viber: duplicate source skipped.
        let imported = sanitize_import(back, &apps[..1]);
        assert_eq!(imported.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), vec!["my-nanazip", "my-compresso"]);
    }

    #[test]
    fn to_item_mapping() {
        let s = sample();
        let viber = s[0].to_item(false);
        assert!(matches!(viber.source, Source::Direct { .. }));
        assert!(viber.allowed_hosts.contains(&"viber.com".to_string()));
        assert!(viber.custom && viber.reviewed && viber.allow_unsigned);
        let nz = s[1].to_item(false);
        assert_eq!(nz.winget_id.as_deref(), Some("M2Team.NanaZip"));
        assert!(nz.reviewed, "winget entries need no file review");
        let c = s[2].to_item(false);
        assert_eq!(c.zip.unwrap().extract_to.unwrap(), r"%LOCALAPPDATA%\Programs\CompressO");
        assert_eq!(list_url("https://drive.google.com/file/d/FILEID/view"), drive::file_url("FILEID"));
    }
}
