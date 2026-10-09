//! Read-only licensing status plus "use my own key". Status comes from the same WMI class slmgr.vbs reads
//! (locale independent); the key path shells out to slmgr. Keys are validated, never stored or logged.

use crate::util;
use anyhow::{bail, Result};
use serde::Serialize;

#[derive(Serialize)]
pub struct License {
    pub name: String,
    pub description: String,
    pub status: u32,
    pub partial_key: String,
    pub grace_minutes: u64,
}

pub async fn status() -> Result<Vec<License>> {
    let v = util::ps_json(
        "@(Get-CimInstance SoftwareLicensingProduct -Filter \"ApplicationID='55c92734-d682-4d71-983e-d6ec3f16059f' AND PartialProductKey IS NOT NULL\" | Select-Object Name,Description,LicenseStatus,PartialProductKey,GracePeriodRemaining)|ConvertTo-Json -Compress",
    )
    .await?;
    let arr = if v.is_array() { v.as_array().unwrap().clone() } else { vec![v] };
    Ok(arr
        .iter()
        .map(|l| License {
            name: l["Name"].as_str().unwrap_or("").into(),
            description: l["Description"].as_str().unwrap_or("").into(),
            status: l["LicenseStatus"].as_u64().unwrap_or(0) as u32,
            partial_key: l["PartialProductKey"].as_str().unwrap_or("").into(),
            grace_minutes: l["GracePeriodRemaining"].as_u64().unwrap_or(0),
        })
        .collect())
}

pub fn valid_key(k: &str) -> bool {
    regex::Regex::new(r"^[A-Z0-9]{5}(-[A-Z0-9]{5}){4}$").unwrap().is_match(k)
}

async fn slmgr(args: &[&str]) -> Result<String> {
    let script = util::expand_env(r"%windir%\System32\slmgr.vbs");
    let mut a = vec!["//nologo", "//U", script.as_str()];
    a.extend_from_slice(args);
    let out = util::tokio_cmd("cscript.exe").args(&a).output().await?;
    // //U makes cscript write UTF-16LE, so Russian output survives.
    let wide: Vec<u16> = out.stdout.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    Ok(String::from_utf16_lossy(&wide).trim().to_string())
}

pub async fn use_key(key: &str) -> Result<String> {
    let key = key.trim().to_uppercase();
    if !valid_key(&key) {
        bail!("Key must look like XXXXX-XXXXX-XXXXX-XXXXX-XXXXX");
    }
    tracing::info!("activation: installing a user-supplied product key"); // the key itself is never logged
    let a = slmgr(&["/ipk", &key]).await?;
    let b = slmgr(&["/ato"]).await?;
    Ok(format!("{a}\n\n{b}"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn key_format() {
        assert!(super::valid_key("ABCDE-12345-FGHIJ-67890-KLMNO"));
        assert!(!super::valid_key("ABCDE-12345"));
        assert!(!super::valid_key("ABCDE-12345-FGHIJ-67890-KLMNO & calc"));
    }
}
