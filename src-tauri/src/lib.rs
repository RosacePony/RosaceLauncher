mod catalog;
mod install;
mod state;

use serde::Serialize;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

struct Launcher {
    data_dir: PathBuf,
    client: reqwest::Client,
    busy: Mutex<HashSet<String>>,
    state_lock: Mutex<()>,
}

/// Marks a title busy for the duration of an install/uninstall.
struct BusyGuard<'a>(&'a Mutex<HashSet<String>>, String);
impl<'a> BusyGuard<'a> {
    fn take(set: &'a Mutex<HashSet<String>>, id: &str) -> Result<Self, String> {
        if !set.lock().unwrap().insert(id.to_string()) {
            return Err("Already busy with this title".into());
        }
        Ok(Self(set, id.to_string()))
    }
}
impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.lock().unwrap().remove(&self.1);
    }
}

#[derive(Serialize)]
struct TitleView {
    id: String,
    name: String,
    description: String,
    repo: String,
    cover: Option<String>,
    supported: bool,
    installed: Option<state::Installed>,
}

async fn find_title(l: &Launcher, id: &str) -> Result<catalog::Title, String> {
    catalog::load(&l.client)
        .await?
        .titles
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| format!("Unknown title '{id}'"))
}

#[tauri::command]
async fn list_titles(launcher: State<'_, Launcher>) -> Result<Vec<TitleView>, String> {
    let catalog = catalog::load(&launcher.client).await?;
    let installed = state::load(&launcher.data_dir);
    Ok(catalog
        .titles
        .into_iter()
        .map(|t| TitleView {
            supported: t.build().is_some(),
            installed: installed.get(&t.id).cloned(),
            id: t.id,
            name: t.name,
            description: t.description,
            repo: t.repo,
            cover: t.cover,
        })
        .collect())
}

#[tauri::command]
async fn latest_release(launcher: State<'_, Launcher>, id: String) -> Result<install::ReleaseInfo, String> {
    let title = find_title(&launcher, &id).await?;
    install::latest_release(&launcher.client, &title).await
}

/// Installs or updates to the latest release.
#[tauri::command]
async fn install_title(app: AppHandle, launcher: State<'_, Launcher>, id: String) -> Result<state::Installed, String> {
    let _busy = BusyGuard::take(&launcher.busy, &id)?;
    let title = find_title(&launcher, &id).await?;
    let emit = move |p: install::Progress| {
        let _ = app.emit("install-progress", p);
    };
    let installed = install::install(&launcher.client, &launcher.data_dir, &title, &emit).await?;
    let _lock = launcher.state_lock.lock().unwrap();
    let mut map = state::load(&launcher.data_dir);
    map.insert(id, installed.clone());
    state::save(&launcher.data_dir, &map)?;
    Ok(installed)
}

#[tauri::command]
async fn uninstall_title(launcher: State<'_, Launcher>, id: String) -> Result<(), String> {
    let _busy = BusyGuard::take(&launcher.busy, &id)?;
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') || id.is_empty() {
        return Err("Invalid id".into());
    }
    let dir = launcher.data_dir.join("games").join(&id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("Could not delete files (is it running?): {e}"))?;
    }
    let _lock = launcher.state_lock.lock().unwrap();
    let mut map = state::load(&launcher.data_dir);
    map.remove(&id);
    state::save(&launcher.data_dir, &map)
}

#[tauri::command]
async fn launch_title(launcher: State<'_, Launcher>, id: String) -> Result<(), String> {
    if !state::load(&launcher.data_dir).contains_key(&id) {
        return Err("Not installed".into());
    }
    let title = find_title(&launcher, &id).await?;
    let build = title.build().ok_or("Not available on this platform")?;
    let dir = launcher.data_dir.join("games").join(&id);
    let exe = dir.join(&build.executable);
    std::process::Command::new(&exe)
        .args(&build.args)
        .current_dir(&dir)
        .spawn()
        .map_err(|e| format!("Could not start {}: {e}", exe.display()))?;
    Ok(())
}

#[tauri::command]
fn open_install_folder(launcher: State<'_, Launcher>, id: String) -> Result<(), String> {
    let dir = launcher.data_dir.join("games").join(&id);
    let dir = if dir.exists() { dir } else { launcher.data_dir.clone() };
    let opener = if cfg!(windows) { "explorer" } else { "xdg-open" };
    std::process::Command::new(opener).arg(dir).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Local (not roaming) app data: %LOCALAPPDATA%\com.rosaceslogic.launcher or ~/.local/share/...
            let data_dir = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            app.manage(Launcher {
                data_dir,
                client: install::client(),
                busy: Mutex::new(HashSet::new()),
                state_lock: Mutex::new(()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_titles,
            latest_release,
            install_title,
            uninstall_title,
            launch_title,
            open_install_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running Rosace's Logic launcher");
}
