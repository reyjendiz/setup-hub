use crate::catalog::{self, Item, Mode, Source};
use crate::installer::{self, Kind, Outcome};
use crate::myapps::{self, MyApp};
use crate::{defaults, detect, download, settings, sig, util};
use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tauri::{AppHandle, Emitter};
use tokio::sync::Semaphore;

#[derive(Debug, Clone, Serialize)]
pub struct JobEvent {
    pub id: String,
    /// queued | resolving | downloading | verifying | installing | review | done | failed | cancelled
    pub phase: &'static str,
    pub progress: Option<f64>,
    pub message: Option<String>,
    pub reboot: bool,
    pub version: Option<String>,
}

/// A My Apps entry was downloaded for the first time and waits for the user to confirm signature/arguments.
#[derive(Debug)]
pub struct NeedsReview;
impl std::fmt::Display for NeedsReview {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("waiting for review")
    }
}
impl std::error::Error for NeedsReview {}

pub struct Engine {
    pub catalog: RwLock<catalog::Catalog>,
    pub catalog_origin: RwLock<&'static str>,
    pub my_apps: RwLock<Vec<MyApp>>,
    jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    install_lock: tokio::sync::Mutex<()>,
    downloads: RwLock<Arc<Semaphore>>,
    pub reboot: Mutex<BTreeSet<String>>,
}

impl Engine {
    pub fn new(c: catalog::Catalog, origin: &'static str, parallel: u8) -> Self {
        Self {
            catalog: RwLock::new(c),
            catalog_origin: RwLock::new(origin),
            my_apps: RwLock::new(myapps::load()),
            jobs: Mutex::new(HashMap::new()),
            install_lock: tokio::sync::Mutex::new(()),
            downloads: RwLock::new(Arc::new(Semaphore::new(parallel.clamp(1, 4) as usize))),
            reboot: Mutex::new(BTreeSet::new()),
        }
    }

    pub fn set_parallel(&self, n: u8) {
        *self.downloads.write().unwrap() = Arc::new(Semaphore::new(n.clamp(1, 4) as usize));
    }

    /// Built-in catalog first, then My Apps — both go through the same pipeline.
    pub fn item(&self, id: &str) -> Option<Item> {
        if let Some(i) = self.catalog.read().unwrap().items.iter().find(|i| i.id == id) {
            return Some(i.clone());
        }
        let allow_http = settings::load().allow_http;
        self.my_apps.read().unwrap().iter().find(|m| m.id == id).map(|m| m.to_item(allow_http))
    }

    pub fn all_items(&self) -> Vec<Item> {
        let allow_http = settings::load().allow_http;
        let mut v = self.catalog.read().unwrap().items.clone();
        v.extend(self.my_apps.read().unwrap().iter().map(|m| m.to_item(allow_http)));
        v
    }

    /// Mutates My Apps, persists the list and tells the UI.
    pub fn update_my_apps<R>(&self, app: &AppHandle, f: impl FnOnce(&mut Vec<MyApp>) -> Result<R>) -> Result<R> {
        let mut list = self.my_apps.write().unwrap();
        let mut next = list.clone();
        let r = f(&mut next)?;
        myapps::save(&next)?;
        *list = next;
        let _ = app.emit("myapps", list.clone());
        Ok(r)
    }

    /// Registers a cancel flag; returns None if the job is already running.
    pub fn begin(&self, id: &str) -> Option<Arc<AtomicBool>> {
        let mut j = self.jobs.lock().unwrap();
        if j.contains_key(id) {
            return None;
        }
        let f = Arc::new(AtomicBool::new(false));
        j.insert(id.to_string(), f.clone());
        Some(f)
    }

    pub fn end(&self, id: &str) {
        self.jobs.lock().unwrap().remove(id);
    }

    pub fn cancel(&self, id: &str) {
        if let Some(f) = self.jobs.lock().unwrap().get(id) {
            f.store(true, Ordering::Relaxed);
        }
    }

    /// Serializes installers: running two at once makes MSI/Inno setups fail with "another install in progress".
    pub async fn install_slot(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.install_lock.lock().await
    }

    pub async fn download_slot(&self) -> tokio::sync::OwnedSemaphorePermit {
        let s = self.downloads.read().unwrap().clone();
        s.acquire_owned().await.expect("semaphore closed")
    }
}

pub fn emit(app: &AppHandle, ev: JobEvent) {
    let _ = app.emit("job", ev);
}

pub fn ev(id: &str, phase: &'static str) -> JobEvent {
    JobEvent { id: id.into(), phase, progress: None, message: None, reboot: false, version: None }
}

/// Runs one job with cancel tracking and the done/failed/cancelled event at the end.
/// `body` returns (outcome, detected version, message).
pub async fn job<F, Fut>(app: &AppHandle, eng: &Engine, id: &str, label: &str, body: F)
where
    F: FnOnce(Arc<AtomicBool>) -> Fut,
    Fut: std::future::Future<Output = Result<(Outcome, Option<String>, Option<String>)>>,
{
    let Some(cancel) = eng.begin(id) else { return };
    emit(app, ev(id, "queued"));
    let r = body(cancel).await;
    eng.end(id);
    match r {
        Ok((outcome, version, message)) => {
            let reboot = outcome == Outcome::Reboot;
            if reboot {
                eng.reboot.lock().unwrap().insert(label.to_string());
            }
            tracing::info!("{id}: done (reboot={reboot}, version={version:?})");
            emit(app, JobEvent { reboot, version, message, ..ev(id, "done") });
        }
        Err(e) if e.is::<download::Cancelled>() => {
            tracing::info!("{id}: cancelled");
            emit(app, ev(id, "cancelled"));
        }
        Err(e) if e.is::<NeedsReview>() => {
            tracing::info!("{id}: downloaded, waiting for review");
            emit(app, ev(id, "review"));
        }
        Err(e) => {
            tracing::error!("{id}: {e:#}");
            emit(app, JobEvent { message: Some(format!("{e:#}")), ..ev(id, "failed") });
        }
    }
}

/// Full pipeline for one catalog or My Apps item.
pub async fn install(app: AppHandle, eng: Arc<Engine>, id: String) {
    let Some(item) = eng.item(&id) else { return };
    let (a2, e2, it) = (app.clone(), eng.clone(), item.clone());
    job(&app, &eng, &id, &item.name, move |cancel| async move {
        let (mut o, mut msg) = run_item(&a2, &e2, &it, &cancel).await?;
        if it.needs_reboot {
            o = Outcome::Reboot;
        }
        // Apps that replace a Windows function (VLC, Chrome, PdfCraft) become the default for it.
        if !it.custom && it.defaults.is_some() {
            msg = defaults::after_install(&it).or(msg);
        }
        let version = detect::detect_one(&e2.item(&it.id).unwrap_or(it), &detect::uninstall_entries(), &detect::appx_packages());
        Ok((o, version, msg))
    })
    .await
}

/// Download slot → download with progress, reusing a finished download of the same URL
/// (so "review, then install" and "retry after a failed install" don't download twice).
pub async fn fetch_cached(
    app: &AppHandle,
    eng: &Engine,
    id: &str,
    url: &str,
    hosts: &[String],
    allow_http: bool,
    cancel: &AtomicBool,
) -> Result<PathBuf> {
    let host = reqwest::Url::parse(url)?.host_str().unwrap_or("").to_string();
    let scheme_ok = url.starts_with("https://") || (allow_http && url.starts_with("http://"));
    if !scheme_ok || !util::host_allowed(&host, hosts) {
        bail!("URL {url} is not on the allow-list");
    }
    let dir = util::cache_dir().join(id.replace(':', "_"));
    let marker = dir.join("ready.txt");
    if let Ok(m) = std::fs::read_to_string(&marker) {
        if let Some((u, f)) = m.split_once('\n') {
            let p = dir.join(f);
            if u == url && p.is_file() {
                emit(app, JobEvent { progress: Some(100.0), ..ev(id, "downloading") });
                return Ok(p);
            }
        }
    }
    let _ = std::fs::remove_file(&marker);
    let _slot = eng.download_slot().await;
    emit(app, JobEvent { progress: Some(0.0), ..ev(id, "downloading") });
    let file = download::fetch(&util::http_opts(hosts.to_vec(), allow_http), url, &dir, cancel, &|done, total| {
        let p = total.filter(|t| *t > 0).map(|t| done as f64 / t as f64 * 100.0);
        emit(app, JobEvent { progress: p, ..ev(id, "downloading") });
    })
    .await?;
    std::fs::write(&marker, format!("{url}\n{}", file.file_name().unwrap_or_default().to_string_lossy()))?;
    Ok(file)
}

/// Download + hash/signature verification (archives are checked after extraction instead).
pub async fn fetch_verified(
    app: &AppHandle,
    eng: &Engine,
    id: &str,
    url: &str,
    hosts: &[String],
    sha256: Option<&str>,
    publisher: Option<&str>,
    cancel: &AtomicBool,
) -> Result<PathBuf> {
    let file = fetch_cached(app, eng, id, url, hosts, false, cancel).await?;
    if !is_archive(&file) {
        emit(app, ev(id, "verifying"));
        let v = sig::verify(&file, sha256, publisher).await?;
        tracing::info!("{id}: {v}");
    }
    Ok(file)
}

fn is_archive(f: &Path) -> bool {
    matches!(installer::ext_of(&f.to_string_lossy()).as_str(), "zip" | "7z")
}

pub fn cleanup(file: &Path) {
    if !settings::load().keep_installers {
        if let Some(d) = file.parent() {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}

async fn run_item(app: &AppHandle, eng: &Engine, item: &Item, cancel: &AtomicBool) -> Result<(Outcome, Option<String>)> {
    let id = item.id.as_str();
    if let Some(wid) = &item.winget_id {
        if installer::ensure_winget().await {
            match via_winget(app, eng, item, wid, cancel).await {
                Ok(o) => return Ok((o, None)),
                Err(e) if e.is::<download::Cancelled>() => return Err(e),
                Err(e) if matches!(item.source, Source::Winget) => return Err(e),
                Err(e) => tracing::warn!("{id}: winget failed ({e:#}), falling back to vendor installer"),
            }
        } else if matches!(item.source, Source::Winget) {
            bail!("winget is not available and {} has no vendor fallback", item.name);
        }
    }
    via_download(app, eng, item, cancel).await
}

/// For My Apps entries without detection info: remember which uninstall key / MSIX package the first install created.
fn record_detection(app: &AppHandle, eng: &Engine, item: &Item, before: Option<detect::Snapshot>) {
    let Some(before) = before else { return };
    if let Some(d) = detect::new_since(&before, &detect::uninstall_entries(), &detect::appx_packages()) {
        tracing::info!("{}: recorded detection {d:?}", item.id);
        let _ = eng.update_my_apps(app, |list| {
            if let Some(m) = list.iter_mut().find(|m| m.id == item.id) {
                m.detect = d;
            }
            Ok(())
        });
    }
}

fn needs_snapshot(item: &Item) -> bool {
    item.custom && item.detect == Default::default() && item.mode != Mode::DownloadOnly
}

async fn via_winget(app: &AppHandle, eng: &Engine, item: &Item, wid: &str, cancel: &AtomicBool) -> Result<Outcome> {
    let id = item.id.clone();
    let source = item.winget_source.as_deref().unwrap_or("winget");
    let _slot = eng.install_slot().await;
    let before = needs_snapshot(item).then(detect::snapshot);
    if item.unelevated {
        emit(app, JobEvent { message: Some("winget (non-admin)".into()), ..ev(&id, "installing") });
        let line = format!("winget.exe {}", installer::winget_args(wid, source).join(" "));
        return installer::exit_outcome(installer::run_unelevated(&line, cancel).await?);
    }
    emit(app, JobEvent { progress: Some(0.0), message: Some("winget".into()), ..ev(&id, "downloading") });
    let installing = AtomicBool::new(false);
    let (o, _) = installer::winget_install(wid, source, cancel, &|pct, line| {
        if installing.load(Ordering::Relaxed) {
            return;
        }
        if line.contains("Starting package install") || line.contains("Successfully verified") || pct == Some(100.0) {
            installing.store(true, Ordering::Relaxed);
            emit(app, ev(&id, "installing"));
        } else if let Some(p) = pct {
            emit(app, JobEvent { progress: Some(p), ..ev(&id, "downloading") });
        }
    })
    .await?;
    record_detection(app, eng, item, before);
    Ok(o)
}

async fn via_download(app: &AppHandle, eng: &Engine, item: &Item, cancel: &AtomicBool) -> Result<(Outcome, Option<String>)> {
    let id = item.id.clone();
    emit(app, ev(&id, "resolving"));
    let token = settings::get_secret("github_token");
    let res = catalog::resolve(item, token.as_deref()).await?;
    tracing::info!("{id}: resolved {} (version {:?})", res.url, res.version);
    let file = fetch_cached(app, eng, &id, &res.url, &item.allowed_hosts, item.allow_http, cancel).await?;
    if installer::is_script(&file.to_string_lossy()) {
        bail!("the link delivered a script ({}) — refusing it", file.display());
    }

    if item.mode == Mode::DownloadOnly {
        let dest = PathBuf::from(settings::load().drive_dest);
        std::fs::create_dir_all(&dest)?;
        let target = dest.join(file.file_name().unwrap_or_default());
        std::fs::copy(&file, &target)?;
        cleanup(&file);
        return Ok((Outcome::Ok, Some(target.display().to_string())));
    }

    if !is_archive(&file) || item.custom {
        emit(app, ev(&id, "verifying"));
        if let Some(want) = res.sha256.as_deref() {
            let got = sig::sha256_file(&file)?;
            if !got.eq_ignore_ascii_case(want) {
                let _ = std::fs::remove_dir_all(file.parent().unwrap());
                bail!("SHA-256 mismatch: expected {want}, got {got}");
            }
        }
        if item.custom && !item.reviewed {
            review(app, eng, item, &file).await?;
            return Err(NeedsReview.into());
        }
        if !is_archive(&file) {
            let v = sig::verify_with(&file, res.sha256.as_deref(), item.publisher.as_deref(), item.allow_unsigned).await?;
            tracing::info!("{id}: {v}");
        }
    }

    let _slot = eng.install_slot().await;
    emit(app, JobEvent { message: item.interactive.then(|| "needs-interaction".to_string()), ..ev(&id, "installing") });
    let before = needs_snapshot(item).then(detect::snapshot);
    let out = if item.custom && item.mode == Mode::Extract {
        extract_custom(item, &file).await
    } else if is_archive(&file) {
        install_zip(item, &file, file.parent().unwrap(), cancel).await
    } else {
        installer::run_installer(&file, &item.silent_args, item.unelevated, cancel).await
    };
    if out.is_ok() {
        record_detection(app, eng, item, before);
        cleanup(&file);
    }
    out.map(|o| (o, None))
}

/// First download of a My Apps entry: record type, signature, icon and suggested switches for the review sheet.
async fn review(app: &AppHandle, eng: &Engine, item: &Item, file: &Path) -> Result<()> {
    let i = myapps::inspect(file).await?;
    tracing::info!("{}: inspected {:?}, signature {} {:?}", item.id, i.kind, i.status, i.signer);
    eng.update_my_apps(app, |list| {
        let m = list.iter_mut().find(|m| m.id == item.id).ok_or_else(|| anyhow!("entry removed"))?;
        m.installer_type = Some(i.kind);
        m.signature = Some(i.status.clone());
        m.signer = if i.status == "Valid" { i.signer.clone() } else { None };
        m.size = Some(i.size);
        if m.icon.as_deref().map_or(true, |s| s.starts_with("https://github.com/")) {
            m.icon = i.icon.clone().or(m.icon.take());
        }
        if m.mode == Mode::Silent && m.silent_args.is_empty() {
            m.silent_args = installer::suggested_args(i.kind);
            // Unknown technology: safest honest default is to let the user click through it.
            if i.kind == Kind::Unknown {
                m.mode = Mode::Interactive;
            }
        }
        Ok(())
    })
}

async fn extract_custom(item: &Item, archive: &Path) -> Result<Outcome> {
    let spec = item.zip.clone().unwrap_or_default();
    let dest = PathBuf::from(util::expand_env(&spec.extract_to.unwrap_or_else(|| myapps::default_extract_dir(&item.name))));
    installer::extract(archive, &dest).await?;
    if let Some(exe) = spec.shortcut.filter(|s| !s.is_empty()) {
        let target = dest.join(&exe);
        if !target.starts_with(&dest) || !target.is_file() {
            bail!("shortcut target {exe} not found in {}", dest.display());
        }
        let lnk = util::start_menu_dir().join(format!("{}.lnk", download::sanitize(&item.name)));
        util::create_shortcut(&lnk, &target, "").await?;
    }
    tracing::info!("{}: extracted to {}", item.id, dest.display());
    Ok(Outcome::Ok)
}

async fn install_zip(item: &Item, zip: &Path, dir: &Path, cancel: &AtomicBool) -> Result<Outcome> {
    let spec = item.zip.clone().ok_or_else(|| anyhow!("{}: zip without zip spec", item.id))?;
    if let Some(to) = &spec.extract_to {
        let dest = PathBuf::from(util::expand_env(to));
        installer::unzip(zip, &dest)?;
        if let Some(exe) = &spec.shortcut {
            let target = dest.join(exe);
            let v = sig::verify(&target, None, item.publisher.as_deref()).await?;
            tracing::info!("{}: {v}", item.id);
            let lnk = PathBuf::from(util::expand_env(r"%ProgramData%\Microsoft\Windows\Start Menu\Programs")).join(format!("{}.lnk", item.name));
            util::create_shortcut(&lnk, &target, "").await?;
            if spec.launch {
                let _ = std::process::Command::new(&target).current_dir(&dest).spawn();
            }
        }
        return Ok(Outcome::Ok);
    }
    let re = regex::Regex::new(spec.run.as_deref().unwrap_or(r"(?i)setup.*\.exe$"))?;
    let files = installer::unzip(zip, &dir.join("x"))?;
    let exe = files
        .iter()
        .find(|f| re.is_match(&f.file_name().unwrap_or_default().to_string_lossy()))
        .ok_or_else(|| anyhow!("no installer matching {re} inside the zip"))?;
    let v = sig::verify(exe, None, item.publisher.as_deref()).await?;
    tracing::info!("{}: {v}", item.id);
    installer::run_installer(exe, &item.silent_args, item.unelevated, cancel).await
}
