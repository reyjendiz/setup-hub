use crate::util;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::io::AsyncReadExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Reboot,
}

/// Maps installer exit codes. 3010/1641 = success, reboot required.
pub fn exit_outcome(code: i32) -> Result<Outcome> {
    match code {
        0 => Ok(Outcome::Ok),
        3010 | 1641 => Ok(Outcome::Reboot),
        // winget: already installed / no applicable upgrade
        c if c == 0x8A15002Bu32 as i32 || c == 0x8A150061u32 as i32 => Ok(Outcome::Ok),
        // winget: reboot required to finish
        c if c == 0x8A150109u32 as i32 => Ok(Outcome::Reboot),
        c => bail!("installer exited with code {c} (0x{:08X})", c as u32),
    }
}

pub async fn winget_available() -> bool {
    util::run("winget.exe", &["--version"]).await.map(|(c, _)| c == 0).unwrap_or(false)
}

/// Fresh Windows often ships App Installer unregistered for the user; registering it makes winget appear.
pub async fn ensure_winget() -> bool {
    if winget_available().await {
        return true;
    }
    tracing::info!("winget missing, registering App Installer");
    let _ = util::run(
        "powershell.exe",
        &["-NoProfile", "-Command", "Add-AppxPackage -RegisterByFamilyName -MainPackage Microsoft.DesktopAppInstaller_8wekyb3d8bbwe"],
    )
    .await;
    winget_available().await
}

/// Parses "12.3 MB / 97.4 MB" style progress out of winget's console output.
pub fn winget_percent(line: &str) -> Option<f64> {
    let re = regex::Regex::new(r"([\d.]+)\s*(KB|MB|GB)\s*/\s*([\d.]+)\s*(KB|MB|GB)").unwrap();
    let c = re.captures_iter(line).last()?;
    let unit = |u: &str| match u {
        "KB" => 1e3,
        "MB" => 1e6,
        _ => 1e9,
    };
    let a: f64 = c[1].parse::<f64>().ok()? * unit(&c[2]);
    let b: f64 = c[3].parse::<f64>().ok()? * unit(&c[4]);
    (b > 0.0).then(|| (a / b * 100.0).min(100.0))
}

pub fn winget_args(id: &str) -> Vec<String> {
    [
        "install", "--id", id, "-e", "--source", "winget", "--silent",
        "--accept-package-agreements", "--accept-source-agreements", "--disable-interactivity",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Runs winget, streaming download percent. Returns the outcome and the tail of its output.
pub async fn winget_install(
    id: &str,
    cancel: &AtomicBool,
    on_progress: &(dyn Fn(Option<f64>, &str) + Send + Sync),
) -> Result<(Outcome, String)> {
    let mut child = util::tokio_cmd("winget.exe")
        .args(winget_args(id))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("winget failed to start")?;
    let mut out = child.stdout.take().unwrap();
    let mut buf = [0u8; 4096];
    let mut log = String::new();
    loop {
        tokio::select! {
            n = out.read(&mut buf) => {
                let n = n?;
                if n == 0 { break; }
                let s = String::from_utf8_lossy(&buf[..n]);
                for seg in s.split(['\r', '\n']).filter(|x| !x.trim().is_empty()) {
                    let pct = winget_percent(seg);
                    if pct.is_none() && !seg.contains('█') && !seg.trim().chars().all(|c| "-\\|/ ".contains(c)) {
                        log.push_str(seg.trim());
                        log.push('\n');
                    }
                    on_progress(pct, seg.trim());
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(300)) => {
                if cancel.load(Ordering::Relaxed) { let _ = child.kill().await; bail!(crate::download::Cancelled); }
            }
        }
    }
    let code = child.wait().await?.code().unwrap_or(-1);
    let tail: String = log.lines().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
    tracing::info!("winget {id} exit {code}: {tail}");
    exit_outcome(code).map(|o| (o, tail.clone())).with_context(|| tail)
}

/// Command line for an installer file, by extension.
pub fn installer_cmd(file: &Path, args: &[String]) -> (String, Vec<String>) {
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if ext == "msi" {
        let mut a = vec!["/i".to_string(), file.display().to_string()];
        a.extend(args.iter().cloned());
        ("msiexec.exe".into(), a)
    } else if matches!(ext.as_str(), "msix" | "msixbundle" | "appx" | "appxbundle") {
        let p = file.display().to_string().replace('\'', "''");
        let script = format!("$ErrorActionPreference='Stop';Add-AppxPackage -Path '{p}' -ForceApplicationShutdown");
        ("powershell.exe".into(), ["-NoProfile", "-NonInteractive", "-Command", &script].map(String::from).to_vec())
    } else {
        (file.display().to_string(), args.to_vec())
    }
}

pub async fn run_installer(file: &Path, args: &[String], unelevated: bool, cancel: &AtomicBool) -> Result<Outcome> {
    let (prog, a) = installer_cmd(file, args);
    if unelevated {
        let mut line = format!("\"{prog}\"");
        for x in &a {
            line.push_str(&format!(" \"{x}\""));
        }
        return exit_outcome(run_unelevated(&line, cancel).await?);
    }
    let mut child = tokio::process::Command::new(&prog)
        .args(&a)
        .current_dir(file.parent().unwrap_or(Path::new(".")))
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("failed to start {prog}"))?;
    loop {
        tokio::select! {
            st = child.wait() => return exit_outcome(st?.code().unwrap_or(-1)),
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                if cancel.load(Ordering::Relaxed) { let _ = child.kill().await; bail!(crate::download::Cancelled); }
            }
        }
    }
}

/// Runs a command line de-elevated (medium integrity, as the signed-in user) through a one-shot
/// scheduled task with /RL LIMITED, waiting for its exit code. Needed for Spotify, which refuses admin.
pub async fn run_unelevated(cmdline: &str, cancel: &AtomicBool) -> Result<i32> {
    let dir = util::cache_dir().join("unelevated");
    std::fs::create_dir_all(&dir)?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis();
    let cmd = dir.join(format!("run-{stamp}.cmd"));
    let exit = dir.join(format!("run-{stamp}.exit"));
    // Redirect first: "echo 7> f" would parse "7>" as a handle redirect and write nothing.
    std::fs::write(&cmd, format!("@echo off\r\n{cmdline}\r\n>\"{}\" echo %ERRORLEVEL%\r\n", exit.display()))?;
    let task = format!("SetupHub-Unelevated-{stamp}");
    let tr = format!("cmd.exe /c \"{}\"", cmd.display());
    let (c, o) = util::run("schtasks.exe", &["/Create", "/TN", &task, "/TR", &tr, "/SC", "ONCE", "/ST", "00:00", "/RL", "LIMITED", "/F"]).await?;
    if c != 0 {
        bail!("could not create de-elevation task: {o}");
    }
    let (c, o) = util::run("schtasks.exe", &["/Run", "/TN", &task]).await?;
    let result = async {
        if c != 0 {
            bail!("could not start de-elevation task: {o}");
        }
        for _ in 0..(60 * 60) {
            if cancel.load(Ordering::Relaxed) {
                bail!(crate::download::Cancelled);
            }
            if let Ok(s) = std::fs::read_to_string(&exit) {
                if let Ok(code) = s.trim().parse::<i32>() {
                    return Ok(code);
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        bail!("timed out after 60 minutes")
    }
    .await;
    let _ = util::run("schtasks.exe", &["/Delete", "/TN", &task, "/F"]).await;
    let _ = std::fs::remove_file(&cmd);
    let _ = std::fs::remove_file(&exit);
    result
}

/// Extracts a zip, refusing entries that escape `dest` (zip-slip).
pub fn unzip(zip: &Path, dest: &Path) -> Result<Vec<PathBuf>> {
    let mut a = zip::ZipArchive::new(std::fs::File::open(zip)?)?;
    std::fs::create_dir_all(dest)?;
    let mut files = Vec::new();
    for i in 0..a.len() {
        let mut e = a.by_index(i)?;
        let Some(rel) = e.enclosed_name() else { bail!("unsafe path in zip: {}", e.name()) };
        let out = dest.join(rel);
        if e.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::io::copy(&mut e, &mut std::fs::File::create(&out)?)?;
        files.push(out);
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes() {
        assert_eq!(exit_outcome(0).unwrap(), Outcome::Ok);
        assert_eq!(exit_outcome(3010).unwrap(), Outcome::Reboot);
        assert_eq!(exit_outcome(0x8A15002Bu32 as i32).unwrap(), Outcome::Ok);
        assert_eq!(exit_outcome(0x8A150109u32 as i32).unwrap(), Outcome::Reboot);
        assert!(exit_outcome(1603).is_err());
    }

    #[test]
    fn winget_progress_parse() {
        assert_eq!(winget_percent("  ██████▒▒▒▒  50.0 MB / 100 MB").unwrap().round(), 50.0);
        assert_eq!(winget_percent("1.00 GB / 1.00 GB").unwrap(), 100.0);
        assert!((winget_percent("512 KB / 2.00 MB").unwrap() - 25.6).abs() < 0.01);
        assert!(winget_percent("Found Steam [Valve.Steam]").is_none());
    }

    #[test]
    fn msi_goes_through_msiexec() {
        let (p, a) = installer_cmd(Path::new(r"C:\c\x.msi"), &["/qn".into()]);
        assert_eq!(p, "msiexec.exe");
        assert_eq!(a, vec!["/i", r"C:\c\x.msi", "/qn"]);
        let (p, a) = installer_cmd(Path::new(r"C:\c\x.exe"), &["/S".into()]);
        assert_eq!(p, r"C:\c\x.exe");
        assert_eq!(a, vec!["/S"]);
        let (p, a) = installer_cmd(Path::new(r"C:\c\NanaZip_7.msixbundle"), &[]);
        assert_eq!(p, "powershell.exe");
        assert!(a.last().unwrap().contains(r"Add-AppxPackage -Path 'C:\c\NanaZip_7.msixbundle'"));
    }
}
