use crate::catalog::{self, Item, Source};
use crate::installer::{self, Outcome};
use crate::{detect, download, settings, sig, util};
use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tauri::{AppHandle, Emitter};
use tokio::sync::Semaphore;

#[derive(Debug, Clone, Serialize)]
pub struct JobEvent {
    pub id: String,
    /// queued | resolving | downloading | verifying | installing | done | failed | cancelled
    pub phase: &'static str,
    pub progress: Option<f64>,
    pub message: Option<String>,
    pub reboot: bool,
    pub version: Option<String>,
}

pub struct Engine {
    pub catalog: RwLock<catalog::Catalog>,
    pub catalog_origin: RwLock<&'static str>,
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
            jobs: Mutex::new(HashMap::new()),
            install_lock: tokio::sync::Mutex::new(()),
            downloads: RwLock::new(Arc::new(Semaphore::new(parallel.clamp(1, 4) as usize))),
            reboot: Mutex::new(BTreeSet::new()),
        }
    }

    pub fn set_parallel(&self, n: u8) {
        *self.downloads.write().unwrap() = Arc::new(Semaphore::new(n.clamp(1, 4) as usize));
    }

    pub fn item(&self, id: &str) -> Option<Item> {
        self.catalog.read().unwrap().items.iter().find(|i| i.id == id).cloned()
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
        Err(e) => {
            tracing::error!("{id}: {e:#}");
            emit(app, JobEvent { message: Some(format!("{e:#}")), ..ev(id, "failed") });
        }
    }
}

/// Full pipeline for one catalog item.
pub async fn install(app: AppHandle, eng: Arc<Engine>, id: String) {
    let Some(item) = eng.item(&id) else { return };
    let (a2, e2, it) = (app.clone(), eng.clone(), item.clone());
    job(&app, &eng, &id, &item.name, move |cancel| async move {
        let mut o = run_item(&a2, &e2, &it, &cancel).await?;
        if it.needs_reboot {
            o = Outcome::Reboot;
        }
        let version = detect::detect_one(&it, &detect::uninstall_entries(), &detect::appx_packages());
        Ok((o, version, None))
    })
    .await
}

/// Download slot → download with progress → hash/signature verification. Returns the verified file.
pub async fn fetch_verified(
    app: &AppHandle,
    eng: &Engine,
    id: &str,
    url: &str,
    hosts: &[String],
    sha256: Option<&str>,
    publisher: Option<&str>,
    cancel: &AtomicBool,
) -> Result<std::path::PathBuf> {
    let host = reqwest::Url::parse(url)?.host_str().unwrap_or("").to_string();
    if !url.starts_with("https://") || !util::host_allowed(&host, hosts) {
        bail!("URL {url} is not on the allow-list");
    }
    let dir = util::cache_dir().join(id.replace(':', "_"));
    let file = {
        let _slot = eng.download_slot().await;
        emit(app, JobEvent { progress: Some(0.0), ..ev(id, "downloading") });
        download::fetch(&util::http(hosts.to_vec()), url, &dir, cancel, &|done, total| {
            let p = total.filter(|t| *t > 0).map(|t| done as f64 / t as f64 * 100.0);
            emit(app, JobEvent { progress: p, ..ev(id, "downloading") });
        })
        .await?
    };
    if !file.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
        emit(app, ev(id, "verifying"));
        let v = sig::verify(&file, sha256, publisher).await?;
        tracing::info!("{id}: {v}");
    }
    Ok(file)
}

pub fn cleanup(file: &Path) {
    if !settings::load().keep_installers {
        if let Some(d) = file.parent() {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}

async fn run_item(app: &AppHandle, eng: &Engine, item: &Item, cancel: &AtomicBool) -> Result<Outcome> {
    let id = item.id.as_str();
    if let Some(wid) = &item.winget_id {
        if installer::ensure_winget().await {
            match via_winget(app, eng, item, wid, cancel).await {
                Ok(o) => return Ok(o),
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

async fn via_winget(app: &AppHandle, eng: &Engine, item: &Item, wid: &str, cancel: &AtomicBool) -> Result<Outcome> {
    let id = item.id.clone();
    let _slot = eng.install_slot().await;
    if item.unelevated {
        emit(app, JobEvent { message: Some("winget (non-admin)".into()), ..ev(&id, "installing") });
        let line = format!("winget.exe {}", installer::winget_args(wid).join(" "));
        return installer::exit_outcome(installer::run_unelevated(&line, cancel).await?);
    }
    emit(app, JobEvent { progress: Some(0.0), message: Some("winget".into()), ..ev(&id, "downloading") });
    let installing = AtomicBool::new(false);
    let (o, _) = installer::winget_install(wid, cancel, &|pct, line| {
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
    Ok(o)
}

async fn via_download(app: &AppHandle, eng: &Engine, item: &Item, cancel: &AtomicBool) -> Result<Outcome> {
    let id = item.id.clone();
    emit(app, ev(&id, "resolving"));
    let token = settings::get_secret("github_token");
    let res = catalog::resolve(item, token.as_deref()).await?;
    tracing::info!("{id}: resolved {} (version {:?})", res.url, res.version);
    let file = fetch_verified(app, eng, &id, &res.url, &item.allowed_hosts, res.sha256.as_deref(), item.publisher.as_deref(), cancel).await?;

    let _slot = eng.install_slot().await;
    emit(app, JobEvent { message: item.interactive.then(|| "needs-interaction".to_string()), ..ev(&id, "installing") });
    let out = if file.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
        install_zip(item, &file, file.parent().unwrap(), cancel).await
    } else {
        installer::run_installer(&file, &item.silent_args, item.unelevated, cancel).await
    };
    if out.is_ok() {
        cleanup(&file);
    }
    out
}

async fn install_zip(item: &Item, zip: &Path, dir: &Path, cancel: &AtomicBool) -> Result<Outcome> {
    let spec = item.zip.clone().ok_or_else(|| anyhow!("{}: zip without zip spec", item.id))?;
    if let Some(to) = &spec.extract_to {
        let dest = std::path::PathBuf::from(util::expand_env(to));
        installer::unzip(zip, &dest)?;
        if let Some(exe) = &spec.shortcut {
            let target = dest.join(exe);
            let v = sig::verify(&target, None, item.publisher.as_deref()).await?;
            tracing::info!("{}: {v}", item.id);
            let lnk = std::path::PathBuf::from(util::expand_env(r"%ProgramData%\Microsoft\Windows\Start Menu\Programs"))
                .join(format!("{}.lnk", item.name));
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
