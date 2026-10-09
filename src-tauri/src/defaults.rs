//! Makes apps the default for what they replace in Windows: VLC instead of Media Player, Chrome
//! instead of Edge for links and web pages, PdfCraft instead of Edge for PDFs.
//!
//! It goes through Windows' own "default associations configuration" policy: Windows writes the
//! protected UserChoice keys itself at the next sign-in. Associations are marked Suggested, so on
//! Windows 11 22H2+ they are applied once per Version and the user can still change them afterwards
//! (older builds re-apply them at every sign-in until reverted). No UserChoice hash is forged.

use crate::catalog::{Defaults, Item};
use crate::util;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use winreg::enums::*;
use winreg::RegKey;

const POLICY_KEY: &str = r"SOFTWARE\Policies\Microsoft\Windows\System";
const POLICY_VALUE: &str = "DefaultAssociationsConfiguration";

/// The policy is machine-wide, so its file and our record of it live in ProgramData.
fn data_dir() -> PathBuf {
    PathBuf::from(util::expand_env(r"%ProgramData%\SetupHub"))
}

fn xml_path() -> PathBuf {
    data_dir().join("DefaultAssociations.xml")
}

fn store_path() -> PathBuf {
    data_dir().join("defaults.json")
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Store {
    /// DefaultAssociations Version; bumping it makes Windows apply Suggested associations once more.
    version: u32,
    /// Unix time of the last policy write (pending until the next sign-in after it).
    written_at: u64,
    /// Catalog item id → the associations we put in the policy for it.
    assoc: BTreeMap<String, AppAssoc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppAssoc {
    application: String,
    /// ".mp4" → "VLC.mp4", "https" → "ChromeHTML"
    prog_ids: BTreeMap<String, String>,
}

fn load() -> Store {
    std::fs::read_to_string(store_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save(s: &Store) -> Result<()> {
    std::fs::create_dir_all(data_dir())?;
    std::fs::write(store_path(), serde_json::to_string_pretty(s)?)?;
    Ok(())
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn boot_time() -> u64 {
    // SAFETY: GetTickCount64 has no preconditions.
    let up = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() } / 1000;
    now().saturating_sub(up)
}

// ---------- what Windows knows ----------

/// An app's default-programs registration: Software\RegisteredApplications → Capabilities.
pub struct Registered {
    /// HKLM (registeredAppMachine) or HKCU (registeredAppUser).
    pub machine: bool,
    pub application: String,
    /// File types and link protocols: ".mp4" → "VLC.mp4", "https" → "ChromeHTML".
    pub types: BTreeMap<String, String>,
}

pub fn registered(app: &str) -> Option<Registered> {
    use winreg::types::FromRegValue;
    let views = [(HKEY_LOCAL_MACHINE, KEY_READ, true), (HKEY_LOCAL_MACHINE, KEY_READ | KEY_WOW64_32KEY, true), (HKEY_CURRENT_USER, KEY_READ, false)];
    for (hive, flags, machine) in views {
        let root = RegKey::predef(hive);
        let Ok(path) = root.open_subkey_with_flags(r"Software\RegisteredApplications", flags).and_then(|k| k.get_value::<String, _>(app)) else {
            continue;
        };
        let Ok(caps) = root.open_subkey_with_flags(&path, flags) else { continue };
        let mut types = BTreeMap::new();
        for sub in ["FileAssociations", "URLAssociations"] {
            if let Ok(k) = caps.open_subkey(sub) {
                types.extend(k.enum_values().flatten().filter_map(|(t, v)| Some((t.to_ascii_lowercase(), String::from_reg_value(&v).ok()?))));
            }
        }
        // "@C:\…\app.exe,-101" is a resource reference; the XML wants a plain name.
        let application = caps.get_value::<String, _>("ApplicationName").ok().filter(|n| !n.is_empty() && !n.starts_with('@'));
        return Some(Registered { machine, application: application.unwrap_or_else(|| app.to_string()), types });
    }
    None
}

/// The ProgId a double-click (".mp4") or a link ("https") currently opens. UserChoiceLatest is
/// where newer Windows 11 builds keep it.
pub fn current_prog_id(t: &str) -> Option<String> {
    let path = if t.starts_with('.') {
        format!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\{t}")
    } else {
        format!(r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\{t}")
    };
    let k = RegKey::predef(HKEY_CURRENT_USER).open_subkey(path).ok()?;
    [r"UserChoiceLatest\ProgId", "UserChoiceLatest", "UserChoice"]
        .iter()
        .find_map(|sub| k.open_subkey(sub).and_then(|s| s.get_value::<String, _>("ProgId")).ok().filter(|p| !p.is_empty()))
}

/// Windows Home editions (EditionID Core*) don't officially support the associations policy.
pub fn is_home() -> bool {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .and_then(|k| k.get_value::<String, _>("EditionID"))
        .map(|e| e.starts_with("Core"))
        .unwrap_or(false)
}

/// Path of an associations policy someone else (an admin, another tool) configured; we never overwrite it.
fn foreign_policy() -> Option<String> {
    let v = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(POLICY_KEY).and_then(|k| k.get_value::<String, _>(POLICY_VALUE)).ok()?;
    let ours = xml_path().display().to_string();
    (!v.trim().is_empty() && !v.trim().eq_ignore_ascii_case(&ours)).then_some(v)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn render_xml(version: u32, apps: &[(&str, &BTreeMap<String, String>)]) -> String {
    let mut rows: Vec<(String, String, String)> = apps
        .iter()
        .flat_map(|(app, ids)| ids.iter().map(move |(ext, prog)| (ext.clone(), prog.clone(), app.to_string())))
        .collect();
    rows.sort();
    let mut x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<DefaultAssociations Version=\"{version}\">\r\n");
    for (ext, prog, app) in rows {
        x.push_str(&format!(
            "  <Association Identifier=\"{}\" ProgId=\"{}\" ApplicationName=\"{}\" Suggested=\"true\" />\r\n",
            xml_escape(&ext),
            xml_escape(&prog),
            xml_escape(&app)
        ));
    }
    x.push_str("</DefaultAssociations>\r\n");
    x
}

/// Writes the XML for everything in the store and points the policy at it (or removes both when empty).
fn write_policy(s: &Store) -> Result<()> {
    let key = RegKey::predef(HKEY_LOCAL_MACHINE).create_subkey(POLICY_KEY)?.0;
    if s.assoc.is_empty() {
        let _ = key.delete_value(POLICY_VALUE);
        let _ = std::fs::remove_file(xml_path());
        return Ok(());
    }
    std::fs::create_dir_all(data_dir())?;
    let apps: Vec<_> = s.assoc.values().map(|a| (a.application.as_str(), &a.prog_ids)).collect();
    std::fs::write(xml_path(), render_xml(s.version, &apps))?;
    key.set_value(POLICY_VALUE, &xml_path().display().to_string())?;
    Ok(())
}

// ---------- apply / revert ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Applied {
    /// Windows applies it at the next sign-in.
    NextSignIn,
    /// Same, but Windows Home may ignore the policy — Settings is the fallback.
    NextSignInHome,
    /// Can't be automated here (another associations policy is configured) — use Settings.
    UseSettings,
}

fn defaults_of(item: &Item) -> Result<&Defaults> {
    item.defaults.as_ref().with_context(|| format!("{} has nothing to make default", item.name))
}

pub fn apply(item: &Item) -> Result<Applied> {
    let d = defaults_of(item)?;
    let reg = registered(&d.app).with_context(|| format!("{} is not installed (Windows has no default-apps registration for it)", item.name))?;
    let prog_ids: BTreeMap<String, String> = d.types.iter().filter_map(|t| reg.types.get(t).map(|p| (t.clone(), p.clone()))).collect();
    if prog_ids.is_empty() {
        bail!("{} registered none of the types to take over", item.name);
    }
    if let Some(other) = foreign_policy() {
        tracing::warn!("{}: another associations policy is configured ({other}); not overwriting it", item.id);
        return Ok(Applied::UseSettings);
    }
    let mut s = load();
    s.version += 1;
    s.written_at = now();
    s.assoc.insert(item.id.clone(), AppAssoc { application: reg.application, prog_ids });
    write_policy(&s)?;
    save(&s)?;
    tracing::info!("{}: default for {} types from the next sign-in (policy version {})", item.id, s.assoc[&item.id].prog_ids.len(), s.version);
    Ok(if is_home() { Applied::NextSignInHome } else { Applied::NextSignIn })
}

pub fn revert(item: &Item) -> Result<()> {
    let mut s = load();
    if s.assoc.remove(&item.id).is_none() {
        bail!("nothing to revert");
    }
    // Same Version: what remains was already applied once and shouldn't be pushed again.
    if foreign_policy().is_none() {
        write_policy(&s)?;
    }
    save(&s)?;
    tracing::info!("{}: removed from the default associations policy", item.id);
    Ok(())
}

/// The app's own page in Settings › Default apps (Windows 11 2023-04 update and later), else the list.
pub fn settings_uri(d: &Defaults, build: u32) -> String {
    match registered(&d.app) {
        Some(r) if build >= 22000 => {
            let param = if r.machine { "registeredAppMachine" } else { "registeredAppUser" };
            format!("ms-settings:defaultapps?{param}={}", d.app.replace(' ', "%20"))
        }
        _ => "ms-settings:defaultapps".into(),
    }
}

/// Runs right after a catalog install. Never fails the install: returns a message code for the card.
pub fn after_install(item: &Item) -> Option<String> {
    item.defaults.as_ref()?;
    let code = match apply(item) {
        Ok(Applied::NextSignIn) => "default-signin",
        Ok(Applied::NextSignInHome) => "default-signin-home",
        Ok(Applied::UseSettings) => "default-settings",
        Err(e) => {
            tracing::warn!("{}: could not set as default: {e:#}", item.id);
            "default-settings"
        }
    };
    Some(code.into())
}

// ---------- Print Screen ----------

/// For a screenshot app (Flameshot): free Print Screen from Snipping Tool (the revertable Tweaks
/// item), start the app with Windows the way it does itself (HKCU Run, value = its name) and start
/// it now as the signed-in user. Returns the message code for the app card.
pub async fn take_print_screen(item: &Item) -> String {
    let Some(exe) = item.detect.path.as_deref().map(util::expand_env) else { return "default-settings".into() };
    if let Err(e) = crate::tweaks::apply("print_screen").await {
        tracing::warn!("{}: could not free Print Screen: {e:#}", item.id);
        return "default-settings".into();
    }
    let run = RegKey::predef(HKEY_CURRENT_USER).create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run");
    if let Err(e) = run.and_then(|(k, _)| k.set_value(&item.name, &exe)) {
        tracing::warn!("{}: could not add to autostart: {e}", item.id);
    }
    if std::path::Path::new(&exe).is_file() {
        let name = std::path::Path::new(&exe).file_name().unwrap_or_default().to_string_lossy().to_string();
        let running = util::run("tasklist.exe", &["/FI", &format!("IMAGENAME eq {name}"), "/NH"]).await.map(|(_, o)| o.to_ascii_lowercase().contains(&name.to_ascii_lowercase())).unwrap_or(false);
        if !running {
            let r = crate::installer::run_unelevated(&format!("start \"\" \"{exe}\""), &std::sync::atomic::AtomicBool::new(false)).await;
            tracing::info!("{}: started {exe}: {r:?}", item.id);
        }
    } else {
        tracing::warn!("{}: {exe} not found after install", item.id);
    }
    "default-print-screen".into()
}

// ---------- state for the Tweaks page ----------

#[derive(Debug, Serialize)]
pub struct DefaultState {
    /// Catalog item id.
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub applied: bool,
    /// Written to the policy, waiting for the next sign-in.
    pub pending: bool,
    pub can_revert: bool,
    /// Types that open in the app now, and how many it should take over.
    pub ours: usize,
    pub total: usize,
    pub home: bool,
    pub settings_uri: String,
}

pub fn states(items: &[Item], build: u32) -> Vec<DefaultState> {
    let s = load();
    let (home, booted) = (is_home(), boot_time());
    let mut out = Vec::new();
    for it in items.iter().filter(|i| !i.custom) {
        let Some(d) = &it.defaults else { continue };
        let reg = registered(&d.app);
        let wanted: Vec<(&String, Option<&String>)> = match &reg {
            Some(r) => d.types.iter().filter_map(|t| r.types.get(t).map(|p| (t, Some(p)))).collect(),
            None => d.types.iter().map(|t| (t, None)).collect(),
        };
        let ours = wanted.iter().filter(|(t, p)| p.is_some() && current_prog_id(t).as_ref() == *p).count();
        let applied = reg.is_some() && !wanted.is_empty() && ours == wanted.len();
        let can_revert = s.assoc.contains_key(&it.id);
        out.push(DefaultState {
            id: it.id.clone(),
            name: it.name.clone(),
            installed: reg.is_some(),
            applied,
            pending: !applied && can_revert && s.written_at > booted,
            can_revert,
            ours,
            total: wanted.len(),
            home,
            settings_uri: settings_uri(d, build),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{parse, EMBEDDED};

    #[test]
    fn xml_is_suggested_and_sorted() {
        let vlc: BTreeMap<String, String> = [(".mp4", "VLC.mp4"), (".mkv", "VLC.mkv")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        let odd: BTreeMap<String, String> = [(".x", "A&B\"")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        let x = render_xml(3, &[("VLC media player", &vlc), ("<Odd>", &odd)]);
        assert!(x.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<DefaultAssociations Version=\"3\">"));
        let mkv = x.find("Identifier=\".mkv\"").unwrap();
        assert!(mkv < x.find("Identifier=\".mp4\"").unwrap());
        assert!(x.contains(r#"<Association Identifier=".mp4" ProgId="VLC.mp4" ApplicationName="VLC media player" Suggested="true" />"#));
        assert!(x.contains(r#"ProgId="A&amp;B&quot;" ApplicationName="&lt;Odd&gt;""#));
        assert!(x.trim_end().ends_with("</DefaultAssociations>"));
    }

    #[test]
    fn catalog_defaults() {
        let c = parse(EMBEDDED).unwrap();
        let get = |id: &str| c.items.iter().find(|i| i.id == id).unwrap().defaults.clone().unwrap();
        let vlc = get("vlc");
        assert_eq!(vlc.app, "VLC");
        for t in [".mp4", ".mkv", ".avi", ".mp3", ".flac", ".m3u8"] {
            assert!(vlc.types.iter().any(|x| x == t), "{t}");
        }
        // VLC registers these too, but they aren't a media player's to take.
        for t in [".iso", ".zip", ".rar", ".vlt", ".wsz"] {
            assert!(!vlc.types.iter().any(|x| x == t), "{t}");
        }
        let chrome = get("chrome");
        assert_eq!(chrome.app, "Google Chrome");
        for t in ["http", "https", ".htm", ".html"] {
            assert!(chrome.types.iter().any(|x| x == t), "{t}");
        }
        // PDFs go to PdfCraft, not the browser.
        assert!(!chrome.types.iter().any(|x| x == ".pdf"));
        assert_eq!(get("pdfcraft").types, vec![".pdf"]);
        // Every app with defaults says what it takes over, in both languages.
        for it in c.items.iter().filter(|i| i.defaults.is_some()) {
            let w = &it.defaults.as_ref().unwrap().what;
            assert!(!w.en.is_empty() && !w.ru.is_empty(), "{}", it.id);
        }
    }

    #[test]
    fn remote_catalog_cannot_hand_out_executables() {
        for bad in [r#"".exe""#, r#"".ps1""#, r#""mp4""#, r#"".lnk""#, r#""ftp""#, r#""ms-settings""#] {
            let evil = EMBEDDED.replacen(r#"".mp4""#, bad, 1);
            assert!(parse(&evil).is_err(), "{bad}");
        }
    }
}
