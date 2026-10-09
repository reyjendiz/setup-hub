//! Ven — keeps Vencord installed and current, then starts Discord.
//!
//! Setup Hub installs it to %LOCALAPPDATA%\Programs\Ven and registers `Ven.exe --startup` under
//! HKCU\…\Run, so it runs at every sign-in as the signed-in user (no admin). Each run:
//!   1. turns off Discord's own autostart (Task Manager's "Disabled"), so Discord can't open unpatched first;
//!   2. updates VencordInstallerCli.exe from Vencord/Installer's latest GitHub release (SHA-256 checked);
//!   3. runs `VencordInstallerCli -install -branch stable`: it downloads the latest Vencord when it changed,
//!      closes Discord if it's open and patches the newest Discord version;
//!   4. starts Discord (to the tray at sign-in, like Discord's own autostart).
//! Without network it skips 2–3 and still starts Discord. Everything is logged to %LOCALAPPDATA%\Ven\ven.log
//! and the outcome of the last run is kept in status.json for Setup Hub's Ven page.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod logic;

use logic::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn local_appdata() -> PathBuf {
    PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into()))
}

fn data_dir() -> PathBuf {
    local_appdata().join("Ven")
}

fn discord_dir() -> PathBuf {
    local_appdata().join("Discord")
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

struct Log(PathBuf);

impl Log {
    fn open() -> Self {
        let p = data_dir().join("ven.log");
        let _ = std::fs::create_dir_all(data_dir());
        if let Ok(t) = std::fs::read_to_string(&p) {
            if t.len() > 256 * 1024 {
                let _ = std::fs::write(&p, trim_log(&t, 128 * 1024));
            }
        }
        Log(p)
    }

    fn line(&self, msg: impl AsRef<str>) {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&self.0) {
            let _ = writeln!(f, "[{}] {}", utc_stamp(now()), msg.as_ref().trim_end());
        }
    }
}

#[derive(Serialize)]
struct Status {
    /// Unix time of this run.
    time: u64,
    /// Vencord was installed/updated in this run.
    vencord_ok: bool,
    discord_started: bool,
    message: String,
}

fn main() {
    let startup = std::env::args().any(|a| a == "--startup");
    let log = Log::open();
    log.line(format!("Ven {} starting{}", env!("CARGO_PKG_VERSION"), if startup { " (sign-in)" } else { "" }));
    let status = run(startup, &log);
    log.line(format!("done: {}", status.message));
    let _ = std::fs::write(data_dir().join("status.json"), serde_json::to_string_pretty(&status).unwrap_or_default());
    std::process::exit(if status.vencord_ok && status.discord_started { 0 } else { 1 });
}

fn run(startup: bool, log: &Log) -> Status {
    let update_exe = discord_dir().join("Update.exe");
    if !update_exe.is_file() {
        return Status { time: now(), vencord_ok: false, discord_started: false, message: "Discord isn't installed".into() };
    }
    if let Err(e) = disable_discord_autostart() {
        log.line(format!("could not turn off Discord's own autostart: {e}"));
    }

    let mut problems = Vec::new();
    let cli = data_dir().join(CLI_ASSET);
    // At sign-in the network can take a while to come up.
    let agent = ureq::AgentBuilder::new().redirects(0).timeout(Duration::from_secs(30)).user_agent(concat!("Ven/", env!("CARGO_PKG_VERSION"))).build();
    let online = wait_for_network(&agent, if startup { 120 } else { 10 }, log);
    if online {
        if let Err(e) = update_cli(&agent, &cli, log) {
            log.line(format!("installer update failed: {e}"));
            problems.push(format!("Vencord installer not updated: {e}"));
        }
    } else {
        problems.push("offline — Vencord not checked".into());
    }

    let vencord_ok = online && cli.is_file() && match patch(&cli, log) {
        Ok(()) => true,
        Err(e) => {
            problems.push(e);
            false
        }
    };

    let discord_started = match launch_discord(&update_exe, startup) {
        Ok(()) => true,
        Err(e) => {
            problems.push(format!("Discord didn't start: {e}"));
            false
        }
    };

    let message = if problems.is_empty() { "Vencord is up to date; Discord started".to_string() } else { problems.join("; ") };
    Status { time: now(), vencord_ok, discord_started, message }
}

fn wait_for_network(agent: &ureq::Agent, secs: u64, log: &Log) -> bool {
    let start = Instant::now();
    loop {
        match agent.head("https://api.github.com/").call() {
            Ok(_) | Err(ureq::Error::Status(..)) => return true,
            Err(e) if start.elapsed() >= Duration::from_secs(secs) => {
                log.line(format!("no network after {secs}s: {e}"));
                return false;
            }
            Err(_) => std::thread::sleep(Duration::from_secs(5)),
        }
    }
}

/// GET that follows redirects only within GitHub's hosts.
fn get(agent: &ureq::Agent, url: &str) -> Result<ureq::Response, String> {
    let mut url = url.to_string();
    for _ in 0..10 {
        let host = url.split("://").nth(1).and_then(|r| r.split(['/', '?', ':']).next()).unwrap_or("").to_string();
        if !url.starts_with("https://") || !host_allowed(&host) {
            return Err(format!("refusing to download from {url}"));
        }
        let resp = agent.get(&url).set("Accept", "application/vnd.github+json, */*").call().map_err(|e| e.to_string())?;
        if (300..400).contains(&resp.status()) {
            url = resp.header("location").ok_or("redirect without location")?.to_string();
            continue;
        }
        return Ok(resp);
    }
    Err("too many redirects".into())
}

fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// Replaces the local VencordInstallerCli.exe when the latest release's file differs.
fn update_cli(agent: &ureq::Agent, cli: &Path, log: &Log) -> Result<(), String> {
    let rel: Release = serde_json::from_reader(get(agent, INSTALLER_RELEASE)?.into_reader().take(4 << 20)).map_err(|e| e.to_string())?;
    let (url, want) = cli_asset(&rel)?;
    if std::fs::read(cli).map(|b| sha256_hex(&b) == want).unwrap_or(false) {
        return Ok(());
    }
    let mut body = Vec::new();
    get(agent, &url)?.into_reader().take(64 << 20).read_to_end(&mut body).map_err(|e| e.to_string())?;
    let got = sha256_hex(&body);
    if got != want {
        return Err(format!("SHA-256 mismatch for {CLI_ASSET}: expected {want}, got {got}"));
    }
    let tmp = cli.with_extension("exe.new");
    std::fs::write(&tmp, &body).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, cli).map_err(|e| e.to_string())?;
    log.line(format!("installer updated to {}", rel.tag_name));
    Ok(())
}

/// `-install` downloads Vencord when its release changed, closes Discord and patches its newest version.
fn patch(cli: &Path, log: &Log) -> Result<(), String> {
    let mut cmd = Command::new(cli);
    cmd.args(["-install", "-branch", "stable"]).current_dir(data_dir());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().map_err(|e| format!("installer didn't start: {e}"))?;
    for l in String::from_utf8_lossy(&out.stdout).lines().chain(String::from_utf8_lossy(&out.stderr).lines()) {
        log.line(format!("  installer: {l}"));
    }
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("Vencord installer failed (exit {})", out.status.code().unwrap_or(-1)))
    }
}

fn launch_discord(update_exe: &Path, startup: bool) -> Result<(), String> {
    let mut cmd = Command::new(update_exe);
    cmd.args(discord_args(startup)).current_dir(update_exe.parent().unwrap_or(Path::new(".")));
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Marks Discord's own Run entry as disabled (what Task Manager › Startup apps does); Discord
/// rewrites its Run value but never this flag. Ven starts Discord instead, after patching.
#[cfg(windows)]
fn disable_discord_autostart() -> Result<(), String> {
    use winreg::enums::*;
    let hkcu = winreg::RegKey::predef(HKEY_CURRENT_USER);
    let k = hkcu
        .create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run")
        .map_err(|e| e.to_string())?
        .0;
    let v = winreg::RegValue { vtype: REG_BINARY, bytes: STARTUP_DISABLED.to_vec() };
    k.set_raw_value("Discord", &v).map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn disable_discord_autostart() -> Result<(), String> {
    Ok(())
}
