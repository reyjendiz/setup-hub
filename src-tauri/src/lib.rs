pub mod activation;
pub mod catalog;
pub mod defaults;
pub mod detect;
pub mod download;
pub mod drive;
pub mod engine;
pub mod gpu;
pub mod installer;
pub mod myapps;
pub mod settings;
pub mod sig;
pub mod tweaks;
pub mod update;
pub mod util;
pub mod ven;
#[cfg(test)]
mod live_tests;

use anyhow::Context;
use engine::{emit, ev, Engine, JobEvent};
use installer::Outcome;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Manager, State};

type Eng<'a> = State<'a, Arc<Engine>>;
type CmdResult<T> = Result<T, String>;

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

static LOG_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

fn init_logging() {
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("setuphub")
        .filename_suffix("log")
        .max_log_files(7)
        .build(util::app_dir().join("logs"))
        .expect("log dir");
    let (w, guard) = tracing_appender::non_blocking(appender);
    let _ = LOG_GUARD.set(guard);
    let _ = tracing_subscriber::fmt()
        .with_writer(w)
        .with_ansi(false)
        .with_env_filter(tracing_subscriber::EnvFilter::new("info,reqwest=warn,hyper=warn"))
        .try_init();
}

// ---------- bootstrap / catalog ----------

#[derive(Serialize)]
struct Boot {
    items: Vec<catalog::Item>,
    catalog_origin: &'static str,
    settings: settings::Settings,
    has_github_token: bool,
    has_google_key: bool,
    os_build: u32,
    version: &'static str,
}

#[tauri::command]
async fn bootstrap(eng: Eng<'_>) -> CmdResult<Boot> {
    let os_build = gpu::windows_build().await;
    Ok(Boot {
        items: eng.catalog.read().unwrap().items.clone(),
        catalog_origin: *eng.catalog_origin.read().unwrap(),
        settings: settings::load(),
        has_github_token: settings::get_secret("github_token").is_some(),
        has_google_key: settings::get_secret("google_api_key").is_some(),
        os_build,
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[tauri::command]
async fn detect_installed(eng: Eng<'_>) -> CmdResult<HashMap<String, String>> {
    let items = eng.all_items();
    tokio::task::spawn_blocking(move || detect::detect_all(&items)).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn reload_catalog(eng: Eng<'_>, url: String) -> CmdResult<(Vec<catalog::Item>, &'static str)> {
    let (c, origin) = catalog::load(url.trim()).await;
    let items = c.items.clone();
    *eng.catalog.write().unwrap() = c;
    *eng.catalog_origin.write().unwrap() = origin;
    Ok((items, origin))
}

#[tauri::command]
async fn winget_status() -> bool {
    installer::ensure_winget().await
}

// ---------- installs ----------

#[tauri::command]
fn install(app: AppHandle, eng: Eng<'_>, ids: Vec<String>) {
    for id in ids {
        tauri::async_runtime::spawn(engine::install(app.clone(), eng.inner().clone(), id));
    }
}

#[tauri::command]
fn cancel(eng: Eng<'_>, id: String) {
    eng.cancel(&id);
}

#[tauri::command]
fn reboot_pending(eng: Eng<'_>) -> Vec<String> {
    eng.reboot.lock().unwrap().iter().cloned().collect()
}

#[tauri::command]
async fn restart_now() -> CmdResult<()> {
    util::run("shutdown.exe", &["/r", "/t", "5", "/c", "Setup Hub: restarting to finish installs"]).await.map_err(err)?;
    Ok(())
}

// ---------- tweaks ----------

#[tauri::command]
async fn tweak_states() -> Vec<serde_json::Value> {
    let mut v = Vec::new();
    for id in tweaks::IDS {
        v.push(match tweaks::state(id).await {
            Ok(s) => serde_json::to_value(s).unwrap(),
            Err(e) => serde_json::json!({ "id": id, "error": format!("{e:#}") }),
        });
    }
    v
}

#[tauri::command]
async fn tweak_apply(id: String) -> CmdResult<()> {
    tweaks::apply(&id).await.map_err(err)
}

#[tauri::command]
async fn tweak_revert(id: String) -> CmdResult<()> {
    tweaks::revert(&id).await.map_err(err)
}

// ---------- default apps ----------

#[tauri::command]
async fn defaults_states(eng: Eng<'_>) -> CmdResult<Vec<defaults::DefaultState>> {
    let items = eng.catalog.read().unwrap().items.clone();
    let build = gpu::windows_build().await;
    tokio::task::spawn_blocking(move || defaults::states(&items, build)).await.map_err(|e| e.to_string())
}

fn default_target(eng: &Engine, id: &str) -> CmdResult<catalog::Item> {
    eng.catalog.read().unwrap().items.iter().find(|i| i.id == id && i.defaults.is_some()).cloned().ok_or_else(|| "unknown app".into())
}

#[tauri::command]
fn defaults_apply(eng: Eng<'_>, id: String) -> CmdResult<defaults::Applied> {
    defaults::apply(&default_target(&eng, &id)?).map_err(err)
}

#[tauri::command]
fn defaults_revert(eng: Eng<'_>, id: String) -> CmdResult<()> {
    defaults::revert(&default_target(&eng, &id)?).map_err(err)
}

/// Settings page for a card's install-time message ("Open Settings" when it couldn't be automated).
#[tauri::command]
async fn defaults_settings_uri(eng: Eng<'_>, id: String) -> CmdResult<String> {
    let d = default_target(&eng, &id)?.defaults.ok_or("no defaults")?;
    Ok(defaults::settings_uri(&d, gpu::windows_build().await))
}

// ---------- updates ----------

#[tauri::command]
async fn check_self_update() -> CmdResult<Option<update::SelfUpdate>> {
    update::check_self(settings::get_secret("github_token").as_deref()).await.map_err(err)
}

/// Downloads and swaps in the new Setup Hub, then restarts into it. The release is looked up again
/// here rather than taken from the UI, so only GitHub's own answer decides what gets installed.
#[tauri::command]
fn apply_self_update(app: AppHandle, eng: Eng<'_>) {
    let eng = eng.inner().clone();
    tauri::async_runtime::spawn(async move {
        let a = app.clone();
        engine::job(&app, &eng, "self-update", "Setup Hub", move |cancel| async move {
            emit(&a, ev("self-update", "resolving"));
            let u = update::check_self(settings::get_secret("github_token").as_deref()).await?.context("Setup Hub is already up to date")?;
            emit(&a, JobEvent { progress: Some(0.0), ..ev("self-update", "downloading") });
            let exe = update::apply_self(&u, &cancel, &|p| emit(&a, JobEvent { progress: p, ..ev("self-update", "downloading") })).await?;
            update::restart_into(&exe)?;
            let a2 = a.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                a2.exit(0);
            });
            Ok((Outcome::Ok, Some(u.version.clone()), None))
        })
        .await
    });
}

#[tauri::command]
async fn check_app_updates(eng: Eng<'_>) -> CmdResult<Vec<update::AppUpdate>> {
    let items = eng.catalog.read().unwrap().items.clone();
    Ok(update::check_apps(&items, settings::get_secret("github_token").as_deref()).await)
}

// ---------- GPU ----------

#[derive(Serialize)]
struct GpuInfo {
    gpus: Vec<gpu::Gpu>,
    nvidia_latest: Option<gpu::DriverInfo>,
    nvidia_error: Option<String>,
    update_available: bool,
    /// Windows couldn't tell us the graphics cards (WMI and the registry both failed).
    detect_error: Option<String>,
    /// Install goes through the NVIDIA App, which finds the card and driver itself.
    use_nvidia_app: bool,
}

/// What Windows says about the NVIDIA card: Ok(Some(model)) to look its driver up, Ok(None) when the
/// model is unknown (no driver yet, or detection failed) — the NVIDIA App handles that — Err when
/// Windows reports graphics cards and none of them is NVIDIA.
async fn nvidia_model() -> anyhow::Result<Option<String>> {
    match gpu::detect().await {
        Ok(gpus) => match gpus.iter().find(|g| g.vendor == gpu::Vendor::Nvidia) {
            Some(g) if !g.driver_missing => Ok(Some(g.name.clone())),
            Some(_) => Ok(None),
            None if gpus.is_empty() => Ok(None),
            None => anyhow::bail!("No NVIDIA GPU detected — nothing to install"),
        },
        Err(e) => {
            tracing::warn!("GPU detection failed ({e:#}); the NVIDIA App will detect the card");
            Ok(None)
        }
    }
}

#[tauri::command]
async fn gpu_info() -> CmdResult<GpuInfo> {
    let (gpus, detect_error) = match gpu::detect().await {
        Ok(g) => (g, None),
        Err(e) => (vec![], Some(format!("{e:#}"))),
    };
    let nv = gpus.iter().find(|g| g.vendor == gpu::Vendor::Nvidia).cloned();
    let (mut latest, mut error, mut update) = (None, None, false);
    let mut use_app = detect_error.is_some() || nv.as_ref().is_some_and(|g| g.driver_missing);
    if let Some(g) = nv.filter(|g| !g.driver_missing) {
        match gpu::latest_nvidia(&g.name).await {
            Ok(d) => {
                update = g.display_version.as_deref().is_none_or(|cur| gpu::is_newer(&d.version, cur));
                latest = Some(d);
            }
            Err(e) => {
                error = Some(format!("{e:#}"));
                use_app = true;
            }
        }
    }
    Ok(GpuInfo { gpus, nvidia_latest: latest, nvidia_error: error, update_available: update, detect_error, use_nvidia_app: use_app })
}

#[tauri::command]
fn install_nvidia(app: AppHandle, eng: Eng<'_>, clean: bool) {
    let eng = eng.inner().clone();
    tauri::async_runtime::spawn(async move {
        let (a, e) = (app.clone(), eng.clone());
        engine::job(&app, &eng, "nvidia", "NVIDIA driver", move |cancel| async move {
            let model = nvidia_model().await?;
            emit(&a, ev("nvidia", "resolving"));
            let lookup = match &model {
                Some(m) => gpu::latest_nvidia(m).await,
                None => Err(anyhow::anyhow!("Windows doesn't know the card's model yet")),
            };
            match lookup {
                Ok(d) => {
                    let f = engine::fetch_verified(&a, &e, "nvidia", &d.url, &gpu::nv_hosts(), None, Some("NVIDIA"), &cancel).await?;
                    let _slot = e.install_slot().await;
                    emit(&a, JobEvent { message: Some("screen-flash".into()), ..ev("nvidia", "installing") });
                    let mut args: Vec<String> = ["-s", "-noreboot", "-noeula"].map(String::from).to_vec();
                    if clean {
                        args.push("-clean".into());
                    }
                    match installer::run_installer(&f, &args, false, &cancel).await {
                        Ok(_) => {}
                        // NVIDIA's setup exits with 1 when it wants a reboot.
                        Err(e) if format!("{e}").contains("code 1 ") => {}
                        Err(e) => return Err(e),
                    }
                    engine::cleanup(&f);
                    // A driver swap always warrants a restart.
                    Ok((Outcome::Reboot, Some(d.version), None))
                }
                Err(err) => {
                    tracing::info!("no driver from the system's GPU info ({err:#}); installing the NVIDIA App, which detects the card itself");
                    let url = gpu::nvidia_app_url().await?;
                    let f = engine::fetch_verified(&a, &e, "nvidia", &url, &gpu::nv_hosts(), None, Some("NVIDIA"), &cancel).await?;
                    let _slot = e.install_slot().await;
                    emit(&a, ev("nvidia", "installing"));
                    let o = installer::run_installer(&f, &["-s".into()], false, &cancel).await?;
                    engine::cleanup(&f);
                    Ok((o, None, Some("nvidia-app-fallback".into())))
                }
            }
        })
        .await
    });
}

// ---------- Google Drive ----------

#[tauri::command]
async fn drive_list(folder: String) -> CmdResult<Vec<drive::Entry>> {
    let key = settings::get_secret("google_api_key");
    if !folder.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("invalid Drive folder id".into());
    }
    drive::list(&folder, key.as_deref()).await.map_err(err)
}

/// Validates a pasted folder link (public, really a folder) and returns its id and name.
#[tauri::command]
async fn drive_folder_info(link: String) -> CmdResult<drive::Folder> {
    drive::folder_info(&link, settings::get_secret("google_api_key").as_deref()).await.map_err(err)
}

#[derive(Deserialize)]
struct DriveFile {
    id: String,
    name: String,
    path: String,
}

#[tauri::command]
fn drive_download(app: AppHandle, eng: Eng<'_>, files: Vec<DriveFile>, dest: String) {
    let eng = eng.inner().clone();
    for f in files {
        let (app, eng, dest) = (app.clone(), eng.clone(), dest.clone());
        tauri::async_runtime::spawn(async move {
            let jid = format!("drive:{}", f.id);
            let (a, e, j, label) = (app.clone(), eng.clone(), jid.clone(), f.name.clone());
            engine::job(&app, &eng, &jid, &label, move |cancel| async move {
                let url = drive::resolve_download(&f.id).await?;
                let dir = util::cache_dir().join(j.replace(':', "_"));
                let tmp = {
                    let _slot = e.download_slot().await;
                    download::fetch(&util::http(drive::hosts()), &url, &dir, &cancel, &|done, total| {
                        let p = total.filter(|t| *t > 0).map(|t| done as f64 / t as f64 * 100.0);
                        emit(&a, JobEvent { progress: p, ..ev(&j, "downloading") });
                    })
                    .await?
                };
                // Rebuild the Drive folder tree under dest; every segment is sanitized so names can't escape it.
                let mut out = std::path::PathBuf::from(&dest);
                for seg in f.path.split('/').filter(|s| !s.is_empty()) {
                    out.push(download::sanitize(seg));
                }
                std::fs::create_dir_all(&out)?;
                let target = out.join(download::sanitize(&f.name));
                if std::fs::rename(&tmp, &target).is_err() {
                    std::fs::copy(&tmp, &target)?; // different volume
                }
                let _ = std::fs::remove_dir_all(&dir);
                Ok((Outcome::Ok, None, Some(target.display().to_string())))
            })
            .await
        });
    }
}

// ---------- My Apps ----------

#[tauri::command]
fn myapps_list(eng: Eng<'_>) -> Vec<myapps::MyApp> {
    eng.my_apps.read().unwrap().clone()
}

/// One analysis per non-empty line (multi-line paste adds several entries).
#[tauri::command]
async fn myapps_analyze(input: String) -> Vec<myapps::Analysis> {
    let allow_http = settings::load().allow_http;
    let token = settings::get_secret("github_token");
    let mut lines: Vec<String> = input.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
    lines.dedup();
    lines.truncate(25);
    futures::future::join_all(lines.iter().map(|l| myapps::analyze(l, allow_http, token.as_deref()))).await
}

#[tauri::command]
fn myapps_add(app: AppHandle, eng: Eng<'_>, entries: Vec<myapps::MyApp>) -> CmdResult<Vec<myapps::MyApp>> {
    let allow_http = settings::load().allow_http;
    eng.update_my_apps(&app, |list| {
        for mut e in entries {
            let taken: Vec<String> = list.iter().map(|a| a.id.clone()).collect();
            if e.id.is_empty() || taken.contains(&e.id) {
                e.id = myapps::new_id(&e.name, &taken);
            }
            if e.added_at == 0 {
                e.added_at = myapps::now();
            }
            myapps::validate(&e, allow_http)?;
            tracing::info!("my apps: added {} ({:?})", e.id, e.source);
            list.push(e);
        }
        Ok(list.clone())
    })
    .map_err(err)
}

#[tauri::command]
fn myapps_update(app: AppHandle, eng: Eng<'_>, entry: myapps::MyApp) -> CmdResult<()> {
    let allow_http = settings::load().allow_http;
    myapps::validate(&entry, allow_http).map_err(err)?;
    eng.update_my_apps(&app, |list| {
        let slot = list.iter_mut().find(|a| a.id == entry.id).ok_or_else(|| anyhow::anyhow!("entry not found"))?;
        let mut e = entry;
        if e.source != slot.source {
            // Change link: the new file must be reviewed and signed-off again.
            e.reviewed = matches!(e.source, myapps::MySource::Winget { .. });
            e.allow_unsigned = false;
            e.signature = None;
            e.signer = None;
            e.installer_type = None;
            let _ = std::fs::remove_dir_all(util::cache_dir().join(&e.id));
        }
        *slot = e;
        Ok(())
    })
    .map_err(err)
}

#[tauri::command]
fn myapps_remove(app: AppHandle, eng: Eng<'_>, id: String) -> CmdResult<()> {
    eng.update_my_apps(&app, |list| Ok(list.retain(|a| a.id != id))).map_err(err)
}

#[tauri::command]
fn myapps_duplicate(app: AppHandle, eng: Eng<'_>, id: String) -> CmdResult<()> {
    eng.update_my_apps(&app, |list| {
        let i = list.iter().position(|a| a.id == id).ok_or_else(|| anyhow::anyhow!("entry not found"))?;
        let mut c = list[i].clone();
        c.id = myapps::new_id(&c.name, &list.iter().map(|a| a.id.clone()).collect::<Vec<_>>());
        c.name = format!("{} (copy)", c.name);
        c.detect = Default::default();
        c.added_at = myapps::now();
        list.insert(i + 1, c);
        Ok(())
    })
    .map_err(err)
}

#[tauri::command]
fn myapps_reorder(app: AppHandle, eng: Eng<'_>, ids: Vec<String>) -> CmdResult<()> {
    eng.update_my_apps(&app, |list| {
        list.sort_by_key(|a| ids.iter().position(|i| *i == a.id).unwrap_or(usize::MAX));
        Ok(())
    })
    .map_err(err)
}

/// HEADs every link (and resolves GitHub "latest"); returns id → status.
#[tauri::command]
async fn myapps_check(eng: Eng<'_>) -> CmdResult<HashMap<String, myapps::LinkStatus>> {
    let apps = eng.my_apps.read().unwrap().clone();
    let allow_http = settings::load().allow_http;
    let token = settings::get_secret("github_token");
    let res = futures::future::join_all(apps.iter().map(|a| async { (a.id.clone(), myapps::check(a, allow_http, token.as_deref()).await) })).await;
    Ok(res.into_iter().collect())
}

#[tauri::command]
fn myapps_uninstall(app: AppHandle, eng: Eng<'_>, id: String) {
    let eng = eng.inner().clone();
    let Some(entry) = eng.my_apps.read().unwrap().iter().find(|a| a.id == id).cloned() else { return };
    tauri::async_runtime::spawn(async move {
        let jid = format!("uninstall:{id}");
        let name = entry.name.clone();
        engine::job(&app, &eng, &jid, &name, move |cancel| async move {
            tracing::info!("my apps: uninstalling {}", entry.id);
            Ok((myapps::uninstall(&entry, &cancel).await?, None, None))
        })
        .await
    });
}

#[tauri::command]
fn myapps_export(eng: Eng<'_>, path: String) -> CmdResult<()> {
    let store = myapps::Store { version: myapps::SCHEMA_VERSION, apps: eng.my_apps.read().unwrap().clone() };
    std::fs::write(&path, serde_json::to_string_pretty(&store).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Reads and validates a list for review; nothing is added until the user confirms.
#[tauri::command]
fn myapps_import(eng: Eng<'_>, path: String) -> CmdResult<Vec<myapps::MyApp>> {
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() > 5 << 20 {
        return Err("file is too large to be a My Apps list".into());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let s = myapps::parse_store(&text, settings::load().allow_http).map_err(err)?;
    Ok(myapps::sanitize_import(s.apps, &eng.my_apps.read().unwrap()))
}

#[tauri::command]
async fn myapps_fetch_url(eng: Eng<'_>, url: String) -> CmdResult<Vec<myapps::MyApp>> {
    let allow_http = settings::load().allow_http;
    let url = myapps::list_url(&url);
    let u = reqwest::Url::parse(&url).map_err(|e| e.to_string())?;
    if !(u.scheme() == "https" || (allow_http && u.scheme() == "http")) {
        return Err("the list URL must be https".into());
    }
    let mut hosts = vec![u.host_str().unwrap_or("").to_string(), "googleusercontent.com".into(), "githubusercontent.com".into()];
    hosts.push(myapps::base_domain(u.host_str().unwrap_or("")));
    let text = util::http_opts(hosts, allow_http)
        .get(&url)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let s = myapps::parse_store(&text, allow_http).map_err(err)?;
    let existing = eng.my_apps.read().unwrap().clone();
    Ok(myapps::sanitize_import(s.apps, &existing))
}

/// Last log lines mentioning a job id (shown after a failed Test install).
#[tauri::command]
fn log_tail(id: String) -> String {
    let dir = util::app_dir().join("logs");
    let Some(latest) = std::fs::read_dir(&dir).ok().and_then(|d| d.flatten().max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())) else {
        return String::new();
    };
    let text = std::fs::read_to_string(latest.path()).unwrap_or_default();
    let lines: Vec<&str> = text.lines().filter(|l| l.contains(&id)).collect();
    lines[lines.len().saturating_sub(30)..].join("\n")
}

// ---------- Ven ----------

#[tauri::command]
fn ven_state() -> ven::VenState {
    ven::state()
}

/// Installs Ven (replacing the old AD launcher) and runs it once: Vencord gets installed and Discord starts.
#[tauri::command]
fn ven_install(app: AppHandle, eng: Eng<'_>) {
    let eng = eng.inner().clone();
    tauri::async_runtime::spawn(async move {
        let a = app.clone();
        engine::job(&app, &eng, "ven", "Ven", move |cancel| async move {
            if !ven::state().discord {
                anyhow::bail!("Discord isn't installed — install it first");
            }
            emit(&a, ev("ven", "installing"));
            ven::install().await?;
            let last = ven::run_now(&cancel).await?;
            if !last.vencord_ok {
                anyhow::bail!("Ven is installed and starts with Windows, but its first run failed: {}", last.message);
            }
            Ok((Outcome::Ok, None, Some(last.message)))
        })
        .await
    });
}

/// Runs Ven now (update Vencord, start Discord), as the signed-in user.
#[tauri::command]
fn ven_run(app: AppHandle, eng: Eng<'_>) {
    let eng = eng.inner().clone();
    tauri::async_runtime::spawn(async move {
        let a = app.clone();
        engine::job(&app, &eng, "ven-run", "Ven", move |cancel| async move {
            emit(&a, ev("ven-run", "installing"));
            let last = ven::run_now(&cancel).await?;
            if !last.vencord_ok || !last.discord_started {
                anyhow::bail!("{}", last.message);
            }
            Ok((Outcome::Ok, None, Some(last.message)))
        })
        .await
    });
}

#[tauri::command]
fn ven_remove() -> CmdResult<()> {
    ven::remove().map_err(err)
}

// ---------- activation ----------

#[tauri::command]
async fn license_status() -> CmdResult<Vec<activation::License>> {
    activation::status().await.map_err(err)
}

#[tauri::command]
async fn activate_key(key: String) -> CmdResult<String> {
    activation::use_key(&key).await.map_err(err)
}

// ---------- settings / logs ----------

#[tauri::command]
fn save_settings(eng: Eng<'_>, s: settings::Settings) -> CmdResult<()> {
    eng.set_parallel(s.parallel_downloads);
    settings::save(&s).map_err(err)
}

#[tauri::command]
fn set_secret(name: String, value: String) -> CmdResult<()> {
    settings::set_secret(&name, &value).map_err(err)
}

#[tauri::command]
fn logs_dir() -> String {
    util::app_dir().join("logs").display().to_string()
}

#[tauri::command]
fn export_log() -> CmdResult<String> {
    let dir = util::app_dir().join("logs");
    let latest = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .ok_or("no log yet")?;
    let dest = util::desktop_dir().join(format!("SetupHub-{}", latest.file_name().to_string_lossy()));
    std::fs::copy(latest.path(), &dest).map_err(|e| e.to_string())?;
    Ok(dest.display().to_string())
}

pub fn run() {
    init_logging();
    tracing::info!("Setup Hub {} starting", env!("CARGO_PKG_VERSION"));
    update::cleanup_old();
    let st = settings::load();
    let (cat, origin) = tauri::async_runtime::block_on(catalog::load(&st.catalog_url));
    let eng = Arc::new(Engine::new(cat, origin, st.parallel_downloads));

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(eng)
        .invoke_handler(tauri::generate_handler![
            bootstrap, detect_installed, reload_catalog, winget_status,
            install, cancel, reboot_pending, restart_now,
            tweak_states, tweak_apply, tweak_revert,
            defaults_states, defaults_apply, defaults_revert, defaults_settings_uri,
            check_self_update, apply_self_update, check_app_updates,
            gpu_info, install_nvidia,
            drive_list, drive_folder_info, drive_download,
            myapps_list, myapps_analyze, myapps_add, myapps_update, myapps_remove, myapps_duplicate, myapps_reorder,
            myapps_check, myapps_uninstall, myapps_export, myapps_import, myapps_fetch_url, log_tail,
            ven_state, ven_install, ven_run, ven_remove,
            license_status, activate_key,
            save_settings, set_secret, logs_dir, export_log
        ])
        .run(tauri::generate_context!())
        .expect("error while running Setup Hub");
}
