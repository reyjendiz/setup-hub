use crate::catalog::{Detect, Item};
use crate::util;
use std::collections::HashMap;
use winreg::enums::*;
use winreg::{RegKey, HKEY};

#[derive(Debug, Clone, Default)]
pub struct UninstallEntry {
    /// "HKLM\SOFTWARE\…\Uninstall\<sub>" — stable identity for My Apps detection.
    pub key: String,
    pub name: String,
    pub version: String,
    pub uninstall: String,
    pub quiet_uninstall: String,
    /// InstallLocation, else the folder of DisplayIcon ("" if neither is recorded).
    pub location: String,
}

const ROOTS: [(HKEY, &str, &str); 3] = [
    (HKEY_LOCAL_MACHINE, "HKLM", r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
    (HKEY_LOCAL_MACHINE, "HKLM", r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
    (HKEY_CURRENT_USER, "HKCU", r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
];

/// Every uninstall entry that has a DisplayName: HKLM 64/32-bit and HKCU.
pub fn uninstall_entries() -> Vec<UninstallEntry> {
    let mut v = Vec::new();
    for (hive, label, path) in ROOTS {
        let Ok(k) = RegKey::predef(hive).open_subkey(path) else { continue };
        for name in k.enum_keys().flatten() {
            if let Ok(sub) = k.open_subkey(&name) {
                if let Ok(dn) = sub.get_value::<String, _>("DisplayName") {
                    v.push(UninstallEntry {
                        key: format!(r"{label}\{path}\{name}"),
                        name: dn,
                        version: sub.get_value("DisplayVersion").unwrap_or_default(),
                        uninstall: sub.get_value("UninstallString").unwrap_or_default(),
                        quiet_uninstall: sub.get_value("QuietUninstallString").unwrap_or_default(),
                        location: install_location(
                            &sub.get_value::<String, _>("InstallLocation").unwrap_or_default(),
                            &sub.get_value::<String, _>("DisplayIcon").unwrap_or_default(),
                        ),
                    });
                }
            }
        }
    }
    v
}

/// `C:\App\` → `C:\App`; without one, `"C:\App\app.exe",0` → `C:\App`.
pub fn install_location(location: &str, icon: &str) -> String {
    let loc = location.trim().trim_matches('"').trim_end_matches('\\');
    if !loc.is_empty() {
        return loc.to_string();
    }
    let icon = icon.split(',').next().unwrap_or("").trim().trim_matches('"');
    icon.rsplit_once('\\').map(|(dir, _)| dir.to_string()).unwrap_or_default()
}

/// Installed MSIX package full names for the current user (e.g. "40174MouriNaruto.NanaZip_7.0.1843.0_x64__gnj4mf6z9tkrc").
pub fn appx_packages() -> Vec<String> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages")
        .map(|k| k.enum_keys().flatten().collect())
        .unwrap_or_default()
}

/// Returns installed version ("" if unknown) for each detected item.
pub fn detect_all(items: &[Item]) -> HashMap<String, String> {
    let entries = uninstall_entries();
    let appx = appx_packages();
    let mut out = HashMap::new();
    for it in items {
        if let Some(v) = detect_one(it, &entries, &appx) {
            out.insert(it.id.clone(), v);
        }
    }
    out
}

pub fn detect_one(it: &Item, entries: &[UninstallEntry], appx: &[String]) -> Option<String> {
    if let Some(k) = &it.detect.uninstall_key {
        if let Some(e) = entries.iter().find(|e| e.key.eq_ignore_ascii_case(k)) {
            return Some(e.version.clone());
        }
    }
    if let Some(re) = it.detect.display_name.as_deref().and_then(|r| regex::Regex::new(r).ok()) {
        if let Some(e) = entries.iter().find(|e| re.is_match(&e.name)) {
            return Some(e.version.clone());
        }
    }
    if let Some(prefix) = &it.detect.appx {
        if let Some(full) = appx.iter().find(|p| p.starts_with(prefix.as_str())) {
            return Some(full.split('_').nth(1).unwrap_or("").to_string());
        }
    }
    if let Some(p) = &it.detect.path {
        if std::path::Path::new(&util::expand_env(p)).exists() {
            return Some(String::new());
        }
    }
    None
}

pub struct Snapshot {
    keys: Vec<String>,
    appx: Vec<String>,
}

pub fn snapshot() -> Snapshot {
    Snapshot { keys: uninstall_entries().into_iter().map(|e| e.key).collect(), appx: appx_packages() }
}

/// What a single install added: the first new uninstall key, else the new MSIX package name.
pub fn new_since(before: &Snapshot, entries: &[UninstallEntry], appx: &[String]) -> Option<Detect> {
    if let Some(e) = entries.iter().find(|e| !before.keys.iter().any(|k| k.eq_ignore_ascii_case(&e.key))) {
        return Some(Detect { uninstall_key: Some(e.key.clone()), ..Default::default() });
    }
    appx.iter()
        .find(|p| !before.appx.contains(p))
        .map(|p| Detect { appx: Some(format!("{}_", p.split('_').next().unwrap_or(p))), ..Default::default() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{parse, EMBEDDED};

    fn e(name: &str, ver: &str) -> UninstallEntry {
        UninstallEntry { key: format!(r"HKLM\X\{name}"), name: name.into(), version: ver.into(), ..Default::default() }
    }

    #[test]
    fn detection_rules() {
        let c = parse(EMBEDDED).unwrap();
        let get = |id: &str| c.items.iter().find(|i| i.id == id).unwrap().clone();
        let entries = vec![
            e("Zoom Workplace", "7.1.9"),
            e("Steam", "2.10"),
            e("Steam Link", "1.0"),
            e("Discord", "1.0.9261"),
            e("GIGABYTE Control Center 25.07.02.01", "25.07.02.01"),
        ];
        let appx = vec!["Claude_2.31226.0.0_x64__pzs8sxrjxfjjc".to_string()];
        assert_eq!(detect_one(&get("zoom"), &entries, &appx).unwrap(), "7.1.9");
        assert_eq!(detect_one(&get("steam"), &entries, &appx).unwrap(), "2.10");
        assert_eq!(detect_one(&get("gcc"), &entries, &appx).unwrap(), "25.07.02.01");
        assert_eq!(detect_one(&get("claude"), &entries, &appx).unwrap(), "2.31226.0.0");
        assert!(detect_one(&get("figma"), &entries, &appx).is_none());

        let mut by_key = get("figma");
        by_key.detect = Detect { uninstall_key: Some(r"hklm\x\Discord".into()), ..Default::default() };
        assert_eq!(detect_one(&by_key, &entries, &appx).unwrap(), "1.0.9261");
    }

    #[test]
    fn install_folder() {
        assert_eq!(install_location(r"C:\Program Files (x86)\Skillbrains\lightshot\", ""), r"C:\Program Files (x86)\Skillbrains\lightshot");
        assert_eq!(install_location("", r#""C:\Program Files\VideoLAN\VLC\vlc.exe",0"#), r"C:\Program Files\VideoLAN\VLC");
        assert_eq!(install_location("", ""), "");
    }

    #[test]
    fn snapshot_diff() {
        let before = Snapshot { keys: vec![r"HKLM\X\Steam".into()], appx: vec!["A_1_x64__h".into()] };
        let now = vec![e("Steam", "1"), e("NewApp", "2")];
        assert_eq!(new_since(&before, &now, &[]).unwrap().uninstall_key.unwrap(), r"HKLM\X\NewApp");
        let appx = vec!["A_1_x64__h".to_string(), "40174MouriNaruto.NanaZip_7_x64__g".to_string()];
        assert_eq!(new_since(&before, &[e("Steam", "1")], &appx).unwrap().appx.unwrap(), "40174MouriNaruto.NanaZip_");
        assert!(new_since(&before, &[e("Steam", "1")], &["A_1_x64__h".into()]).is_none());
    }
}
