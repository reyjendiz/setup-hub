//! Pure helpers (no I/O), unit-tested on any platform.

use serde::Deserialize;

pub const INSTALLER_RELEASE: &str = "https://api.github.com/repos/Vencord/Installer/releases/latest";
pub const CLI_ASSET: &str = "VencordInstallerCli.exe";

/// Hosts the installer CLI may be downloaded from (GitHub's API and its release-asset CDN).
pub fn host_allowed(host: &str) -> bool {
    let h = host.to_ascii_lowercase();
    ["github.com", "githubusercontent.com"].iter().any(|a| h == *a || h.ends_with(&format!(".{a}")))
}

#[derive(Deserialize, Debug)]
pub struct Release {
    pub tag_name: String,
    pub assets: Vec<Asset>,
}

#[derive(Deserialize, Debug)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub digest: Option<String>,
}

/// The CLI asset's download URL and its SHA-256 (lowercase hex) as GitHub recorded it.
pub fn cli_asset(rel: &Release) -> Result<(String, String), String> {
    let a = rel.assets.iter().find(|a| a.name == CLI_ASSET).ok_or_else(|| format!("release {} has no {CLI_ASSET}", rel.tag_name))?;
    let sha = a
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .filter(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| format!("release {} publishes no SHA-256 for {CLI_ASSET}", rel.tag_name))?;
    Ok((a.browser_download_url.clone(), sha.to_ascii_lowercase()))
}

/// Task Manager's "Disabled" state for a Run entry (Explorer\StartupApproved\Run): 03 + 11 zero bytes.
#[cfg_attr(not(windows), allow(dead_code))]
pub const STARTUP_DISABLED: [u8; 12] = [3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// Arguments for Discord's Squirrel launcher; at sign-in Discord starts to the tray like its own autostart.
pub fn discord_args(startup: bool) -> Vec<&'static str> {
    let mut a = vec!["--processStart", "Discord.exe"];
    if startup {
        a.extend(["--process-start-args", "--start-minimized"]);
    }
    a
}

/// Unix seconds → "2026-10-09 17:52:34 UTC" (civil-from-days, no time-zone database needed).
pub fn utc_stamp(secs: u64) -> String {
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Keeps the last `keep` bytes of a log, starting at a line boundary.
pub fn trim_log(text: &str, keep: usize) -> &str {
    if text.len() <= keep {
        return text;
    }
    let mut start = text.len() - keep;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    match text[start..].find('\n') {
        Some(i) => &text[start + i + 1..],
        None => &text[start..],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts() {
        assert!(host_allowed("api.github.com"));
        assert!(host_allowed("release-assets.githubusercontent.com"));
        assert!(host_allowed("GitHub.com"));
        assert!(!host_allowed("evilgithub.com"));
        assert!(!host_allowed("github.com.evil.io"));
    }

    #[test]
    fn release_asset() {
        let rel: Release = serde_json::from_str(
            r#"{"tag_name":"v1.4.0","assets":[
                {"name":"VencordInstaller.exe","browser_download_url":"https://github.com/x/VencordInstaller.exe","digest":"sha256:aa"},
                {"name":"VencordInstallerCli.exe","browser_download_url":"https://github.com/x/VencordInstallerCli.exe",
                 "digest":"sha256:ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789"}]}"#,
        )
        .unwrap();
        let (url, sha) = cli_asset(&rel).unwrap();
        assert_eq!(url, "https://github.com/x/VencordInstallerCli.exe");
        assert_eq!(sha, "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789");

        let no_digest: Release = serde_json::from_str(r#"{"tag_name":"v1","assets":[{"name":"VencordInstallerCli.exe","browser_download_url":"u"}]}"#).unwrap();
        assert!(cli_asset(&no_digest).unwrap_err().contains("no SHA-256"));
        let missing: Release = serde_json::from_str(r#"{"tag_name":"v1","assets":[]}"#).unwrap();
        assert!(cli_asset(&missing).is_err());
    }

    #[test]
    fn launch_args() {
        assert_eq!(discord_args(false), vec!["--processStart", "Discord.exe"]);
        assert_eq!(discord_args(true), vec!["--processStart", "Discord.exe", "--process-start-args", "--start-minimized"]);
    }

    #[test]
    fn stamps() {
        assert_eq!(utc_stamp(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(utc_stamp(1_791_570_944), "2026-10-09 18:35:44 UTC");
        assert_eq!(utc_stamp(951_782_400), "2000-02-29 00:00:00 UTC");
    }

    #[test]
    fn log_trimming() {
        assert_eq!(trim_log("a\nb\n", 100), "a\nb\n");
        assert_eq!(trim_log("line1\nline2\nline3\n", 9), "line3\n");
        assert_eq!(trim_log("ééé\nx\n", 4), "x\n");
    }
}
