//! Read-only checks against this machine and the live endpoints. Not run in CI:
//! `cargo test --lib live -- --ignored --nocapture`

use crate::*;

#[tokio::test]
#[ignore]
async fn live_detect_installed() {
    let c = catalog::parse(catalog::EMBEDDED).unwrap();
    let found = detect::detect_all(&c.items);
    println!("{found:#?}");
    assert!(!found.is_empty());
}

#[tokio::test]
#[ignore]
async fn live_gpu_and_nvidia_lookup() {
    let gpus = gpu::detect().await.unwrap();
    println!("{gpus:#?}");
    if let Some(g) = gpus.iter().find(|g| g.vendor == gpu::Vendor::Nvidia) {
        let d = gpu::latest_nvidia(&g.name).await.unwrap();
        println!("latest: {d:?}; newer than installed: {}", gpu::is_newer(&d.version, g.display_version.as_deref().unwrap_or("0")));
        assert!(d.url.ends_with(".exe"));
    }
    println!("nvidia app: {}", gpu::nvidia_app_url().await.unwrap());
}

#[tokio::test]
#[ignore]
async fn live_drive_listing() {
    let e = drive::list(drive::FOLDER_ID, None).await.unwrap();
    for x in &e {
        println!("{:>12?}  {}/{}  ({})", x.size, x.path, x.name, x.mime);
    }
    assert!(!e.is_empty());
    let first = e.iter().find(|x| !x.is_folder).unwrap();
    println!("download url: {}", drive::resolve_download(&first.id).await.unwrap());
}

#[tokio::test]
#[ignore]
async fn live_tweak_and_license_state() {
    for id in tweaks::IDS {
        match tweaks::state(id).await {
            Ok(s) => println!("{id}: applied={} detail={}", s.applied, s.detail),
            Err(e) => println!("{id}: ERR {e:#}"),
        }
    }
    for l in activation::status().await.unwrap() {
        println!("license: {} status={} key=…{}", l.name, l.status, l.partial_key);
    }
    println!("winget: {}", installer::winget_available().await);
    let v = ven::state();
    println!("ven: installed={} autostart={} discord={} vencord={} last={:?}", v.installed, v.autostart, v.discord, v.vencord, v.last_run);
}

#[tokio::test]
#[ignore]
async fn live_myapps_analyze() {
    for input in [
        "https://github.com/M2Team/NanaZip",
        "https://download.cdn.viber.com/desktop/windows/ViberSetup.msi",
        "M2Team.NanaZip",
        "obs studio",
        "9NKSQGP7F2NH",
        "https://avamodmanager.com/",
        "https://example.com/run.ps1",
        "https://drive.google.com/drive/folders/1P_yy_Sw-Ca4m7sfqEmdLjpGeIPY5DZoU?usp=sharing",
    ] {
        let a = myapps::analyze(input, false, None).await;
        let s = serde_json::to_string(&a).unwrap();
        println!("{input}\n  → {}\n", &s[..s.len().min(420)]);
    }
}

#[tokio::test]
#[ignore]
async fn live_unelevated_runner() {
    let code = installer::run_unelevated("cmd.exe /c exit /b 7", &std::sync::atomic::AtomicBool::new(false)).await.unwrap();
    assert_eq!(code, 7);
}

#[tokio::test]
#[ignore]
async fn live_github_resolve() {
    let c = catalog::parse(catalog::EMBEDDED).unwrap();
    for it in c.items.iter().filter(|i| matches!(i.source, catalog::Source::GithubRelease { .. } | catalog::Source::Scrape { .. })) {
        let r = catalog::resolve(it, None).await.unwrap();
        println!("{}: {} sha256={:?}", it.id, r.url, r.sha256);
    }
}
