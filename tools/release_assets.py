#!/usr/bin/env python3
"""Release assets: readable file names, updater manifests and a version check.

Used by .github/workflows/release.yml; see .github/RELEASE_TEMPLATE.md for the names.
"""
import datetime, json, pathlib, re, shutil, sys

REPO = "Bexxs75/3mf-katalog-manager"
PRODUCT = "3MF-Katalog-Manager"
PRODUCT_PREVIEW = "3MF-Katalog-Manager-Preview"
PREVIEW_TAG = "preview"
# Numeric build number keeps the Windows MSI happy (it rejects non-numeric pre-release parts).
PREVIEW_VERSION = re.compile(r"^\d+\.\d+\.\d+-\d+$")
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

def asset_name(version, platform, step, ext, preview=False):
    product = PRODUCT_PREVIEW if preview else PRODUCT
    return f"{product}-{version}-{platform}{'-STEP' if step else ''}{ext}"

def rename(version, artifacts_dir, out_dir, preview=False, step=True):
    out = pathlib.Path(out_dir); out.mkdir(parents=True, exist_ok=True)
    for art, ext, platform, art_step in ARTIFACTS:
        if art_step and not step:
            continue  # no-STEP preview builds never produce the STEP artifacts
        found = [p for p in pathlib.Path(artifacts_dir, art).glob(f"*{ext}")] if pathlib.Path(artifacts_dir, art).is_dir() else []
        if len(found) != 1:
            fail(f"expected exactly one *{ext} in {art}, found {len(found)}")
        shutil.copy2(found[0], out / asset_name(version, platform, art_step, ext, preview=preview))

def latest_json(version, out_dir, now=None, preview=False, step=True):
    now = now or datetime.datetime.now(datetime.timezone.utc)
    out = pathlib.Path(out_dir)
    # Preview builds live on their own tag, separate from tagged releases, so
    # neither manifest nor asset URLs overlap between the two update channels.
    tag = PREVIEW_TAG if preview else f"v{version}"
    if preview:
        variants = ((False, "latest-preview.json"), (True, "latest-preview-step.json"))
    else:
        variants = ((False, "latest.json"), (True, "latest-step.json"))
    for is_step, file in variants:
        if is_step and not step:
            continue
        platforms = {}
        for key, (platform, ext) in UPDATE_TARGETS.items():
            name = asset_name(version, platform, is_step, ext, preview=preview)
            sig = out / f"{name}.sig"
            if not (out / name).is_file() or not sig.is_file():
                fail(f"missing {name} or its signature")
            platforms[key] = {
                "signature": sig.read_text().strip(),
                "url": f"https://github.com/{REPO}/releases/download/{tag}/{name}",
            }
        manifest = {
            "version": version,
            "notes": f"https://github.com/{REPO}/releases/tag/{tag}",
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

def version_key(version):
    # A build with no "-n" is a stable version, which always outranks any
    # preview build of the same x.y.z, hence the infinite build number.
    m = re.match(r"^(\d+)\.(\d+)\.(\d+)(?:-(\d+))?$", version)
    if not m:
        fail(f"invalid version {version}")
    x, y, z, n = m.groups()
    return (int(x), int(y), int(z), int(n) if n is not None else float("inf"))

def check_preview(version, manifest_path=None):
    if not PREVIEW_VERSION.match(version):
        fail(f"invalid preview version {version}")
    if manifest_path is None:
        return
    manifest_path = pathlib.Path(manifest_path)
    if not manifest_path.is_file():
        return  # nothing published yet under this channel, so anything goes
    manifest = json.loads(manifest_path.read_text())
    if version_key(version) < version_key(manifest["version"]):
        fail(f"version {version} is older than the published preview {manifest['version']}")

def set_version(version, root="."):
    root = pathlib.Path(root)

    cargo_path = root / "src-tauri" / "Cargo.toml"
    prefix, _, rest = cargo_path.read_text().partition("[package]")
    package, sep, suffix = rest.partition("\n[")
    name_match = re.search(r'^name\s*=\s*"([^"]+)"', package, re.M)
    if not name_match:
        fail(f"{cargo_path} has no [package] name")
    package_name = name_match.group(1)
    package = re.sub(r'^version\s*=\s*"[^"]+"', f'version = "{version}"', package, count=1, flags=re.M)
    cargo_path.write_text(prefix + "[package]" + package + sep + suffix, newline="\n")

    conf_path = root / "src-tauri" / "tauri.conf.json"
    conf = json.loads(conf_path.read_text())
    conf["version"] = version
    conf_path.write_text(json.dumps(conf, indent=2) + "\n", newline="\n")

    pkg_path = root / "package.json"
    pkg = json.loads(pkg_path.read_text())
    pkg["version"] = version
    pkg_path.write_text(json.dumps(pkg, indent=2) + "\n", newline="\n")

    # Only the root package's version line is rewritten; dependency entries
    # that happen to share a version string (e.g. "1.0.0") must stay put.
    lock_path = root / "src-tauri" / "Cargo.lock"
    pattern = re.compile(r'(name = "' + re.escape(package_name) + r'"\nversion = ")[^"]+(")')
    lock_text, count = pattern.subn(lambda m: m.group(1) + version + m.group(2), lock_path.read_text(), count=1)
    if count != 1:
        fail(f"could not find root package {package_name} in {lock_path}")
    lock_path.write_text(lock_text, newline="\n")

def main(argv):
    cmd, *args = argv
    preview = "--preview" in args
    no_step = "--no-step" in args
    args = [a for a in args if a not in ("--preview", "--no-step")]
    commands = {
        "rename": lambda *a: rename(*a, preview=preview, step=not no_step),
        "latest-json": lambda *a: latest_json(*a, preview=preview, step=not no_step),
        "check-version": check_version,
        "check-preview": check_preview,
        "set-version": set_version,
    }
    if cmd not in commands:
        fail(f"unknown command {cmd}")
    commands[cmd](*args)

if __name__ == "__main__":
    main(sys.argv[1:])
