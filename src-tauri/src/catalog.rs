//! The catalog lists every title the launcher can install.
//! Source order: ROSACE_CATALOG env var (URL or file path), REMOTE_CATALOG_URL, bundled catalog.json.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Set this to a raw GitHub URL (e.g. a `catalog.json` in a public repo) to add titles
/// without shipping a new launcher. `None` = use the catalog bundled at build time.
pub const REMOTE_CATALOG_URL: Option<&str> = Some("https://raw.githubusercontent.com/RosacePony/RosacesLogicLauncher/main/catalog.json");

const BUNDLED: &str = include_str!("../../catalog.json");

pub const PLATFORM: &str = if cfg!(windows) {
    "windows"
} else if cfg!(target_os = "linux") {
    "linux"
} else {
    "macos"
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Catalog {
    pub titles: Vec<Title>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Title {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// GitHub "owner/repo" whose latest release is installed (empty for store titles).
    #[serde(default)]
    pub repo: String,
    /// Banner image: a path inside the launcher (`covers/x.jpg`) or an https URL.
    #[serde(default)]
    pub cover: Option<String>,
    /// Store page (e.g. Steam): the launcher links there instead of installing.
    #[serde(default)]
    pub store_url: Option<String>,
    #[serde(default)]
    pub platforms: HashMap<String, PlatformBuild>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlatformBuild {
    /// Regex matched against release asset file names.
    pub asset: String,
    /// Path of the program to run, relative to the install folder.
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
}

impl Title {
    pub fn build(&self) -> Option<&PlatformBuild> {
        self.platforms.get(PLATFORM)
    }
}

pub fn parse(text: &str) -> Result<Catalog, String> {
    let catalog: Catalog = serde_json::from_str(text).map_err(|e| format!("Invalid catalog: {e}"))?;
    for t in &catalog.titles {
        let ok = !t.id.is_empty()
            && t.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
        if !ok {
            return Err(format!("Invalid title id '{}': use a-z, 0-9, '-' or '_'", t.id));
        }
        if let Some(url) = &t.store_url {
            if !url.starts_with("https://") {
                return Err(format!("Store link of '{}' must be an https URL", t.id));
            }
            continue;
        }
        if t.repo.split('/').count() != 2 {
            return Err(format!("Invalid repo '{}' for '{}': expected owner/name", t.repo, t.id));
        }
        for (platform, build) in &t.platforms {
            regex::Regex::new(&build.asset)
                .map_err(|e| format!("Bad asset regex for {}/{platform}: {e}", t.id))?;
            if build.executable.contains("..") {
                return Err(format!("Executable path of {}/{platform} must stay inside the install folder", t.id));
            }
        }
    }
    Ok(catalog)
}

async fn fetch(client: &reqwest::Client, url: &str) -> Result<Catalog, String> {
    let text = client
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Could not download catalog: {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    parse(&text)
}

pub async fn load(client: &reqwest::Client) -> Result<Catalog, String> {
    if let Ok(src) = std::env::var("ROSACE_CATALOG") {
        return if src.starts_with("http://") || src.starts_with("https://") {
            fetch(client, &src).await
        } else {
            parse(&std::fs::read_to_string(&src).map_err(|e| format!("{src}: {e}"))?)
        };
    }
    if let Some(url) = REMOTE_CATALOG_URL {
        if let Ok(c) = fetch(client, url).await {
            return Ok(c);
        }
    }
    parse(BUNDLED)
}

#[cfg(test)]
mod tests {
    #[test]
    fn bundled_catalog_is_valid() {
        super::parse(super::BUNDLED).unwrap();
    }

    #[test]
    fn store_titles_need_no_repo_but_an_https_link() {
        let ok = r#"{"titles":[{"id":"x","name":"x","store_url":"https://store.steampowered.com/app/1/"}]}"#;
        let c = super::parse(ok).unwrap();
        assert!(c.titles[0].build().is_none() && c.titles[0].store_url.is_some());
        let bad = r#"{"titles":[{"id":"x","name":"x","store_url":"javascript:alert(1)"}]}"#;
        assert!(super::parse(bad).is_err());
        let bundled = super::parse(super::BUNDLED).unwrap();
        assert!(bundled.titles.iter().any(|t| t.id == "ponyside" && t.store_url.is_some()));
    }

    #[test]
    fn rejects_unsafe_ids() {
        let bad = r#"{"titles":[{"id":"../x","name":"x","repo":"a/b","platforms":{}}]}"#;
        assert!(super::parse(bad).is_err());
    }
}
