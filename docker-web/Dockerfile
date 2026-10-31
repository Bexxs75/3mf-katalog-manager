# syntax=docker/dockerfile:1
# Slim web image: katalog-server (read-only catalog server) plus the built web UI.
# Build from the repository root:  docker build -f docker-web/Dockerfile -t 3mfkatalog-web .

FROM node:22-bookworm-slim AS ui
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY index.html vite.config.ts tsconfig.json tsconfig.node.json postcss.config.js tailwind.config.js ./
COPY public ./public
COPY src ./src
RUN npm run build:web

FROM rust:1-bookworm AS server
WORKDIR /w
COPY Cargo.toml Cargo.lock ./
COPY .cargo ./.cargo
COPY crates ./crates
COPY src-tauri ./src-tauri
# The core embeds two JSON files that live next to the frontend sources.
COPY src/lib/autoTags.json src/lib/licensePresets.json ./src/lib/
# Only the server and the core are compiled; the Tauri crate is just a workspace member.
# Build liblzma statically: the final image is distroless and has no liblzma.so.
ENV LZMA_API_STATIC=1
RUN cargo build --release --locked -p katalog-server


FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=server /w/src-tauri/target/release/katalog-server /usr/local/bin/katalog-server
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
ENTRYPOINT ["/usr/local/bin/katalog-server"]
