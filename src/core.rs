//! Launcher logic without any UI: list/install/uninstall/launch titles, settings, safe links.

use crate::{catalog, install, state};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Support link shown in the sidebar.
pub const KOFI_URL: &str = "https://ko-fi.com/rosacepony";

/// Serialises read-modify-write cycles of installed.json.
static STATE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone)]
pub struct Core {
    pub data_dir: PathBuf,
    pub client: reqwest::Client,
}

#[derive(Clone, Debug)]
pub struct TitleView {
    pub title: catalog::Title,
    pub supported: bool,
    pub installed: Option<state::Installed>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "yes")]
    pub auto_update: bool,
}
fn yes() -> bool {
    true
}
impl Default for Settings {
    fn default() -> Self {
        Self { auto_update: true }
    }
}

pub fn load_settings(dir: &Path) -> Settings {
    std::fs::read_to_string(dir.join("settings.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}
pub fn save_settings(dir: &Path, s: &Settings) {
    let _ = std::fs::write(dir.join("settings.json"), serde_json::to_string_pretty(s).unwrap());
}

/// Local (not roaming) data: %LOCALAPPDATA%\com.rosace.launcher or ~/.local/share/com.rosace.launcher.
/// Same folder as the Tauri versions, so installed games are kept.
pub fn data_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    let dir = base.unwrap_or_else(std::env::temp_dir).join("com.rosace.launcher");
    let _ = std::fs::create_dir_all(&dir);
    // The old web view profile is no longer used.
    let _ = std::fs::remove_dir_all(dir.join("EBWebView"));
    dir
}

impl Core {
    pub fn new() -> Self {
        Self { data_dir: data_dir(), client: install::client() }
    }

    async fn find_title(&self, id: &str) -> Result<catalog::Title, String> {
        catalog::load(&self.client).await?.titles.into_iter().find(|t| t.id == id).ok_or_else(|| format!("Unknown title '{id}'"))
    }

    pub async fn list_titles(&self) -> Result<Vec<TitleView>, String> {
        let catalog = catalog::load(&self.client).await?;
        let installed = state::load(&self.data_dir);
        Ok(catalog
            .titles
            .into_iter()
            .map(|t| TitleView { supported: t.build().is_some() || t.store_url.is_some(), installed: installed.get(&t.id).cloned(), title: t })
            .collect())
    }

    pub async fn latest_release(&self, id: &str) -> Result<install::ReleaseInfo, String> {
        let title = self.find_title(id).await?;
        install::latest_release(&self.client, &title).await
    }

    /// Installs or updates to the latest release.
    pub async fn install_title(&self, id: &str, on_progress: &(dyn Fn(install::Progress) + Send + Sync)) -> Result<state::Installed, String> {
        let title = self.find_title(id).await?;
        let installed = install::install(&self.client, &self.data_dir, &title, on_progress).await?;
        let _lock = STATE_LOCK.lock().unwrap();
        let mut map = state::load(&self.data_dir);
        map.insert(id.to_string(), installed.clone());
        state::save(&self.data_dir, &map)?;
        Ok(installed)
    }

    pub fn uninstall_title(&self, id: &str) -> Result<(), String> {
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err("Invalid id".into());
        }
        let dir = self.data_dir.join("games").join(id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| format!("Could not delete files (is it running?): {e}"))?;
        }
        let _lock = STATE_LOCK.lock().unwrap();
        let mut map = state::load(&self.data_dir);
        map.remove(id);
        state::save(&self.data_dir, &map)
    }

    pub async fn launch_title(&self, id: &str) -> Result<(), String> {
        if !state::load(&self.data_dir).contains_key(id) {
            return Err("Not installed".into());
        }
        let title = self.find_title(id).await?;
        let build = title.build().ok_or("Not available on this platform")?;
        let dir = self.data_dir.join("games").join(id);
        let exe = dir.join(&build.executable);
        if title.elevate && cfg!(windows) {
            return launch_elevated(&exe, &build.args, &dir);
        }
        std::process::Command::new(&exe)
            .args(&build.args)
            .current_dir(&dir)
            .spawn()
            .map_err(|e| format!("Could not start {}: {e}", exe.display()))?;
        Ok(())
    }

    pub fn open_install_folder(&self, id: &str) -> Result<(), String> {
        let dir = self.data_dir.join("games").join(id);
        let dir = if dir.exists() { dir } else { self.data_dir.clone() };
        let opener = if cfg!(windows) { "explorer" } else { "xdg-open" };
        std::process::Command::new(opener).arg(dir).spawn().map(|_| ()).map_err(|e| e.to_string())
    }

    /// Opens a link in the default browser: only the Ko-fi page or a catalog title's store page.
    pub async fn open_link(&self, url: &str) -> Result<(), String> {
        let allowed = url == KOFI_URL || catalog::load(&self.client).await?.titles.iter().any(|t| t.store_url.as_deref() == Some(url));
        if !allowed {
            return Err("This link is not allowed".into());
        }
        let opener = if cfg!(windows) { "explorer" } else { "xdg-open" };
        std::process::Command::new(opener).arg(url).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}

/// Starts a program with administrator rights (Windows UAC prompt) without keeping a console around.
fn launch_elevated(exe: &Path, args: &[String], dir: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let q = |s: &str| format!("'{}'", s.replace('\'', "''"));
        let mut script = format!("Start-Process -FilePath {} -WorkingDirectory {} -Verb RunAs", q(&exe.display().to_string()), q(&dir.display().to_string()));
        if !args.is_empty() {
            script += &format!(" -ArgumentList {}", args.iter().map(|a| q(a)).collect::<Vec<_>>().join(","));
        }
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
            .creation_flags(0x0800_0000)
            .status()
            .map_err(|e| format!("Could not start {}: {e}", exe.display()))?;
        if status.success() {
            Ok(())
        } else {
            Err("Administrator permission was declined (or the program could not start).".into())
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (exe, args, dir);
        Err("Elevation is only used on Windows".into())
    }
}
