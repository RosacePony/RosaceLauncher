"""Publish a Rosace Launcher release: signed installer + latest.json for the self-updater.

usage: python tools/release.py --version 0.4.0 --notes "What changed"   [--dry-run]

Steps: bump the version (package.json, Cargo.toml, tauri.conf.json) -> cargo test ->
`npm run build` signed with the updater key (~/.tauri/rosace-launcher.key, never in git) ->
latest.json -> git commit + tag + push -> GitHub release with the installer and latest.json.
Installed launchers read releases/latest/download/latest.json, verify the signature with the
public key built into them, and offer "Update & restart".
"""
import argparse
import datetime as dt
import json
import os
import re
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REPO = "RosacePony/RosaceLauncher"
KEY = Path.home() / ".tauri" / "rosace-launcher.key"
GH = shutil.which("gh") or r"C:\Program Files\GitHub CLI\gh.exe"
NPM = shutil.which("npm") or "npm"


def run(cmd, **kw):
    print("+", " ".join(map(str, cmd)))
    subprocess.run(cmd, check=True, **kw)


def bump(version):
    for rel, pat in [
        ("package.json", r'("version":\s*")[^"]+(")'),
        ("src-tauri/tauri.conf.json", r'("version":\s*")[^"]+(")'),
        ("src-tauri/Cargo.toml", r'(?m)^(version\s*=\s*")[^"]+(")'),
    ]:
        p = ROOT / rel
        s = p.read_text(encoding="utf-8")
        s2, n = re.subn(pat, rf"\g<1>{version}\g<2>", s, count=1)
        assert n == 1, f"no version in {rel}"
        p.write_text(s2, encoding="utf-8")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--version", required=True)
    ap.add_argument("--notes", default="")
    ap.add_argument("--dry-run", action="store_true")
    a = ap.parse_args()
    v = a.version.lstrip("v")
    tag = f"v{v}"
    if subprocess.run(["git", "rev-parse", "-q", "--verify", f"refs/tags/{tag}"], cwd=ROOT, capture_output=True).returncode == 0:
        raise SystemExit(f"tag {tag} already exists")
    if not KEY.is_file():
        raise SystemExit(f"missing signing key {KEY}")
    bump(v)
    run(["cargo", "test"], cwd=ROOT / "src-tauri")
    env = dict(os.environ, TAURI_SIGNING_PRIVATE_KEY=KEY.read_text().strip(), TAURI_SIGNING_PRIVATE_KEY_PASSWORD="")
    run([NPM, "run", "build"], cwd=ROOT, env=env, shell=os.name == "nt")
    nsis = ROOT / "src-tauri/target/release/bundle/nsis"
    setup = nsis / f"Rosace Launcher_{v}_x64-setup.exe"
    sig = setup.with_name(setup.name + ".sig")
    assert setup.is_file() and sig.is_file(), f"missing {setup} or its .sig"
    out = ROOT / "src-tauri/target/upload"
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    asset = out / f"Rosace.Launcher_{v}_x64-setup.exe"  # GitHub turns spaces into dots anyway
    shutil.copy2(setup, asset)
    latest = {
        "version": v,
        "notes": a.notes,
        "pub_date": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "platforms": {
            "windows-x86_64": {
                "signature": sig.read_text().strip(),
                "url": f"https://github.com/{REPO}/releases/download/{tag}/{asset.name}",
            }
        },
    }
    (out / "latest.json").write_text(json.dumps(latest, indent=2), encoding="utf-8")
    print(f"staged {asset.name} + latest.json in {out}")
    if a.dry_run:
        return
    run(["git", "add", "-A"], cwd=ROOT)
    run(["git", "commit", "-qm", f"Release {tag}"], cwd=ROOT)
    run(["git", "tag", "-a", tag, "-m", f"Rosace Launcher {tag}"], cwd=ROOT)
    run(["git", "push", "origin", "HEAD", "--follow-tags"], cwd=ROOT)
    notes = f"Download **{asset.name}** below and run it (Windows). Installed launchers update themselves.\n\n{a.notes}"
    run([GH, "release", "create", tag, str(asset), str(out / "latest.json"), "--repo", REPO, "--title", f"Rosace Launcher {tag}", "--notes", notes])


if __name__ == "__main__":
    main()
