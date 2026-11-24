# syntax=docker/dockerfile:1@sha256:4edf897a3ffa55b89f906fc8cc78afdb3f1834cc9c7083565e611a8a7d5fe99e
# Web image: katalog-server (read-only catalog server, with STEP preview) plus the built web UI.
# Build from the repository root:  docker build -f docker-web/Dockerfile -t 3mfkatalog-web .

# Base images are pinned by digest so a changed upstream tag cannot alter a build unnoticed;
# update the digests deliberately (tag kept for readability).
FROM node:22-bookworm-slim@sha256:4b150b48b4116e2a22f2c39cc27025fbf4f153e2078fd3b14d159fdb5c7279da AS ui
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY index.html vite.config.ts tsconfig.json tsconfig.node.json postcss.config.js tailwind.config.js ./
COPY public ./public
COPY src ./src
RUN npm run build:web

# Debian trixie is the first Debian with Open CASCADE 7.8 (the STEP reader needs 7.8 or newer).
FROM rust:1-trixie@sha256:223cc32ddca8f3ee90c436dbc94b6b5f052ea7f9653c5201e9612f08ad7d21b8 AS server
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake \
       libocct-foundation-dev libocct-modeling-data-dev libocct-modeling-algorithms-dev \
       libocct-data-exchange-dev libocct-ocaf-dev libocct-visualization-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /w
COPY Cargo.toml Cargo.lock ./
COPY .cargo ./.cargo
COPY crates ./crates
COPY src-tauri ./src-tauri
# The core embeds two JSON files that live next to the frontend sources.
COPY src/lib/autoTags.json src/lib/licensePresets.json ./src/lib/
# Only the server and the core are compiled; the Tauri crate is just a workspace member.
# liblzma is linked statically so the run-time stage needs no extra package for it.
ENV LZMA_API_STATIC=1
RUN cargo build --release --locked -p katalog-server --features step-preview
COPY docker-web/scripts/collect-occt-runtime.sh /usr/local/bin/
RUN collect-occt-runtime.sh /w/src-tauri/target/release/katalog-server /occt


# debian-slim instead of distroless: the OCCT libraries need X11/fontconfig/FreeImage from apt
# (the STEP translator links the visualisation toolkit even though nothing is drawn).
FROM debian:trixie-slim@sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2
LABEL org.opencontainers.image.title="3MF Katalog Manager (web)" \
      org.opencontainers.image.licenses="MIT AND LGPL-2.1-only WITH OCCT-exception-1.0" \
      org.opencontainers.image.source="https://github.com/Bexxs75/3mf-katalog-manager"
COPY --from=server /occt/apt-packages.txt /tmp/apt-packages.txt
# tzdata: chrono reads the local time zone for backup and rename dates (distroless shipped it).
# --no-upgrade keeps the base packages at the digest's versions.
RUN apt-get update \
    && xargs -a /tmp/apt-packages.txt apt-get install -y --no-install-recommends --no-upgrade tzdata \
    && rm -rf /var/lib/apt/lists/* /var/cache/apt/archives/* /var/cache/debconf/*-old \
       /var/log/apt /var/log/dpkg.log /tmp/apt-packages.txt
COPY --from=server /occt/lib/ /usr/local/lib/
COPY --from=server /occt/doc/ /usr/share/doc/occt-runtime/
COPY --from=server /w/src-tauri/target/release/katalog-server /usr/local/bin/katalog-server
# Licence texts of the application and its third-party components (checked by ci-smoke.sh).
COPY LICENSE THIRD-PARTY-LICENSES.md /licenses/
# Fail the build, not the first STEP preview, if a library is missing.
RUN ldconfig && ! ldd /usr/local/bin/katalog-server | grep 'not found'
COPY --from=ui /app/dist-web /srv/ui
# A non-loopback bind needs a host list (DNS rebinding); other names/IPs must be added via Compose.
# Same volume layout as the desktop image: /config (home of the app data) and /models.
ENV KATALOG_BIND=0.0.0.0:8080 \
    KATALOG_STATIC_DIR=/srv/ui \
    KATALOG_DB=/config/.local/share/com.thebexxs.mfkatalogmanager/catalog.db \
    KATALOG_ROOTS=/models \
    KATALOG_AUTH_FILE=/config/web-auth.json
EXPOSE 8080
USER 65532:65532
# The server binary asks its own /api/health (exit code 0/1), so the check needs no curl.
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD ["/usr/local/bin/katalog-server", "health"]
ENTRYPOINT ["/usr/local/bin/katalog-server"]
