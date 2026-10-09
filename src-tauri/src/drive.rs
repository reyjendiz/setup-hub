use crate::util;
use anyhow::{bail, Context, Result};
use futures::{stream, StreamExt};
use serde::Serialize;
use std::time::Duration;

pub const FOLDER_ID: &str = "1P_yy_Sw-Ca4m7sfqEmdLjpGeIPY5DZoU";
const HOSTS: [&str; 4] = ["drive.google.com", "drive.usercontent.google.com", "googleusercontent.com", "googleapis.com"];

pub fn hosts() -> Vec<String> {
    HOSTS.iter().map(|s| s.to_string()).collect()
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub is_folder: bool,
    pub size: Option<u64>,
    /// Folder path relative to the shared root, '/'-separated, "" for top level.
    pub path: String,
}

pub fn html_unescape(s: &str) -> String {
    let re = regex::Regex::new(r"&#(\d+);|&#x([0-9a-fA-F]+);").unwrap();
    let s = re.replace_all(s, |c: &regex::Captures| {
        let n = c.get(1).map(|m| m.as_str().parse().ok()).unwrap_or_else(|| u32::from_str_radix(&c[2], 16).ok());
        n.and_then(char::from_u32).map(String::from).unwrap_or_default()
    });
    s.replace("&quot;", "\"").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// Parses Google's embeddedfolderview HTML (no API key needed).
pub fn parse_embedded(html: &str, path: &str) -> Vec<Entry> {
    let re = regex::Regex::new(
        r#"(?s)<div class="flip-entry" id="entry-([\w-]+)".*?<a href="([^"]+)".*?/type/([^"]+)".*?<div class="flip-entry-title">([^<]*)</div>"#,
    )
    .unwrap();
    re.captures_iter(html)
        .map(|c| {
            let is_folder = c[2].contains("/folders/");
            Entry {
                id: c[1].to_string(),
                name: html_unescape(&c[4]),
                mime: if is_folder { "folder".into() } else { c[3].to_string() },
                is_folder,
                size: None,
                path: path.to_string(),
            }
        })
        .collect()
}

async fn list_embedded(c: &reqwest::Client, folder: &str, path: &str) -> Result<Vec<Entry>> {
    let html = c
        .get(format!("https://drive.google.com/embeddedfolderview?id={folder}"))
        .timeout(Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    if !html.contains("flip-entry") && !html.contains("folder-view") {
        bail!("Drive folder is not public or the listing page changed");
    }
    Ok(parse_embedded(&html, path))
}

async fn list_api(c: &reqwest::Client, folder: &str, path: &str, key: &str) -> Result<Vec<Entry>> {
    let mut out = Vec::new();
    let mut page = String::new();
    loop {
        let mut req = c.get("https://www.googleapis.com/drive/v3/files").query(&[
            ("q", format!("'{folder}' in parents and trashed=false")),
            ("fields", "nextPageToken,files(id,name,mimeType,size)".into()),
            ("pageSize", "1000".into()),
            ("key", key.into()),
        ]);
        if !page.is_empty() {
            req = req.query(&[("pageToken", &page)]);
        }
        let v: serde_json::Value = req.send().await?.error_for_status()?.json().await?;
        for f in v["files"].as_array().into_iter().flatten() {
            let mime = f["mimeType"].as_str().unwrap_or("").to_string();
            let is_folder = mime == "application/vnd.google-apps.folder";
            out.push(Entry {
                id: f["id"].as_str().unwrap_or("").into(),
                name: f["name"].as_str().unwrap_or("").into(),
                mime: if is_folder { "folder".into() } else { mime },
                is_folder,
                size: f["size"].as_str().and_then(|s| s.parse().ok()),
                path: path.into(),
            });
        }
        match v["nextPageToken"].as_str() {
            Some(t) => page = t.to_string(),
            None => break,
        }
    }
    Ok(out)
}

pub fn file_url(id: &str) -> String {
    format!("https://drive.usercontent.google.com/download?id={id}&export=download&confirm=t")
}

/// Size via a 1-byte range probe (embedded view doesn't show sizes).
async fn probe_size(c: &reqwest::Client, id: &str) -> Option<u64> {
    let r = c.get(file_url(id)).header("Range", "bytes=0-0").timeout(Duration::from_secs(15)).send().await.ok()?;
    r.headers().get("content-range")?.to_str().ok()?.rsplit('/').next()?.parse().ok()
}

/// Recursively lists the folder tree. API key path when available, HTML fallback otherwise.
pub async fn list(folder: &str, api_key: Option<&str>) -> Result<Vec<Entry>> {
    let c = util::http(hosts());
    let mut out = Vec::new();
    let mut queue = vec![(folder.to_string(), String::new())];
    while let Some((fid, path)) = queue.pop() {
        let entries = match api_key {
            Some(k) => list_api(&c, &fid, &path, k).await.context("Drive API listing failed (check the API key)")?,
            None => list_embedded(&c, &fid, &path).await?,
        };
        for e in &entries {
            if e.is_folder {
                let sub = if path.is_empty() { e.name.clone() } else { format!("{path}/{}", e.name) };
                queue.push((e.id.clone(), sub));
            }
        }
        out.extend(entries);
        if out.len() > 5000 {
            bail!("folder tree too large");
        }
    }
    if api_key.is_none() {
        let ids: Vec<String> = out.iter().filter(|e| !e.is_folder).map(|e| e.id.clone()).collect();
        let sizes: Vec<(String, Option<u64>)> = stream::iter(ids)
            .map(|id| {
                let c = c.clone();
                async move {
                    let s = probe_size(&c, &id).await;
                    (id, s)
                }
            })
            .buffer_unordered(8)
            .collect()
            .await;
        for (id, s) in sizes {
            if let Some(e) = out.iter_mut().find(|e| e.id == id) {
                e.size = s;
            }
        }
    }
    out.sort_by(|a, b| (&a.path, !a.is_folder, a.name.to_lowercase()).cmp(&(&b.path, !b.is_folder, b.name.to_lowercase())));
    Ok(out)
}

/// File name of a single shared file (from Content-Disposition of a 1-byte probe).
pub async fn file_name(id: &str) -> Result<String> {
    let r = util::http(hosts()).get(file_url(id)).header("Range", "bytes=0-0").timeout(Duration::from_secs(15)).send().await?;
    r.headers()
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .and_then(crate::download::parse_disposition)
        .context("this Drive file is not shared publicly")
}

/// When Drive serves the "can't scan for viruses" page instead of the file, rebuild the URL from its form.
pub fn interstitial_url(html: &str) -> Option<String> {
    let action = regex::Regex::new(r#"<form[^>]+id="download-form"[^>]+action="([^"]+)""#).unwrap().captures(html)?[1].to_string();
    let inputs = regex::Regex::new(r#"<input type="hidden" name="([^"]+)" value="([^"]*)""#).unwrap();
    let q: Vec<String> = inputs.captures_iter(html).map(|c| format!("{}={}", &c[1], html_unescape(&c[2]))).collect();
    Some(format!("{}?{}", html_unescape(&action), q.join("&")))
}

/// Returns the real file URL for `id`, following the large-file confirmation page if needed.
pub async fn resolve_download(id: &str) -> Result<String> {
    let c = util::http(hosts());
    let url = file_url(id);
    let r = c.get(&url).header("Range", "bytes=0-0").timeout(Duration::from_secs(20)).send().await?;
    let html = r.headers().get("content-type").and_then(|v| v.to_str().ok()).is_some_and(|t| t.starts_with("text/html"));
    if !html {
        return Ok(url);
    }
    let body = r.text().await?;
    interstitial_url(&body).context("Drive returned a page instead of the file (quota exceeded or not shared)")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<div class="flip-entry" id="entry-1sGXmEHYYOFpQGFKQlwsQgMrwPvYvevxx" tabindex="0" role="link"><div class="flip-entry-info"><a href="https://drive.google.com/file/d/1sGXmEHYYOFpQGFKQlwsQgMrwPvYvevxx/view?usp=drive_web" target="_blank"><div class="flip-entry-visual"><div class="flip-entry-visual-card"><div class="flip-entry-icon"><img src="https://drive-thirdparty.googleusercontent.com/128/type/application/x-msdownload" alt=""/></div></div></div><div class="flip-entry-list-icon"><img src="https://drive-thirdparty.googleusercontent.com/16/type/application/x-msdownload" alt=""/></div><div class="flip-entry-title">4K.Video.Downloader.exe</div></a></div><div class="flip-entry-last-modified"><div>Sep 29</div></div></div><div class="flip-entry" id="entry-1Fold-er_ID" tabindex="0" role="link"><div class="flip-entry-info"><a href="https://drive.google.com/drive/folders/1Fold-er_ID" target="_blank"><div class="flip-entry-visual"><div class="flip-entry-visual-card"><div class="flip-entry-icon"><img src="https://drive-thirdparty.googleusercontent.com/128/type/application/vnd.google-apps.folder" alt=""/></div></div></div><div class="flip-entry-title">Wizardon&#39;s Mods &amp; Packs</div></a></div></div>"#;

    #[test]
    fn parses_embedded_listing() {
        let e = parse_embedded(SAMPLE, "");
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].id, "1sGXmEHYYOFpQGFKQlwsQgMrwPvYvevxx");
        assert_eq!(e[0].name, "4K.Video.Downloader.exe");
        assert_eq!(e[0].mime, "application/x-msdownload");
        assert!(!e[0].is_folder);
        assert!(e[1].is_folder);
        assert_eq!(e[1].name, "Wizardon's Mods & Packs");
    }

    #[test]
    fn parses_virus_scan_interstitial() {
        let html = r#"<form id="download-form" action="https://drive.usercontent.google.com/download" method="get"><input type="hidden" name="id" value="ABC"><input type="hidden" name="export" value="download"><input type="hidden" name="confirm" value="t"><input type="hidden" name="uuid" value="1234-uuid"></form>"#;
        assert_eq!(
            interstitial_url(html).unwrap(),
            "https://drive.usercontent.google.com/download?id=ABC&export=download&confirm=t&uuid=1234-uuid"
        );
        assert!(interstitial_url("<html>no form</html>").is_none());
    }
}
