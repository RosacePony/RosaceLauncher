//! GitHub Releases lookup, download, extraction.

use crate::catalog::Title;
use crate::state::Installed;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    name: Option<String>,
    published_at: Option<String>,
    body: Option<String>,
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    /// "sha256:<hex>", computed by GitHub when the file was uploaded.
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReleaseInfo {
    pub tag: String,
    pub name: String,
    pub published_at: Option<String>,
    pub notes: String,
    pub asset_name: String,
    pub size: u64,
    pub download_url: String,
    /// Expected SHA-256 of the file (lowercase hex), from GitHub.
    pub sha256: Option<String>,
}

/// The lowercase hex SHA-256 in a GitHub asset digest ("sha256:<64 hex>").
pub fn parse_digest(digest: Option<&str>) -> Option<String> {
    let hex = digest?.strip_prefix("sha256:")?.to_ascii_lowercase();
    (hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit())).then_some(hex)
}

/// Lowercase hex of a SHA-256 hash.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub id: String,
    pub stage: &'static str, // "download" | "extract" | "done"
    pub downloaded: u64,
    pub total: u64,
}

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("RosaceLauncher/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .expect("http client")
}

pub async fn latest_release(client: &reqwest::Client, title: &Title) -> Result<ReleaseInfo, String> {
    let build = title
        .build()
        .ok_or_else(|| format!("{} is not available on {}", title.name, crate::catalog::PLATFORM))?;
    let url = format!("https://api.github.com/repos/{}/releases/latest", title.repo);
    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;
    match resp.status().as_u16() {
        200 => {}
        404 => return Err("No public release yet".into()),
        403 | 429 => return Err("GitHub rate limit reached, try again later".into()),
        s => return Err(format!("GitHub answered HTTP {s}")),
    }
    let rel: GhRelease = resp.json().await.map_err(|e| e.to_string())?;
    let re = regex::Regex::new(&build.asset).map_err(|e| e.to_string())?;
    let asset = rel
        .assets
        .iter()
        .find(|a| re.is_match(&a.name))
        .ok_or_else(|| format!("Release {} has no file matching {}", rel.tag_name, build.asset))?;
    Ok(ReleaseInfo {
        name: rel.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| rel.tag_name.clone()),
        tag: rel.tag_name,
        published_at: rel.published_at,
        notes: rel.body.unwrap_or_default(),
        asset_name: asset.name.clone(),
        size: asset.size,
        download_url: asset.browser_download_url.clone(),
        sha256: parse_digest(asset.digest.as_deref()),
    })
}

/// Download, extract and swap in the latest release. Old install is only replaced once
/// the new one is fully extracted and its executable found.
pub async fn install(
    client: &reqwest::Client,
    data_dir: &Path,
    title: &Title,
    on_progress: &(dyn Fn(Progress) + Send + Sync),
) -> Result<Installed, String> {
    let build = title.build().ok_or("Not available on this platform")?.clone();
    let info = latest_release(client, title).await?;

    let downloads = data_dir.join("downloads");
    let games = data_dir.join("games");
    fs::create_dir_all(&downloads).map_err(|e| e.to_string())?;
    fs::create_dir_all(&games).map_err(|e| e.to_string())?;
    let part = downloads.join(format!("{}.part", title.id));

    // Download with progress.
    let resp = client
        .get(&info.download_url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Download failed: {e}"))?;
    let total = resp.content_length().unwrap_or(info.size);
    let mut file = tokio::fs::File::create(&part).await.map_err(|e| e.to_string())?;
    let mut stream = resp.bytes_stream();
    let mut downloaded = 0u64;
    let mut hasher = Sha256::new();
    let mut last = Instant::now();
    let progress = |stage, downloaded| Progress { id: title.id.clone(), stage, downloaded, total };
    on_progress(progress("download", 0));
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download interrupted: {e}"))?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;
        if last.elapsed() > Duration::from_millis(120) {
            on_progress(progress("download", downloaded));
            last = Instant::now();
        }
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    // Integrity: the file must match the SHA-256 GitHub recorded at upload time.
    let got = hex(&hasher.finalize());
    match &info.sha256 {
        Some(want) if *want != got => {
            let _ = fs::remove_file(&part);
            return Err(format!("Download corrupted or tampered with (SHA-256 mismatch: expected {want}, got {got}). Nothing was installed."));
        }
        Some(_) => {}
        None => eprintln!("{}: GitHub gave no checksum for {}; installed without verification", title.id, info.asset_name),
    }
    on_progress(progress("extract", downloaded));

    // Extract off the async runtime.
    let staging = games.join(format!(".{}.staging", title.id));
    let target = games.join(&title.id);
    let asset_name = info.asset_name.clone();
    let part2 = part.clone();
    let staging2 = staging.clone();
    let exe = build.executable.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        extract(&part2, &asset_name, &staging2)?;
        if !staging2.join(&exe).is_file() {
            flatten_single_dir(&staging2)?;
        }
        if !staging2.join(&exe).is_file() {
            let _ = fs::remove_dir_all(&staging2);
            return Err(format!("'{exe}' not found in {asset_name}"));
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())??;

    if target.exists() {
        fs::remove_dir_all(&target)
            .map_err(|e| format!("Could not remove old version (is the game running?): {e}"))?;
    }
    fs::rename(&staging, &target).map_err(|e| e.to_string())?;
    let _ = fs::remove_file(&part);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(target.join(&build.executable), fs::Permissions::from_mode(0o755));
    }

    on_progress(progress("done", downloaded));
    Ok(Installed {
        version: info.tag,
        asset: info.asset_name,
        installed_at: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
    })
}

fn extract(archive: &Path, asset_name: &str, dest: &Path) -> Result<(), String> {
    if dest.exists() {
        fs::remove_dir_all(dest).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let lower = asset_name.to_ascii_lowercase();
    if lower.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(fs::File::open(archive).map_err(|e| e.to_string())?)
            .map_err(|e| format!("Bad zip: {e}"))?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
            // enclosed_name rejects absolute paths and "..": no zip-slip.
            let Some(rel) = entry.enclosed_name() else { continue };
            let out = dest.join(rel);
            if entry.is_dir() {
                fs::create_dir_all(&out).map_err(|e| e.to_string())?;
                continue;
            }
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut f = fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut f).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            if let Some(mode) = entry.unix_mode() {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&out, fs::Permissions::from_mode(mode));
            }
        }
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        let gz = flate2::read::GzDecoder::new(fs::File::open(archive).map_err(|e| e.to_string())?);
        // tar's unpack refuses entries escaping dest.
        tar::Archive::new(gz).unpack(dest).map_err(|e| format!("Bad archive: {e}"))?;
    } else {
        // Single file release (.exe, .AppImage...).
        fs::copy(archive, dest.join(asset_name)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Archives often wrap everything in one top folder; move its contents up.
fn flatten_single_dir(dest: &Path) -> Result<(), String> {
    let entries: Vec<PathBuf> = fs::read_dir(dest)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    if entries.len() != 1 || !entries[0].is_dir() {
        return Ok(());
    }
    let tmp = dest.join(".flatten-tmp");
    fs::rename(&entries[0], &tmp).map_err(|e| e.to_string())?;
    for e in fs::read_dir(&tmp).map_err(|e| e.to_string())? {
        let p = e.map_err(|e| e.to_string())?.path();
        fs::rename(&p, dest.join(p.file_name().unwrap())).map_err(|e| e.to_string())?;
    }
    fs::remove_dir(&tmp).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_parse_and_hash_matches() {
        let h = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(parse_digest(Some(&format!("sha256:{}", h.to_uppercase()))).as_deref(), Some(h));
        assert_eq!(parse_digest(Some("md5:abc")), None);
        assert_eq!(parse_digest(Some("sha256:xyz")), None);
        assert_eq!(parse_digest(None), None);
        // SHA-256 of nothing.
        assert_eq!(hex(&Sha256::new().finalize()), h);
    }

    /// Real network test: installs ripgrep's Windows/Linux release (archive with a top folder).
    /// Run with: cargo test -- --ignored
    #[tokio::test]
    #[ignore]
    async fn installs_real_github_release() {
        let catalog = crate::catalog::parse(
            r#"{"titles":[{"id":"rg","name":"ripgrep","repo":"BurntSushi/ripgrep","platforms":{
              "windows":{"asset":"x86_64-pc-windows-msvc\\.zip$","executable":"rg.exe"},
              "linux":{"asset":"x86_64-unknown-linux-musl\\.tar\\.gz$","executable":"rg"}}}]}"#,
        )
        .unwrap();
        let dir = std::env::temp_dir().join("rosace-launcher-test");
        let _ = fs::remove_dir_all(&dir);
        let client = client();
        let stages = std::sync::Mutex::new(Vec::new());
        let installed = install(&client, &dir, &catalog.titles[0], &|p| stages.lock().unwrap().push(p.stage))
            .await
            .unwrap();
        let exe = dir.join("games/rg").join(&catalog.titles[0].build().unwrap().executable);
        assert!(exe.is_file(), "missing {exe:?}");
        let out = std::process::Command::new(&exe).arg("--version").output().unwrap();
        assert!(String::from_utf8_lossy(&out.stdout).contains("ripgrep"));
        assert_eq!(stages.lock().unwrap().last(), Some(&"done"));
        println!("installed {}", installed.version);
        let _ = fs::remove_dir_all(&dir);
    }

    /// The catalog points Claws & Magic at the PUBLIC binaries-only repo (the source repo is private)
    /// and keeps its id, so existing installs in games/claws-and-magic update in place.
    #[test]
    fn claws_and_magic_installs_from_public_builds_repo() {
        let catalog = crate::catalog::parse(include_str!("../catalog.json")).unwrap();
        let title = catalog.titles.iter().find(|t| t.id == "claws-and-magic").expect("catalog entry");
        assert_eq!(title.repo, "RosacePony/ClawsAndMagic-builds");
        let re = regex::Regex::new(&title.build().unwrap().asset).unwrap();
        assert!(re.is_match("ClawsAndMagic-windows-v0.4.1.zip"));
    }

    /// Real network test (light): resolves the latest Claws & Magic release asset from the builds
    /// repo and checks the download URL answers. Run with: cargo test resolves_claws -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn resolves_claws_and_magic_latest_asset() {
        let catalog = crate::catalog::parse(include_str!("../catalog.json")).unwrap();
        let title = catalog.titles.iter().find(|t| t.id == "claws-and-magic").expect("catalog entry");
        let c = client();
        let info = latest_release(&c, title).await.unwrap();
        assert!(info.tag.starts_with('v') && info.size > 1_000_000 && info.sha256.is_some(), "{info:?}");
        assert!(info.download_url.starts_with("https://github.com/RosacePony/ClawsAndMagic-builds/releases/download/"), "{}", info.download_url);
        let resp = c.head(&info.download_url).send().await.unwrap();
        assert!(resp.status().is_success(), "HEAD {} -> {}", info.download_url, resp.status());
        println!("{} {} ({} bytes) -> {}", info.tag, info.asset_name, info.size, info.download_url);
    }

    /// Real network test: installs Claws & Magic from the bundled catalog (latest GitHub release,
    /// ~230 MB) and checks the installed game answers `--version`.
    /// Run with: cargo test installs_claws -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn installs_claws_and_magic_from_catalog() {
        let catalog = crate::catalog::parse(include_str!("../catalog.json")).unwrap();
        let title = catalog.titles.iter().find(|t| t.id == "claws-and-magic").expect("catalog entry");
        let dir = std::env::temp_dir().join("rosace-launcher-test-cm");
        let _ = fs::remove_dir_all(&dir);
        let installed = install(&client(), &dir, title, &|_| {}).await.unwrap();
        let game = dir.join("games/claws-and-magic");
        let exe = game.join(&title.build().unwrap().executable);
        assert!(exe.is_file(), "missing {exe:?}");
        assert!(game.join("assets/characters").is_dir(), "assets not installed next to the exe");
        let out = std::process::Command::new(&exe).arg("--version").current_dir(&game).output().unwrap();
        let v = String::from_utf8_lossy(&out.stdout);
        assert!(v.contains("Claws & Magic"), "{v}");
        assert!(installed.version.trim_start_matches('v') == v.trim().rsplit(' ').next().unwrap(), "{} vs {v}", installed.version);
        println!("installed {} -> {}", installed.version, v.trim());
        let _ = fs::remove_dir_all(&dir);
    }
}
