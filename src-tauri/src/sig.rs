use crate::util;
use anyhow::{bail, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct Signature {
    pub status: String,
    pub publisher: Option<String>,
}

/// Authenticode check via Get-AuthenticodeSignature (which calls WinVerifyTrust).
pub async fn authenticode(path: &Path) -> Result<Signature> {
    let p = path.display().to_string().replace('\'', "''");
    let v = util::ps_json(&format!(
        "$s=Get-AuthenticodeSignature -LiteralPath '{p}';@{{status=[string]$s.Status;subject=if($s.SignerCertificate){{$s.SignerCertificate.GetNameInfo('SimpleName',$false)}}else{{$null}}}}|ConvertTo-Json -Compress"
    ))
    .await?;
    Ok(Signature {
        status: v["status"].as_str().unwrap_or("Unknown").to_string(),
        publisher: v["subject"].as_str().map(str::to_string),
    })
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// Policy: a published SHA-256 must match; a signature, if present, must be Valid and match the
/// expected publisher; an unsigned file is only accepted when its hash was verified.
pub async fn verify(path: &Path, sha256: Option<&str>, publisher: Option<&str>) -> Result<String> {
    verify_with(path, sha256, publisher, false).await
}

/// `allow_unsigned`: only for My Apps entries where the user explicitly confirmed "install anyway".
/// A present-but-invalid signature or a hash mismatch still fails.
pub async fn verify_with(path: &Path, sha256: Option<&str>, publisher: Option<&str>, allow_unsigned: bool) -> Result<String> {
    let mut log = Vec::new();
    let hash_ok = match sha256 {
        Some(want) => {
            let got = sha256_file(path)?;
            if !got.eq_ignore_ascii_case(want) {
                bail!("SHA-256 mismatch: expected {want}, got {got}");
            }
            log.push("sha256 ok".to_string());
            true
        }
        None => false,
    };
    let sig = authenticode(path).await?;
    match sig.status.as_str() {
        "Valid" => {
            let who = sig.publisher.clone().unwrap_or_default();
            if let Some(want) = publisher {
                if !who.to_lowercase().contains(&want.to_lowercase()) {
                    bail!("signed by '{who}', expected '{want}'");
                }
            }
            log.push(format!("signed by {who}"));
        }
        "NotSigned" if hash_ok => log.push("unsigned (hash verified)".into()),
        "NotSigned" if allow_unsigned => log.push("unsigned (installed anyway, confirmed by the user)".into()),
        "NotSigned" => bail!("installer is not signed and no published hash exists — refusing to run it"),
        other => bail!("signature check failed: {other}"),
    }
    Ok(log.join(", "))
}
