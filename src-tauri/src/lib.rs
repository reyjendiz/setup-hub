pub mod activation;
pub mod ad;
pub mod catalog;
pub mod detect;
pub mod download;
pub mod drive;
pub mod engine;
pub mod gpu;
pub mod installer;
pub mod settings;
pub mod sig;
pub mod tweaks;
pub mod util;
#[cfg(test)]
mod live_tests;

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
    let items = eng.catalog.read().unwrap().items.clone();
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

// ---------- GPU ----------

#[derive(Serialize)]
struct GpuInfo {
    gpus: Vec<gpu::Gpu>,
    nvidia_latest: Option<gpu::DriverInfo>,
    nvidia_error: Option<String>,
    update_available: bool,
}

#[tauri::command]
async fn gpu_info() -> CmdResult<GpuInfo> {
    let gpus = gpu::detect().await.map_err(err)?;
    let nv = gpus.iter().find(|g| g.vendor == gpu::Vendor::Nvidia).cloned();
    let (mut latest, mut error, mut update) = (None, None, false);
    if let Some(g) = nv {
        match gpu::latest_nvidia(&g.name).await {
            Ok(d) => {
                update = g.display_version.as_deref().is_none_or(|cur| gpu::is_newer(&d.version, cur));
                latest = Some(d);
            }
            Err(e) => error = Some(format!("{e:#}")),
        }
    }
    Ok(GpuInfo { gpus, nvidia_latest: latest, nvidia_error: error, update_available: update })
}

#[tauri::command]
fn install_nvidia(app: AppHandle, eng: Eng<'_>, clean: bool) {
    let eng = eng.inner().clone();
    tauri::async_runtime::spawn(async move {
        let (a, e) = (app.clone(), eng.clone());
        engine::job(&app, &eng, "nvidia", "NVIDIA driver", move |cancel| async move {
            let gpus = gpu::detect().await?;
            let g = gpus.iter().find(|g| g.vendor == gpu::Vendor::Nvidia).ok_or_else(|| anyhow::anyhow!("No NVIDIA GPU detected — nothing to install"))?;
            emit(&a, ev("nvidia", "resolving"));
            match gpu::latest_nvidia(&g.name).await {
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
                    tracing::warn!("NVIDIA lookup failed ({err:#}); falling back to NVIDIA App");
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
async fn drive_list() -> CmdResult<Vec<drive::Entry>> {
    let key = settings::get_secret("google_api_key");
    drive::list(drive::FOLDER_ID, key.as_deref()).await.map_err(err)
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

// ---------- AD ----------

#[tauri::command]
fn ad_state() -> ad::AdState {
    ad::state()
}

#[tauri::command]
fn ad_install(app: AppHandle, eng: Eng<'_>) {
    let eng = eng.inner().clone();
    tauri::async_runtime::spawn(async move {
        let a = app.clone();
        engine::job(&app, &eng, "ad", "AD", move |cancel| async move {
            if !ad::dotnet8_desktop_present() {
                emit(&a, JobEvent { message: Some(".NET 8 Desktop Runtime".into()), ..ev("ad", "installing") });
                if !installer::ensure_winget().await {
                    anyhow::bail!("AD needs the .NET 8 Desktop Runtime and winget is unavailable to install it");
                }
                installer::winget_install("Microsoft.DotNet.DesktopRuntime.8", &cancel, &|_, _| {}).await?;
            }
            emit(&a, ev("ad", "installing"));
            let path = ad::install(&cancel).await?;
            let _ = std::process::Command::new(&path).spawn();
            Ok((Outcome::Ok, None, Some(path)))
        })
        .await
    });
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
        .manage(eng)
        .invoke_handler(tauri::generate_handler![
            bootstrap, detect_installed, reload_catalog, winget_status,
            install, cancel, reboot_pending, restart_now,
            tweak_states, tweak_apply, tweak_revert,
            gpu_info, install_nvidia,
            drive_list, drive_download,
            ad_state, ad_install,
            license_status, activate_key,
            save_settings, set_secret, logs_dir, export_log
        ])
        .run(tauri::generate_context!())
        .expect("error while running Setup Hub");
}
