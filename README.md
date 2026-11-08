# Rosace Launcher

Desktop launcher (Windows + Linux) for Rosace's games and apps.
Pure Rust with the [Iced](https://iced.rs) GUI (native, no webview). Installs, updates, launches and uninstalls
titles from their **GitHub Releases**, and links to store pages (Steam) for the others.

## Download

Get **`Rosace.Launcher_x.y.z_x64-setup.exe`** from the
[latest release](https://github.com/RosacePony/RosaceLauncher/releases/latest) and run it.
Installed games update themselves (on start, every 30 minutes and before Play).

Like my games? [Fund my projects on Ko-fi](https://ko-fi.com/rosacepony).

## Categories

The sidebar groups titles into **Games** and **Software**. Set `"category": "game"` (default) or
`"software"` on a catalog entry. A title can also set `"elevate": true` to be started with
administrator rights on Windows (UAC prompt), e.g. network tools like
[RosaceLan](https://github.com/RosacePony/RosaceLan).

## Adding a title

Edit `catalog.json`:

```jsonc
{
  "id": "ponyside",                       // folder name: a-z 0-9 - _
  "name": "PonySide",
  "description": "…",
  "repo": "RosacePony/PonySide2025",      // GitHub owner/repo (must be public)
  "cover": "covers/ponyside.jpg",         // optional banner: file in assets/covers/ or an https URL
  "store_url": "https://store.steampowered.com/app/…", // optional: link to a store instead of installing (no repo/platforms needed)
  "platforms": {
    "windows": { "asset": "(?i)windows.*\\.zip$", "executable": "PonySide.exe" },
    "linux":   { "asset": "(?i)linux.*\\.(zip|tar\\.gz)$", "executable": "PonySide.x86_64", "args": [] }
  }
}
```

- `asset` is a regex matched against the file names of the repo's **latest** release.
- Supported release files: `.zip`, `.tar.gz`/`.tgz`, or a single file (`.exe`, `.AppImage`).
  If the archive has one top-level folder, it is flattened automatically.
- `executable` is relative to the install folder.

### Publishing a game version
Create a GitHub Release (e.g. tag `v0.3.0`) on the game's repo and attach e.g.
`PonySide-windows.zip` and `PonySide-linux.zip`. Launchers see the new tag and show **Update**.

### Catalog without rebuilding the launcher
Launchers read `catalog.json` from this repo's `main` branch (`REMOTE_CATALOG_URL` in
`src/catalog.rs`), falling back to the copy bundled at build time. Pushing a
catalog change reaches every installed launcher; new cover images need a launcher release
(or an https URL). For testing, the env var `ROSACE_CATALOG`
(URL or file path) overrides everything.

## Security

- Games: downloads use HTTPS, and every file must match the SHA-256 that GitHub recorded at
  upload (release asset `digest`); a mismatch aborts before anything is installed. Archives
  can't write outside the game folder (zip-slip / tar escape checks).
- The launcher updates itself from this repo's releases (`latest.json`). Updates are signed:
  each launcher carries the public key and refuses an installer whose signature doesn't
  match. The private key lives in `~/.tauri/rosace-launcher.key` on the release PC, never in
  git. **Back it up**: without it, installed launchers can't receive updates anymore.
- Everything installs per user (no admin). The page can only open the Ko-fi page or a store
  link from the catalog.
- Protect the GitHub account with two-factor authentication: the catalog and releases come
  from it.

## Releasing the launcher
```
python tools/release.py --version 0.4.1 --notes "What changed"
```
Bumps the version, runs the tests, builds the signed installer, writes `latest.json`, tags,
pushes and creates the GitHub release. Installed launchers show **Update & restart**.

## Where things go
- Windows: `%LOCALAPPDATA%\com.rosace.launcher\games\<id>`
- Linux: `~/.local/share/com.rosace.launcher/games/<id>`
- `installed.json` next to `games/` records installed versions.

## Develop / build
```
cargo run                      # debug build
cargo test -- --include-ignored   # includes a real GitHub install test
python tools/release.py --version X.Y.Z --notes "..."   # test, build, NSIS installer, sign, GitHub release
```
Linux: `cargo build --release` (needs the usual Wayland/X11 + Vulkan/OpenGL libraries); no installer or
self-update is published for Linux yet.

## Limits (v1)
- Public repos only (private releases need a token).
- Unauthenticated GitHub API: 60 requests/hour per IP — one per title per refresh.
