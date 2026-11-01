//! installed.json: what is installed, which version.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Installed {
    pub version: String,
    pub asset: String,
    pub installed_at: u64,
}

pub type InstalledMap = HashMap<String, Installed>;

fn file(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("installed.json")
}

pub fn load(data_dir: &Path) -> InstalledMap {
    std::fs::read_to_string(file(data_dir))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(data_dir: &Path, map: &InstalledMap) -> Result<(), String> {
    std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let tmp = data_dir.join("installed.json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(map).unwrap()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, file(data_dir)).map_err(|e| e.to_string())
}
