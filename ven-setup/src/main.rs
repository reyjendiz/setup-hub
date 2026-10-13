//! Standalone installer for Ven (../ven). Same steps as Setup Hub's Ven page, without Setup Hub:
//! writes %LOCALAPPDATA%\Programs\Ven\Ven.exe, adds HKCU\…\Run\Ven = `"…\Ven.exe" --startup`, runs Ven once.
//! `Ven-Setup.exe --uninstall` removes it and gives Discord its own autostart back.
//! Build: `VEN_EXE=<path to release ven.exe> cargo build --release`.

use std::io::BufRead;
use std::path::PathBuf;
use std::process::Command;
use winreg::enums::*;
use winreg::RegKey;

const EXE: &[u8] = include_bytes!(env!("VEN_EXE"));
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

fn local() -> Result<PathBuf, String> {
    std::env::var("LOCALAPPDATA").map(PathBuf::from).map_err(|e| format!("LOCALAPPDATA: {e}"))
}

fn install() -> Result<String, String> {
    let dir = local()?.join(r"Programs\Ven");
    let exe = dir.join("Ven.exe");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(&exe, EXE).map_err(|e| format!("cannot write {} (is Ven running? try again in a minute): {e}", exe.display()))?;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.create_subkey(RUN_KEY).and_then(|k| k.0.set_value("Ven", &format!("\"{}\" --startup", exe.display()))).map_err(|e| e.to_string())?;
    // A Run entry disabled in Task Manager stays disabled; Ven's must be on.
    if let Ok(k) = hkcu.open_subkey_with_flags(APPROVED_KEY, KEY_WRITE) {
        let _ = k.delete_value("Ven");
    }
    println!("Installed to {}\nRunning Ven once (this closes Discord, patches it with Vencord and starts it again)...", exe.display());
    let data = local()?.join("Ven");
    let _ = std::fs::remove_file(data.join("status.json"));
    Command::new(&exe).status().map_err(|e| format!("Ven didn't start: {e}"))?;
    let status = std::fs::read_to_string(data.join("status.json")).map_err(|_| format!("Ven reported nothing, see {}", data.join("ven.log").display()))?;
    let msg = status.lines().find(|l| l.contains("\"message\"")).unwrap_or("").trim();
    Ok(format!("Done. Ven will run at every sign-in.\nResult: {msg}"))
}

fn remove() -> Result<String, String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(k) = hkcu.open_subkey_with_flags(RUN_KEY, KEY_WRITE) {
        let _ = k.delete_value("Ven");
    }
    if let Ok(k) = hkcu.open_subkey_with_flags(APPROVED_KEY, KEY_WRITE) {
        let _ = k.delete_value("Discord");
    }
    let _ = std::fs::remove_dir_all(local()?.join(r"Programs\Ven"));
    let _ = std::fs::remove_dir_all(local()?.join("Ven"));
    Ok("Ven removed. Discord has its own autostart back (Vencord stays in Discord).".into())
}

fn main() {
    let r = if std::env::args().any(|a| a == "--uninstall") { remove() } else { install() };
    match r {
        Ok(m) => println!("\n{m}"),
        Err(e) => println!("\nFailed: {e}"),
    }
    println!("\nPress Enter to close.");
    let _ = std::io::stdin().lock().read_line(&mut String::new());
}
