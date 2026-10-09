use crate::catalog::Item;
use crate::util;
use std::collections::HashMap;
use winreg::enums::*;
use winreg::RegKey;

/// (DisplayName, DisplayVersion) for every uninstall entry: HKLM 64/32-bit and HKCU.
pub fn uninstall_entries() -> Vec<(String, String)> {
    let mut v = Vec::new();
    let roots = [
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
        (HKEY_CURRENT_USER, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
    ];
    for (hive, path) in roots {
        let Ok(k) = RegKey::predef(hive).open_subkey(path) else { continue };
        for name in k.enum_keys().flatten() {
            if let Ok(sub) = k.open_subkey(&name) {
                if let Ok(dn) = sub.get_value::<String, _>("DisplayName") {
                    v.push((dn, sub.get_value("DisplayVersion").unwrap_or_default()));
                }
            }
        }
    }
    v
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

pub fn detect_one(it: &Item, entries: &[(String, String)], appx: &[String]) -> Option<String> {
    if let Some(re) = it.detect.display_name.as_deref().and_then(|r| regex::Regex::new(r).ok()) {
        if let Some((_, ver)) = entries.iter().find(|(n, _)| re.is_match(n)) {
            return Some(ver.clone());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{parse, EMBEDDED};

    #[test]
    fn detection_rules() {
        let c = parse(EMBEDDED).unwrap();
        let get = |id: &str| c.items.iter().find(|i| i.id == id).unwrap().clone();
        let entries = vec![
            ("Zoom Workplace".to_string(), "7.1.9".to_string()),
            ("Steam".into(), "2.10".into()),
            ("Steam Link".into(), "1.0".into()),
            ("Discord".into(), "1.0.9261".into()),
            ("GIGABYTE Control Center 25.07.02.01".into(), "25.07.02.01".into()),
        ];
        let appx = vec!["Claude_2.31226.0.0_x64__pzs8sxrjxfjjc".to_string()];
        assert_eq!(detect_one(&get("zoom"), &entries, &appx).unwrap(), "7.1.9");
        assert_eq!(detect_one(&get("steam"), &entries, &appx).unwrap(), "2.10");
        assert_eq!(detect_one(&get("gcc"), &entries, &appx).unwrap(), "25.07.02.01");
        assert_eq!(detect_one(&get("claude"), &entries, &appx).unwrap(), "2.31226.0.0");
        assert!(detect_one(&get("figma"), &entries, &appx).is_none());
    }
}
