FROM rust:1-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \
    libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev \
    patchelf pkg-config libssl-dev python3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /work
COPY src-tauri ./src-tauri
COPY LICENSE THIRD-PARTY-LICENSES.md ./
COPY src/lib/autoTags.json ./src/lib/autoTags.json
RUN mkdir -p dist && printf '<html></html>' > dist/index.html
WORKDIR /work/src-tauri
RUN cargo test --locked --no-default-features --lib --no-run --message-format=json > /tmp/artifacts.json \
    && python3 -c 'import json, shutil; rows = [json.loads(line) for line in open("/tmp/artifacts.json")]; paths = [r["executable"] for r in rows if r.get("reason") == "compiler-artifact" and r.get("profile", {}).get("test") and r.get("executable")]; assert len(paths) == 1, paths; shutil.copy2(paths[0], "/usr/local/bin/printer-tests")'
ENTRYPOINT ["/usr/local/bin/printer-tests"]
