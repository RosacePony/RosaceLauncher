//! Launcher self-update. Reads the same `latest.json` format as the Tauri updater and checks the
//! installer's minisign signature against the public key built into this executable, so a
//! tampered download is never run.

use base64::Engine;
use futures_util::StreamExt;
use serde::Deserialize;
use std::path::Path;
use tokio::io::AsyncWriteExt;

const ENDPOINT: &str = "https://github.com/RosacePony/RosaceLauncher/releases/latest/download/latest.json";
/// Base64 of the minisign public key file (the value that used to live in tauri.conf.json).
const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDM2QjczREI5QkMwNEZENTcKUldSWC9RUzh1VDIzTnFnd2tmSlo5V1RNZGV6QW9XYXZFNWNlWkxJSkxtQ05DL3hzdkZpay9sRDIK";
const PLATFORM: &str = "windows-x86_64";

#[derive(Deserialize)]
struct Manifest {
    version: String,
    #[serde(default)]
    notes: Option<String>,
    platforms: std::collections::HashMap<String, Entry>,
}

#[derive(Deserialize)]
struct Entry {
    url: String,
    signature: String,
}

#[derive(Clone, Debug)]
pub struct Update {
    pub version: String,
    pub notes: Option<String>,
    url: String,
    signature: String,
}

/// "1.2.3" (optionally "v1.2.3" / "1.2.3-beta") as comparable numbers.
fn parse_version(v: &str) -> Option<[u64; 3]> {
    let core = v.trim().trim_start_matches('v').split(['-', '+']).next()?;
    let mut it = core.split('.').map(|p| p.parse::<u64>());
    Some([it.next()?.ok()?, it.next().unwrap_or(Ok(0)).ok()?, it.next().unwrap_or(Ok(0)).ok()?])
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    matches!((parse_version(candidate), parse_version(current)), (Some(a), Some(b)) if a > b)
}

/// A newer signed launcher on GitHub, or None. Only Windows installers are published.
pub async fn check(client: &reqwest::Client) -> Result<Option<Update>, String> {
    if !cfg!(windows) {
        return Ok(None);
    }
    let m: Manifest = client
        .get(ENDPOINT)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    if !is_newer(&m.version, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }
    let e = m.platforms.get(PLATFORM).ok_or("No installer for this platform")?;
    Ok(Some(Update { version: m.version, notes: m.notes, url: e.url.clone(), signature: e.signature.clone() }))
}

fn b64(s: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(s.trim()).map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

/// Checks `data` against the minisign signature (base64 of the .sig file) with the embedded key.
pub fn verify(data: &[u8], signature_b64: &str) -> Result<(), String> {
    verify_with(PUBKEY, data, signature_b64)
}

fn verify_with(pubkey_b64: &str, data: &[u8], signature_b64: &str) -> Result<(), String> {
    let pk = minisign_verify::PublicKey::decode(&b64(pubkey_b64)?).map_err(|e| format!("Bad public key: {e}"))?;
    let sig = minisign_verify::Signature::decode(&b64(signature_b64)?).map_err(|e| format!("Bad signature: {e}"))?;
    pk.verify(data, &sig, false).map_err(|e| format!("Signature check failed, update refused: {e}"))
}

/// Downloads the installer, verifies it, starts it and quits so it can replace the running exe.
pub async fn install(client: &reqwest::Client, data_dir: &Path, update: &Update) -> Result<(), String> {
    let dir = data_dir.join("updates");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!("Rosace.Launcher_{}_x64-setup.exe", update.version));
    let resp = client.get(&update.url).send().await.and_then(|r| r.error_for_status()).map_err(|e| format!("Download failed: {e}"))?;
    let mut out = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        out.extend_from_slice(&chunk.map_err(|e| format!("Download interrupted: {e}"))?);
    }
    verify(&out, &update.signature)?;
    let mut f = tokio::fs::File::create(&file).await.map_err(|e| e.to_string())?;
    f.write_all(&out).await.map_err(|e| e.to_string())?;
    f.flush().await.map_err(|e| e.to_string())?;
    drop(f);
    // /P = passive (progress only), /R = start the launcher again when done.
    std::process::Command::new(&file).args(["/P", "/R"]).spawn().map_err(|e| format!("Could not start the installer: {e}"))?;
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("v1.0.0", "0.99.0"));
        assert!(!is_newer("0.5.0", "0.5.0"));
        assert!(!is_newer("0.4.1", "0.5.0"));
        assert!(!is_newer("garbage", "0.5.0"));
    }

    #[test]
    fn embedded_public_key_decodes() {
        let key = b64(PUBKEY).unwrap();
        assert!(minisign_verify::PublicKey::decode(&key).is_ok());
    }

    #[test]
    fn rejects_garbage_signatures() {
        assert!(verify(b"hello", "not base64!!").is_err());
        let fake = base64::engine::general_purpose::STANDARD.encode("untrusted comment: x\nRWQ=\ntrusted comment: y\nAAAA\n");
        assert!(verify(b"hello", &fake).is_err());
    }

    /// Run `python tools/release.py --version X --dry-run` first: checks the real installer and
    /// its real signature against the embedded key, and that a flipped byte is refused.
    #[test]
    #[ignore]
    fn staged_installer_signature_verifies() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/upload");
        let exe = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).find(|p| p.to_string_lossy().ends_with("-setup.exe")).expect("staged installer");
        let data = std::fs::read(&exe).unwrap();
        let latest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("latest.json")).unwrap()).unwrap();
        let sig = latest["platforms"]["windows-x86_64"]["signature"].as_str().unwrap();
        verify(&data, sig).unwrap();
        let mut bad = data.clone();
        bad[100] ^= 1;
        assert!(verify(&bad, sig).is_err());
    }
}
