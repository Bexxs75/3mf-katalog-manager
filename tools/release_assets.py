#!/usr/bin/env python3
"""Release assets: readable file names, updater manifests and a version check.

Used by .github/workflows/release.yml; see .github/RELEASE_TEMPLATE.md for the names.
"""
import datetime, json, pathlib, re, shutil, sys

REPO = "Bexxs75/3mf-katalog-manager"
PRODUCT = "3MF-Katalog-Manager"
# (artifact name from the build workflows, extension, platform part of the name, STEP?)
ARTIFACTS = [
    ("3mf-katalog-manager-windows-msi", ".msi", "Windows-x64", False),
    ("3mf-katalog-manager-windows-msi-step", ".msi", "Windows-x64", True),
    ("3mf-katalog-manager-macos-dmg", ".dmg", "macOS-universal", False),
    ("3mf-katalog-manager-macos-dmg-universal-step", ".dmg", "macOS-universal", True),
    ("3mf-katalog-manager-macos-updater", ".app.tar.gz", "macOS-universal", False),
    ("3mf-katalog-manager-macos-updater-step", ".app.tar.gz", "macOS-universal", True),
    ("3mf-katalog-manager-linux-appimage", ".AppImage", "Linux-x86_64", False),
    ("3mf-katalog-manager-linux-appimage-step", ".AppImage", "Linux-x86_64", True),
]
# Updater platform key -> (platform part, extension). Apple Silicon and Intel share the universal bundle.
UPDATE_TARGETS = {
    "windows-x86_64": ("Windows-x64", ".msi"),
    "darwin-x86_64": ("macOS-universal", ".app.tar.gz"),
    "darwin-aarch64": ("macOS-universal", ".app.tar.gz"),
    "linux-x86_64": ("Linux-x86_64", ".AppImage"),
}

def fail(msg):
    print(f"release_assets: {msg}", file=sys.stderr)
    raise SystemExit(1)

def asset_name(version, platform, step, ext):
    return f"{PRODUCT}-{version}-{platform}{'-STEP' if step else ''}{ext}"

def rename(version, artifacts_dir, out_dir):
    out = pathlib.Path(out_dir); out.mkdir(parents=True, exist_ok=True)
    for art, ext, platform, step in ARTIFACTS:
        found = [p for p in pathlib.Path(artifacts_dir, art).glob(f"*{ext}")] if pathlib.Path(artifacts_dir, art).is_dir() else []
        if len(found) != 1:
            fail(f"expected exactly one *{ext} in {art}, found {len(found)}")
        shutil.copy2(found[0], out / asset_name(version, platform, step, ext))

def latest_json(version, out_dir, now=None):
    now = now or datetime.datetime.now(datetime.timezone.utc)
    out = pathlib.Path(out_dir)
    for step, file in ((False, "latest.json"), (True, "latest-step.json")):
        platforms = {}
        for key, (platform, ext) in UPDATE_TARGETS.items():
            name = asset_name(version, platform, step, ext)
            sig = out / f"{name}.sig"
            if not (out / name).is_file() or not sig.is_file():
                fail(f"missing {name} or its signature")
            platforms[key] = {
                "signature": sig.read_text().strip(),
                "url": f"https://github.com/{REPO}/releases/download/v{version}/{name}",
            }
        manifest = {
            "version": version,
            "notes": f"https://github.com/{REPO}/releases/tag/v{version}",
            "pub_date": now.strftime("%Y-%m-%dT%H:%M:%SZ"),
            "platforms": platforms,
        }
        (out / file).write_text(json.dumps(manifest, indent=2) + "\n")

def check_version(version, root="."):
    cargo = pathlib.Path(root, "src-tauri", "Cargo.toml").read_text()
    package = cargo.split("[package]", 1)[1].split("\n[", 1)[0]
    m = re.search(r'^version\s*=\s*"([^"]+)"', package, re.M)
    conf = json.loads(pathlib.Path(root, "src-tauri", "tauri.conf.json").read_text())
    found = {"Cargo.toml": m.group(1) if m else None, "tauri.conf.json": conf.get("version")}
    wrong = {k: v for k, v in found.items() if v != version}
    if wrong:
        fail(f"version {version} does not match {wrong}")

def main(argv):
    cmd, *args = argv
    {"rename": rename, "latest-json": latest_json, "check-version": check_version}[cmd](*args)

if __name__ == "__main__":
    main(sys.argv[1:])
