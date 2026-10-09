use crate::util;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub lang: String,
    pub theme: String,
    pub drive_dest: String,
    pub keep_installers: bool,
    pub parallel_downloads: u8,
    pub catalog_url: String,
    pub clean_driver_install: bool,
    pub allow_http: bool,
    /// Where My Apps is restored from after a reinstall (raw GitHub, Drive file link or any HTTPS URL).
    pub my_apps_url: String,
    /// On launch, look for a newer Setup Hub and for app updates.
    pub check_updates: bool,
    /// Public Google Drive folders added on the Files page.
    pub drive_folders: Vec<crate::drive::Folder>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            lang: "en".into(),
            theme: "system".into(),
            drive_dest: format!("{}\\Downloads\\SetupHub-Drive", std::env::var("USERPROFILE").unwrap_or_default()),
            keep_installers: false,
            parallel_downloads: 3,
            catalog_url: String::new(),
            clean_driver_install: true,
            allow_http: false,
            my_apps_url: String::new(),
            check_updates: true,
            drive_folders: Vec::new(),
        }
    }
}

fn path() -> std::path::PathBuf {
    util::app_dir().join("settings.json")
}

pub fn load() -> Settings {
    std::fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save(s: &Settings) -> anyhow::Result<()> {
    std::fs::write(path(), serde_json::to_string_pretty(s)?)?;
    Ok(())
}

// Secrets live in Windows Credential Manager, never in settings.json or logs.
const SECRETS: [&str; 2] = ["github_token", "google_api_key"];

fn entry(name: &str) -> anyhow::Result<keyring::Entry> {
    anyhow::ensure!(SECRETS.contains(&name), "unknown secret");
    Ok(keyring::Entry::new("SetupHub", name)?)
}

pub fn get_secret(name: &str) -> Option<String> {
    entry(name).ok()?.get_password().ok().filter(|s| !s.is_empty())
}

pub fn set_secret(name: &str, value: &str) -> anyhow::Result<()> {
    let e = entry(name)?;
    if value.trim().is_empty() {
        let _ = e.delete_credential();
        Ok(())
    } else {
        Ok(e.set_password(value.trim())?)
    }
}
