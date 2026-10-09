use anyhow::{anyhow, bail, Result};
use futures::StreamExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

#[derive(Debug)]
pub struct Cancelled;
impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("Cancelled")
    }
}
impl std::error::Error for Cancelled {}

/// Downloads `url` into `dir`, resuming `dir/download.part` with HTTP Range when present.
/// Retries 3 times with exponential back-off. Returns the final file path.
pub async fn fetch(
    client: &reqwest::Client,
    url: &str,
    dir: &Path,
    cancel: &AtomicBool,
    progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let mut last = anyhow!("no attempt");
    for attempt in 0..3u32 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
        }
        match once(client, url, dir, cancel, progress).await {
            Ok(p) => return Ok(p),
            Err(e) if e.is::<Cancelled>() => return Err(e),
            Err(e) => {
                tracing::warn!("download attempt {} of {url} failed: {e:#}", attempt + 1);
                last = e;
            }
        }
    }
    Err(last)
}

async fn once(
    client: &reqwest::Client,
    url: &str,
    dir: &Path,
    cancel: &AtomicBool,
    progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
) -> Result<PathBuf> {
    let part = dir.join("download.part");
    let have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    let mut req = client.get(url);
    if have > 0 {
        req = req.header("Range", format!("bytes={have}-"));
    }
    let resp = req.send().await?;
    let status = resp.status();
    if status.as_u16() == 416 {
        // Server says our partial is already complete or stale; start over.
        let _ = std::fs::remove_file(&part);
        bail!("range not satisfiable, restarting");
    }
    let resp = resp.error_for_status()?;
    let resumed = status.as_u16() == 206;
    let name = file_name(&resp);
    let total = resp.content_length().map(|l| if resumed { l + have } else { l });

    let mut f = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resumed)
        .truncate(!resumed)
        .open(&part)
        .await?;
    let mut done = if resumed { have } else { 0 };
    let mut stream = resp.bytes_stream();
    let mut tick = Instant::now();
    progress(done, total);
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Err(Cancelled.into());
        }
        let chunk = chunk?;
        f.write_all(&chunk).await?;
        done += chunk.len() as u64;
        if tick.elapsed() > Duration::from_millis(150) {
            progress(done, total);
            tick = Instant::now();
        }
    }
    f.flush().await?;
    drop(f);
    if let Some(t) = total {
        if done != t {
            bail!("incomplete download: {done} of {t} bytes");
        }
    }
    progress(done, total);
    let dest = dir.join(name);
    let _ = std::fs::remove_file(&dest);
    std::fs::rename(&part, &dest)?;
    Ok(dest)
}

/// Content-Disposition filename, else the last URL path segment, else "download.exe".
fn file_name(resp: &reqwest::Response) -> String {
    let cd = resp
        .headers()
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_disposition);
    let from_url = resp.url().path_segments().and_then(|s| s.last()).filter(|s| s.contains('.')).map(str::to_string);
    let raw = cd.or(from_url).unwrap_or_else(|| "download.exe".into());
    sanitize(&raw)
}

pub fn parse_disposition(v: &str) -> Option<String> {
    let re = regex::Regex::new(r#"(?i)filename\*?=(?:UTF-8'')?"?([^";]+)"?"#).unwrap();
    re.captures(v).map(|c| c[1].replace("%20", " "))
}

pub fn sanitize(name: &str) -> String {
    let n: String = name.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '_' } else { c }).collect();
    let n = n.trim_matches(|c| c == '.' || c == ' ').to_string();
    if n.is_empty() { "download.bin".into() } else { n }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disposition() {
        assert_eq!(parse_disposition(r#"attachment; filename="Adobe.Setup.exe""#).unwrap(), "Adobe.Setup.exe");
        assert_eq!(parse_disposition("attachment; filename*=UTF-8''my%20file.zip").unwrap(), "my file.zip");
        assert!(parse_disposition("inline").is_none());
    }

    #[test]
    fn sanitizes_paths() {
        assert_eq!(sanitize(r"..\..\evil.exe"), r"_.._evil.exe");
        assert_eq!(sanitize("a:b?.zip"), "a_b_.zip");
        assert_eq!(sanitize(".."), "download.bin");
    }
}
