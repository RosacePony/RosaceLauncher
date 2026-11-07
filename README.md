# Rosace Launcher

Desktop launcher (Windows + Linux) for Rosace's games and apps.
Rust + Tauri v2 backend, plain HTML/JS UI. Installs, updates, launches and uninstalls
titles from their **GitHub Releases**, and links to store pages (Steam) for the others.

## Download

Get **`Rosace.Launcher_x.y.z_x64-setup.exe`** from the
[latest release](https://github.com/RosacePony/RosaceLauncher/releases/latest) and run it.
Installed games update themselves (on start, every 30 minutes and before Play).

Like my games? [Fund my projects on Ko-fi](https://ko-fi.com/rosacepony).

## Adding a title

Edit `catalog.json`:

```jsonc
{
  "id": "ponyside",                       // folder name: a-z 0-9 - _
  "name": "PonySide",
  "description": "…",
  "repo": "RosacePony/PonySide2025",      // GitHub owner/repo (must be public)
  "cover": "covers/ponyside.jpg",         // optional banner: file in src/covers/ or an https URL
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
`src-tauri/src/catalog.rs`), falling back to the copy bundled at build time. Pushing a
catalog change reaches every installed launcher; new cover images need a launcher release
(or an https URL). For testing, the env var `ROSACE_CATALOG`
(URL or file path) overrides everything.

## Where things go
- Windows: `%LOCALAPPDATA%\com.rosace.launcher\games\<id>`
- Linux: `~/.local/share/com.rosace.launcher/games/<id>`
- `installed.json` next to `games/` records installed versions.

## Develop / build
```
npm install
npm run dev          # run with hot reload of the UI
npm run build        # Windows: NSIS installer in src-tauri/target/release/bundle/nsis
cd src-tauri && cargo test -- --include-ignored   # includes a real GitHub install test
```
Linux build (on a Linux machine): install the Tauri prerequisites
(`libwebkit2gtk-4.1-dev build-essential libssl-dev libayatana-appindicator3-dev librsvg2-dev`),
then `npm run build` → `.AppImage` and `.deb`.

## Limits (v1)
- Public repos only (private releases need a token).
- Unauthenticated GitHub API: 60 requests/hour per IP — one per title per refresh.
