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
# ASCII digits only (\d also matches other-script decimal digits) and no leading
# zeros other than a bare "0", matched with fullmatch so stray characters -
# including a trailing newline - can't sneak past the check.
PREVIEW_VERSION = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)-(0|[1-9][0-9]*)")
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
    m = PREVIEW_VERSION.fullmatch(version)
    if not m:
        fail(f"invalid preview version {version}")
    major, minor, patch, pre = (int(g) for g in m.groups())
    # The Windows MSI version field packs major/minor into 8 bits each and
    # patch into 16 bits; the pre-release build number is stored separately
    # but is also capped at 16 bits. A version outside these bounds would
    # fail (or silently truncate) when the MSI is built.
    if major > 255 or minor > 255 or patch > 65535 or pre > 65535:
        fail(f"preview version {version} is out of range for an MSI version (major/minor <= 255, patch/build <= 65535)")
    if manifest_path is None:
        return
    manifest_path = pathlib.Path(manifest_path)
    # Missing or empty: nothing published yet under this channel (a failed
    # download may still leave an empty file), so anything goes.
    if not manifest_path.is_file() or not manifest_path.read_text().strip():
        return
    try:
        published = json.loads(manifest_path.read_text())["version"]
        published_key = version_key(published)
    except (ValueError, KeyError, TypeError) as e:
        fail(f"cannot read the published preview manifest {manifest_path}: {e}")
    if version_key(version) < published_key:
        fail(f"version {version} is older than the published preview {published}")

def rc_version_key(version):
    # The release channel accepts the project's numeric SemVer subset only.
    # Reject leading zeros and trailing whitespace instead of normalizing them.
    numeric = r"(0|[1-9][0-9]*)"
    if not isinstance(version, str) or not re.fullmatch(
            rf"{numeric}\.{numeric}\.{numeric}(?:-{numeric})?", version):
        fail(f"invalid RC version {version!r}")
    return version_key(version)


def read_rc_json(path):
    try:
        data = json.loads(pathlib.Path(path).read_text(encoding="utf-8"))
        if not isinstance(data, dict):
            raise ValueError("expected a JSON object")
        return data
    except (OSError, ValueError) as e:
        fail(f"cannot read RC input {path}: {e}")


def check_rc(version, manifest_path=None):
    candidate = rc_version_key(version)
    # Only an explicitly absent argument means first run. A failed download or
    # broken existing manifest must never disable rollback protection.
    if manifest_path is not None:
        published = read_rc_json(manifest_path).get("version")
        if candidate < rc_version_key(published):
            fail(f"version {version} is older than the published RC {published}")


def rc_manifest(tag, source_path, release_path, output_path, current_path=None):
    if not isinstance(tag, str) or not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9]+)?", tag):
        fail(f"invalid release tag {tag!r}")
    version = tag[1:]
    check_rc(version, current_path)
    manifest = read_rc_json(source_path)
    release = read_rc_json(release_path)
    if release.get("tag_name") != tag or release.get("draft") is not False:
        fail("source must be the requested published release")
    if manifest.get("version") != version:
        fail("manifest version does not match release tag")
    assets = release.get("assets")
    if not isinstance(assets, list):
        fail("release asset inventory is missing")
    urls = {a.get("browser_download_url") for a in assets
            if isinstance(a, dict) and a.get("state") == "uploaded"
            and isinstance(a.get("browser_download_url"), str)}
    platforms = manifest.get("platforms")
    if not isinstance(platforms, dict) or not set(UPDATE_TARGETS).issubset(platforms):
        fail("manifest is missing updater platforms")
    prefix = f"https://github.com/{REPO}/releases/download/{tag}/"
    for platform, package in platforms.items():
        if not isinstance(package, dict):
            fail(f"invalid platform {platform}")
        signature = package.get("signature")
        url = package.get("url")
        if not isinstance(signature, str) or not signature.strip():
            fail(f"missing signature for {platform}")
        if not isinstance(url, str) or not url.startswith(prefix) or url not in urls:
            fail(f"URL for {platform} is not an asset of {tag}")
    # Preserve signatures, URLs, notes and timestamps byte for byte; only the
    # manifest's filename changes, while packages stay on the versioned release.
    shutil.copyfile(source_path, output_path)


def set_version(version, root="."):
    root = pathlib.Path(root)

    cargo_path = root / "src-tauri" / "Cargo.toml"
    prefix, _, rest = cargo_path.read_text(encoding="utf-8").partition("[package]")
    package, sep, suffix = rest.partition("\n[")
    name_match = re.search(r'^name\s*=\s*"([^"]+)"', package, re.M)
    if not name_match:
        fail(f"{cargo_path} has no [package] name")
    package_name = name_match.group(1)
    package = re.sub(r'^version\s*=\s*"[^"]+"', f'version = "{version}"', package, count=1, flags=re.M)
    cargo_path.write_text(prefix + "[package]" + package + sep + suffix, encoding="utf-8", newline="\n")

    conf_path = root / "src-tauri" / "tauri.conf.json"
    conf = json.loads(conf_path.read_text(encoding="utf-8"))
    conf["version"] = version
    # ensure_ascii=False keeps non-ASCII text (e.g. an umlaut in productName)
    # as literal UTF-8 instead of \uXXXX escapes, matching how the file
    # already reads before this rewrite.
    conf_path.write_text(json.dumps(conf, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")

    pkg_path = root / "package.json"
    pkg = json.loads(pkg_path.read_text(encoding="utf-8"))
    pkg["version"] = version
    pkg_path.write_text(json.dumps(pkg, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")

    # Only the root package's version line is rewritten; dependency entries
    # that happen to share a version string (e.g. "1.0.0") must stay put.
    lock_path = root / "src-tauri" / "Cargo.lock"
    pattern = re.compile(r'(name = "' + re.escape(package_name) + r'"\nversion = ")[^"]+(")')
    lock_text, count = pattern.subn(lambda m: m.group(1) + version + m.group(2), lock_path.read_text(encoding="utf-8"), count=1)
    if count != 1:
        fail(f"could not find root package {package_name} in {lock_path}")
    lock_path.write_text(lock_text, encoding="utf-8", newline="\n")

def main(argv):
    cmd, *args = argv
    preview = "--preview" in args
    no_step = "--no-step" in args
    args = [a for a in args if a not in ("--preview", "--no-step")]
    unknown = [a for a in args if a.startswith("--")]
    if unknown:
        fail(f"unknown option {' '.join(unknown)}")
    commands = {
        "rename": lambda *a: rename(*a, preview=preview, step=not no_step),
        "latest-json": lambda *a: latest_json(*a, preview=preview, step=not no_step),
        "check-version": check_version,
        "check-preview": check_preview,
        "rc-manifest": rc_manifest,
        "set-version": set_version,
    }
    if cmd not in commands:
        fail(f"unknown command {cmd}")
    commands[cmd](*args)

if __name__ == "__main__":
    main(sys.argv[1:])
