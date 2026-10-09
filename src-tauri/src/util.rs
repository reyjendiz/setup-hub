use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;

pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;
pub const UA: &str = concat!("SetupHub/", env!("CARGO_PKG_VERSION"), " (+https://github.com/sevcenkoa864-oss)");

pub fn app_dir() -> PathBuf {
    let d = PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into())).join("SetupHub");
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn cache_dir() -> PathBuf {
    let d = std::env::temp_dir().join("SetupHub").join("cache");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Expands %VAR% tokens using the current environment; unknown vars are left as-is.
pub fn expand_env(s: &str) -> String {
    let re = regex::Regex::new(r"%([A-Za-z0-9_()]+)%").unwrap();
    re.replace_all(s, |c: &regex::Captures| std::env::var(&c[1]).unwrap_or_else(|_| c[0].to_string()))
        .into_owned()
}

/// Suffix match: "github.com" allows "github.com" and "api.github.com", not "evilgithub.com".
pub fn host_allowed(host: &str, allowed: &[String]) -> bool {
    let host = host.to_ascii_lowercase();
    allowed.iter().any(|a| {
        let a = a.to_ascii_lowercase();
        host == a || host.ends_with(&format!(".{a}"))
    })
}

pub fn tokio_cmd(program: &str) -> tokio::process::Command {
    let mut c = tokio::process::Command::new(program);
    c.creation_flags(CREATE_NO_WINDOW);
    c.kill_on_drop(true);
    c
}

/// Runs a PowerShell snippet and parses its JSON output (the snippet must end in ConvertTo-Json).
pub async fn ps_json(script: &str) -> Result<serde_json::Value> {
    let full = format!("$ProgressPreference='SilentlyContinue';[Console]::OutputEncoding=[Text.Encoding]::UTF8;{script}");
    let out = tokio_cmd("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &full])
        .output()
        .await
        .context("powershell failed to start")?;
    let text = String::from_utf8_lossy(&out.stdout);
    let text = text.trim();
    if text.is_empty() {
        return Err(anyhow!("powershell returned nothing: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    serde_json::from_str(text).with_context(|| format!("bad powershell json: {text}"))
}

/// Console tools (powercfg, schtasks…) write in the OEM code page (cp866 on Russian Windows), not UTF-8.
pub fn decode_console(b: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(b) {
        return s.to_string();
    }
    use windows_sys::Win32::Globalization::{MultiByteToWideChar, CP_OEMCP};
    // SAFETY: first call sizes the buffer, second fills exactly that many UTF-16 units.
    unsafe {
        let n = MultiByteToWideChar(CP_OEMCP, 0, b.as_ptr(), b.len() as i32, std::ptr::null_mut(), 0);
        let mut w = vec![0u16; n.max(0) as usize];
        MultiByteToWideChar(CP_OEMCP, 0, b.as_ptr(), b.len() as i32, w.as_mut_ptr(), n);
        String::from_utf16_lossy(&w)
    }
}

/// Runs a program hidden, returns (exit code, stdout+stderr).
pub async fn run(program: &str, args: &[&str]) -> Result<(i32, String)> {
    let out = tokio_cmd(program).args(args).output().await.with_context(|| format!("{program} failed to start"))?;
    let mut s = decode_console(&out.stdout);
    s.push_str(&decode_console(&out.stderr));
    Ok((out.status.code().unwrap_or(-1), s))
}

pub fn http(allowed: Vec<String>) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(UA)
        .connect_timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::custom(move |a| {
            let ok = a.url().scheme() == "https" && a.url().host_str().is_some_and(|h| host_allowed(h, &allowed));
            if !ok {
                let msg = format!("redirect to non-allowed host: {}", a.url());
                a.error(msg)
            } else if a.previous().len() > 10 {
                a.error("too many redirects")
            } else {
                a.follow()
            }
        }))
        .build()
        .expect("http client")
}

/// Creates a .lnk via WScript.Shell (no extra crate for one COM call).
pub async fn create_shortcut(lnk: &std::path::Path, target: &std::path::Path, args: &str) -> Result<()> {
    if let Some(p) = lnk.parent() {
        std::fs::create_dir_all(p)?;
    }
    let q = |p: &std::path::Path| p.display().to_string().replace('\'', "''");
    let script = format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');$s.TargetPath='{}';$s.Arguments='{}';$s.WorkingDirectory='{}';$s.Save();ConvertTo-Json $true",
        q(lnk), q(target), args.replace('\'', "''"),
        q(target.parent().unwrap_or(target))
    );
    ps_json(&script).await.map(|_| ())
}

pub fn desktop_dir() -> PathBuf {
    PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default()).join("Desktop")
}

pub fn start_menu_dir() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join(r"Microsoft\Windows\Start Menu\Programs")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_suffix_match() {
        let a = vec!["github.com".to_string(), "githubusercontent.com".into()];
        assert!(host_allowed("github.com", &a));
        assert!(host_allowed("release-assets.githubusercontent.com", &a));
        assert!(host_allowed("API.GitHub.com", &a));
        assert!(!host_allowed("evilgithub.com", &a));
        assert!(!host_allowed("github.com.evil.io", &a));
    }

    #[test]
    fn env_expansion() {
        std::env::set_var("SH_TEST_VAR", r"C:\X");
        assert_eq!(expand_env(r"%SH_TEST_VAR%\Tools"), r"C:\X\Tools");
        assert_eq!(expand_env("%NOPE_NOT_SET%"), "%NOPE_NOT_SET%");
    }
}
