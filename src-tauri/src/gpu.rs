use crate::util;
use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum Vendor {
    Nvidia,
    Amd,
    Intel,
    Other,
}

#[derive(Debug, Clone, Serialize)]
pub struct Gpu {
    pub name: String,
    pub vendor: Vendor,
    pub driver_version: String,
    /// NVIDIA-style "617.42" for NVIDIA cards.
    pub display_version: Option<String>,
    /// Windows runs it on its Basic Display Adapter (no vendor driver yet), so the model isn't known.
    pub driver_missing: bool,
}

pub fn vendor_from_pnp(pnp: &str) -> Vendor {
    let p = pnp.to_ascii_uppercase();
    if p.contains("VEN_10DE") {
        Vendor::Nvidia
    } else if p.contains("VEN_1002") {
        Vendor::Amd
    } else if p.contains("VEN_8086") {
        Vendor::Intel
    } else {
        Vendor::Other
    }
}

/// WMI "32.0.16.1742" → NVIDIA "617.42": last five digits of the last two fields.
pub fn nvidia_version(wmi: &str) -> Option<String> {
    let parts: Vec<&str> = wmi.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    let digits = format!("{}{}", parts[parts.len() - 2], parts[parts.len() - 1]);
    if digits.len() < 5 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let d = &digits[digits.len() - 5..];
    Some(format!("{}.{}", &d[..3], &d[3..]))
}

/// Builds the GPU list from (name, PnP device id, driver version) rows. A card still on Windows'
/// Basic Display Adapter is kept when its PCI vendor is known: a fresh Windows has no NVIDIA driver yet.
pub fn gpus_from(rows: Vec<(String, String, String)>) -> Vec<Gpu> {
    rows.into_iter()
        .filter_map(|(name, pnp, drv)| {
            let vendor = vendor_from_pnp(&pnp);
            let basic = name.contains("Microsoft Basic Display") || name.trim().is_empty();
            if basic && vendor == Vendor::Other {
                return None;
            }
            Some(Gpu {
                display_version: (vendor == Vendor::Nvidia && !basic).then(|| nvidia_version(&drv)).flatten(),
                name: if basic { String::new() } else { name },
                vendor,
                driver_version: drv,
                driver_missing: basic,
            })
        })
        .collect()
}

/// Graphics cards as Windows sees them: WMI first, the device registry if PowerShell/WMI is unavailable.
pub async fn detect() -> Result<Vec<Gpu>> {
    let wmi = util::ps_json(
        "@(Get-CimInstance Win32_VideoController | Select-Object Name,PNPDeviceID,DriverVersion)|ConvertTo-Json -Compress",
    )
    .await
    .map(|v| {
        let arr = if v.is_array() { v.as_array().unwrap().clone() } else { vec![v] };
        let s = |g: &serde_json::Value, k: &str| g[k].as_str().unwrap_or("").to_string();
        arr.iter().map(|g| (s(g, "Name"), s(g, "PNPDeviceID"), s(g, "DriverVersion"))).collect::<Vec<_>>()
    });
    let rows = match wmi {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("WMI GPU query failed ({e:#}); reading display devices from the registry");
            registry_rows()?
        }
    };
    Ok(gpus_from(rows))
}

const DISPLAY_CLASS: &str = "{4d36e968-e325-11ce-bfc1-08002be10318}";

/// "@oem12.inf,%nvidia_dev.2702%;NVIDIA GeForce RTX 4080" → "NVIDIA GeForce RTX 4080".
pub fn device_desc(s: &str) -> String {
    s.rsplit(';').next().unwrap_or(s).trim().to_string()
}

/// Display-class PCI devices from HKLM\SYSTEM\CurrentControlSet\Enum\PCI and their driver keys.
fn registry_rows() -> Result<Vec<(String, String, String)>> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    let hklm = winreg::RegKey::predef(HKEY_LOCAL_MACHINE);
    let pci = hklm.open_subkey(r"SYSTEM\CurrentControlSet\Enum\PCI").context("no PCI device list in the registry")?;
    let mut rows = Vec::new();
    for dev in pci.enum_keys().flatten() {
        let Ok(dk) = pci.open_subkey(&dev) else { continue };
        for inst in dk.enum_keys().flatten() {
            let Ok(k) = dk.open_subkey(&inst) else { continue };
            if !k.get_value::<String, _>("ClassGUID").is_ok_and(|g| g.eq_ignore_ascii_case(DISPLAY_CLASS)) {
                continue;
            }
            let name = k.get_value::<String, _>("FriendlyName").or_else(|_| k.get_value::<String, _>("DeviceDesc")).map(|d| device_desc(&d)).unwrap_or_default();
            let drv = k
                .get_value::<String, _>("Driver")
                .ok()
                .and_then(|d| hklm.open_subkey(format!(r"SYSTEM\CurrentControlSet\Control\Class\{d}")).ok())
                .and_then(|c| c.get_value::<String, _>("DriverVersion").ok())
                .unwrap_or_default();
            rows.push((name, format!(r"PCI\{dev}\{inst}"), drv));
        }
    }
    if rows.is_empty() {
        anyhow::bail!("no display adapters in the registry");
    }
    Ok(rows)
}

#[derive(Debug, Clone, Serialize)]
pub struct DriverInfo {
    pub version: String,
    pub release_date: String,
    pub url: String,
    pub size: String,
    pub name: String,
}

const NV_HOSTS: [&str; 2] = ["nvidia.com", "geforce.com"];

fn nv_client() -> reqwest::Client {
    util::http(NV_HOSTS.iter().map(|s| s.to_string()).collect())
}

fn norm(s: &str) -> String {
    s.to_lowercase().replace("nvidia", "").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Finds (psid, pfid) for a GPU name in NVIDIA's lookupValueSearch TypeID=3 XML.
pub fn find_product(xml: &str, gpu_name: &str) -> Option<(String, String)> {
    let re = regex::Regex::new(r#"<LookupValue ParentID="(\d+)"[^>]*>\s*<Name>([^<]+)</Name>\s*<Value>(\d+)</Value>"#).unwrap();
    let want = norm(gpu_name);
    let hit = re.captures_iter(xml).find(|c| norm(&c[2]) == want).map(|c| (c[1].to_string(), c[3].to_string()));
    hit
}

pub fn find_os(xml: &str, win11: bool) -> Option<String> {
    let want = if win11 { "Windows 11" } else { "Windows 10 64-bit" };
    let re = regex::Regex::new(r"<Name>([^<]+)</Name>\s*<Value>(\d+)</Value>").unwrap();
    let hit = re.captures_iter(xml).find(|c| c[1].trim() == want).map(|c| c[2].to_string());
    hit
}

fn urldecode(s: &str) -> String {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Picks the first WHQL Game Ready (non-Studio/CRD, non-beta) entry from a DriverManualLookup response.
pub fn pick_game_ready(json: &serde_json::Value) -> Option<DriverInfo> {
    json["IDS"].as_array()?.iter().map(|x| &x["downloadInfo"]).find_map(|d| {
        let name = urldecode(d["Name"].as_str()?);
        let ok = d["IsCRD"].as_str() == Some("0") && d["IsWHQL"].as_str() == Some("1") && d["IsBeta"].as_str() != Some("1")
            && name.contains("Game Ready");
        ok.then(|| DriverInfo {
            version: d["Version"].as_str().unwrap_or("").into(),
            release_date: d["ReleaseDateTime"].as_str().unwrap_or("").into(),
            url: d["DownloadURL"].as_str().unwrap_or("").into(),
            size: d["DownloadURLFileSize"].as_str().unwrap_or("").into(),
            name,
        })
    })
}

pub async fn windows_build() -> u32 {
    winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .and_then(|k| k.get_value::<String, _>("CurrentBuildNumber"))
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

pub async fn latest_nvidia(gpu_name: &str) -> Result<DriverInfo> {
    let c = nv_client();
    let get = |u: String| {
        let c = c.clone();
        async move { c.get(u).timeout(Duration::from_secs(20)).send().await?.error_for_status()?.text().await }
    };
    let products = get("https://www.nvidia.com/Download/API/lookupValueSearch.aspx?TypeID=3".into()).await?;
    let (psid, pfid) = find_product(&products, gpu_name).ok_or_else(|| anyhow!("NVIDIA has no product named '{gpu_name}'"))?;
    let oses = get("https://www.nvidia.com/Download/API/lookupValueSearch.aspx?TypeID=4".into()).await?;
    let os = find_os(&oses, windows_build().await >= 22000).context("OS id not found")?;
    let url = format!(
        "https://gfwsl.geforce.com/services_toolkit/services/com/nvidia/services/AjaxDriverService.php?func=DriverManualLookup&psid={psid}&pfid={pfid}&osID={os}&languageCode=1033&isWHQL=1&dch=1&upCRD=0&sort1=0&numberOfResults=5"
    );
    let json: serde_json::Value = serde_json::from_str(&get(url).await?)?;
    let d = pick_game_ready(&json).context("no Game Ready driver in NVIDIA's answer")?;
    if !d.url.starts_with("https://") || !util::host_allowed(reqwest::Url::parse(&d.url)?.host_str().unwrap_or(""), &nv_hosts()) {
        anyhow::bail!("unexpected driver URL {}", d.url);
    }
    Ok(d)
}

pub fn nv_hosts() -> Vec<String> {
    NV_HOSTS.iter().map(|s| s.to_string()).collect()
}

pub async fn nvidia_app_url() -> Result<String> {
    let body = nv_client()
        .get("https://www.nvidia.com/en-us/software/nvidia-app/")
        .timeout(Duration::from_secs(20))
        .send()
        .await?
        .text()
        .await?;
    regex::Regex::new(r"https://us\.download\.nvidia\.com/nvapp/client/[\d.]+/NVIDIA_app_v[\d.]+\.exe")
        .unwrap()
        .find(&body)
        .map(|m| m.as_str().to_string())
        .context("NVIDIA App link not found on nvidia.com")
}

/// Strict numeric compare of "617.42" style versions.
pub fn is_newer(latest: &str, installed: &str) -> bool {
    let p = |s: &str| s.split('.').map(|x| x.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>();
    p(latest) > p(installed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendors() {
        assert_eq!(vendor_from_pnp(r"PCI\VEN_10DE&DEV_2702&SUBSYS_51181462"), Vendor::Nvidia);
        assert_eq!(vendor_from_pnp(r"pci\ven_1002&dev_744c"), Vendor::Amd);
        assert_eq!(vendor_from_pnp(r"PCI\VEN_8086&DEV_A780"), Vendor::Intel);
        assert_eq!(vendor_from_pnp(r"ROOT\BasicDisplay"), Vendor::Other);
    }

    #[test]
    fn fresh_windows_keeps_nvidia_on_basic_display() {
        let row = |n: &str, p: &str, d: &str| (n.to_string(), p.to_string(), d.to_string());
        let g = gpus_from(vec![
            row("Microsoft Basic Display Adapter", r"PCI\VEN_10DE&DEV_2702&SUBSYS_51181462\4&1", "10.0.26100.1"),
            row("Microsoft Basic Display Adapter", r"ROOT\BasicDisplay\0000", "10.0.26100.1"),
            row("Intel(R) UHD Graphics 770", r"PCI\VEN_8086&DEV_A780", "31.0.101.5186"),
            row("NVIDIA GeForce RTX 4080 SUPER", r"PCI\VEN_10DE&DEV_2702", "32.0.16.1742"),
        ]);
        assert_eq!(g.len(), 3);
        assert!(g[0].driver_missing && g[0].vendor == Vendor::Nvidia && g[0].name.is_empty() && g[0].display_version.is_none());
        assert!(!g[1].driver_missing && g[1].vendor == Vendor::Intel);
        assert_eq!(g[2].display_version.as_deref(), Some("617.42"));
        assert_eq!(device_desc("@oem12.inf,%nvidia_dev.2702%;NVIDIA GeForce RTX 4080"), "NVIDIA GeForce RTX 4080");
        assert_eq!(device_desc("NVIDIA GeForce RTX 3060"), "NVIDIA GeForce RTX 3060");
    }

    #[test]
    fn nvidia_versions() {
        assert_eq!(nvidia_version("32.0.16.1742").unwrap(), "617.42");
        assert_eq!(nvidia_version("31.0.15.5222").unwrap(), "552.22");
        assert!(nvidia_version("bogus").is_none());
        assert!(is_newer("617.42", "552.22"));
        assert!(!is_newer("617.42", "617.42"));
        assert!(is_newer("1000.01", "999.99"));
    }

    #[test]
    fn product_lookup() {
        let xml = r#"<LookupValue ParentID="127"><Name>NVIDIA GeForce RTX 4080 SUPER</Name><Value>1041</Value></LookupValue>
<LookupValue ParentID="127"><Name>NVIDIA GeForce RTX 4080</Name><Value>999</Value></LookupValue>
<LookupValue ParentID="129"><Name>GeForce RTX 4080 Laptop GPU</Name><Value>1005</Value></LookupValue>"#;
        assert_eq!(find_product(xml, "NVIDIA GeForce RTX 4080 SUPER").unwrap(), ("127".into(), "1041".into()));
        assert_eq!(find_product(xml, "NVIDIA GeForce RTX 4080").unwrap(), ("127".into(), "999".into()));
        assert_eq!(find_product(xml, "NVIDIA GeForce RTX 4080 Laptop GPU").unwrap(), ("129".into(), "1005".into()));
        let os = "<LookupValue><Name>Windows 10 64-bit</Name><Value>57</Value></LookupValue><LookupValue><Name>Windows 11</Name><Value>135</Value></LookupValue>";
        assert_eq!(find_os(os, true).unwrap(), "135");
        assert_eq!(find_os(os, false).unwrap(), "57");
    }

    #[test]
    fn game_ready_pick_skips_studio() {
        let j = serde_json::json!({"IDS": [
            {"downloadInfo": {"Name": "NVIDIA%20Studio%20Driver", "IsCRD": "1", "IsWHQL": "1", "Version": "616.00"}},
            {"downloadInfo": {"Name": "GeForce%20Game%20Ready%20Driver", "IsCRD": "0", "IsWHQL": "1", "IsBeta": "0", "Version": "617.42",
              "DownloadURL": "https://us.download.nvidia.com/Windows/617.42/x.exe", "ReleaseDateTime": "Tue Oct 06, 2026", "DownloadURLFileSize": "990.85 MB"}}
        ]});
        let d = pick_game_ready(&j).unwrap();
        assert_eq!(d.version, "617.42");
        assert_eq!(d.name, "GeForce Game Ready Driver");
    }
}
