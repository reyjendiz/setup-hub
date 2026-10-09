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

/// Installer technology, sniffed from the file name and binary markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Msi,
    Msix,
    Zip,
    SevenZip,
    Inno,
    Nsis,
    WixBurn,
    Squirrel,
    InstallShield,
    Unknown,
}

pub const SCRIPT_EXTS: [&str; 11] = ["bat", "cmd", "ps1", "psm1", "vbs", "vbe", "js", "jse", "wsf", "hta", "reg"];

pub fn ext_of(name: &str) -> String {
    let path = name.split(['?', '#']).next().unwrap_or(name);
    Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

pub fn is_script(name: &str) -> bool {
    SCRIPT_EXTS.contains(&ext_of(name).as_str())
}

pub fn kind_of(name: &str, data: &[u8]) -> Kind {
    match ext_of(name).as_str() {
        "msi" => return Kind::Msi,
        "msix" | "msixbundle" | "appx" | "appxbundle" => return Kind::Msix,
        "zip" => return Kind::Zip,
        "7z" => return Kind::SevenZip,
        _ => {}
    }
    let has = |s: &[u8]| data.windows(s.len()).any(|w| w == s);
    if has(b"Inno Setup") {
        Kind::Inno
    } else if has(b"Nullsoft") || has(b"NSIS Error") {
        Kind::Nsis
    } else if has(b".wixburn") {
        Kind::WixBurn
    } else if has(b"Squirrel") {
        Kind::Squirrel
    } else if has(b"InstallShield") {
        Kind::InstallShield
    } else {
        Kind::Unknown
    }
}

/// Reads the head and tail of the file (markers live there; installers can be gigabytes).
pub fn detect_kind(path: &Path) -> Kind {
    use std::io::{Read, Seek, SeekFrom};
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let Ok(mut f) = std::fs::File::open(path) else { return Kind::Unknown };
    let mut data = Vec::new();
    let _ = (&mut f).take(16 << 20).read_to_end(&mut data);
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    if len > 20 << 20 && f.seek(SeekFrom::End(-(4 << 20))).is_ok() {
        let _ = f.take(4 << 20).read_to_end(&mut data);
    }
    kind_of(&name, &data)
}

pub fn suggested_args(k: Kind) -> Vec<String> {
    let a: &[&str] = match k {
        Kind::Inno => &["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"],
        Kind::Nsis => &["/S"],
        Kind::Msi => &["/qn", "/norestart"],
        Kind::WixBurn => &["/quiet", "/norestart"],
        Kind::Squirrel => &["--silent"],
        Kind::InstallShield => &["/s", "/v\"/qn\""],
        _ => &[],
    };
    a.iter().map(|s| s.to_string()).collect()
}

/// Splits a user-typed argument string on spaces, keeping "quoted parts" together.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in s.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                cur.push(c);
            }
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
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

pub fn winget_args(id: &str, source: &str) -> Vec<String> {
    [
        "install", "--id", id, "-e", "--source", source, "--silent",
        "--accept-package-agreements", "--accept-source-agreements", "--disable-interactivity",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Runs winget, streaming download percent. Returns the outcome and the tail of its output.
pub async fn winget_install(
    id: &str,
    source: &str,
    cancel: &AtomicBool,
    on_progress: &(dyn Fn(Option<f64>, &str) + Send + Sync),
) -> Result<(Outcome, String)> {
    let mut child = util::tokio_cmd("winget.exe")
        .args(winget_args(id, source))
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

/// Quotes an argument only when it has spaces and no quotes of its own, so user-typed
/// switches like `/v"/qn"` reach the installer verbatim.
fn arg_text(a: &str) -> String {
    if a.contains(' ') && !a.contains('"') { format!("\"{a}\"") } else { a.to_string() }
}

pub async fn run_installer(file: &Path, args: &[String], unelevated: bool, cancel: &AtomicBool) -> Result<Outcome> {
    // Defense in depth: scripts are refused at analysis time too.
    if is_script(&file.to_string_lossy()) {
        bail!("refusing to run a script file ({})", file.display());
    }
    let (prog, a) = installer_cmd(file, args);
    if unelevated {
        let mut line = format!("\"{prog}\"");
        for x in &a {
            line.push(' ');
            line.push_str(&arg_text(x));
        }
        return exit_outcome(run_unelevated(&line, cancel).await?);
    }
    let mut cmd = tokio::process::Command::new(&prog);
    for x in &a {
        cmd.raw_arg(arg_text(x));
    }
    let mut child = cmd
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

/// Extracts .zip (built in) or .7z (Windows 11's bundled tar.exe / libarchive) into `dest`.
pub async fn extract(archive: &Path, dest: &Path) -> Result<()> {
    if ext_of(&archive.to_string_lossy()) == "zip" {
        unzip(archive, dest)?;
        return Ok(());
    }
    std::fs::create_dir_all(dest)?;
    let tar = util::expand_env(r"%windir%\System32\tar.exe");
    let (code, out) = util::run(&tar, &["-xf", &archive.to_string_lossy(), "-C", &dest.to_string_lossy()]).await?;
    if code != 0 {
        bail!("could not extract .7z (needs Windows 11 23H2+ tar.exe): {}", out.trim());
    }
    Ok(())
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
    fn installer_kind_detection() {
        assert_eq!(kind_of("x.MSI", b""), Kind::Msi);
        assert_eq!(kind_of("NanaZip_7.msixbundle", b""), Kind::Msix);
        assert_eq!(kind_of("a.7z", b""), Kind::SevenZip);
        assert_eq!(kind_of("a.zip?x=1", b""), Kind::Zip);
        assert_eq!(kind_of("s.exe", b"MZ....Inno Setup Setup Data (6.2.0)"), Kind::Inno);
        assert_eq!(kind_of("s.exe", b"MZ..Nullsoft Install System v3.08"), Kind::Nsis);
        // "Burn" alone appears in plenty of binaries (that was GearUP's false positive); only the PE section counts.
        assert_eq!(kind_of("s.exe", b"MZ CD Burner Burn"), Kind::Unknown);
        assert_eq!(kind_of("s.exe", b"MZ...PE..text.rdata\x00.wixburn\x00"), Kind::WixBurn);
        assert_eq!(kind_of("s.exe", b"MZ SquirrelSetup.log"), Kind::Squirrel);
        assert_eq!(kind_of("s.exe", b"MZ InstallShield(R)"), Kind::InstallShield);
        assert_eq!(kind_of("s.exe", b"MZ nothing to see"), Kind::Unknown);
        assert_eq!(suggested_args(Kind::Inno), vec!["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"]);
        assert_eq!(suggested_args(Kind::Nsis), vec!["/S"]);
        assert!(suggested_args(Kind::Unknown).is_empty());
    }

    #[test]
    fn scripts_and_args() {
        for s in ["a.bat", "B.CMD", "x.ps1", "y.vbs", "z.js", "q.hta", "r.reg", "https://h/x.ps1?dl=1"] {
            assert!(is_script(s), "{s}");
        }
        assert!(!is_script("setup.exe") && !is_script("a.msi"));
        assert_eq!(split_args(r#"/S /D="C:\Program Files\X"  /v"/qn""#), vec!["/S", r#"/D="C:\Program Files\X""#, r#"/v"/qn""#]);
        assert_eq!(arg_text(r"C:\a b\x.msi"), r#""C:\a b\x.msi""#);
        assert_eq!(arg_text(r#"/v"/qn""#), r#"/v"/qn""#);
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
