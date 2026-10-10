ARG BASE_IMAGE=ghcr.io/linuxserver/baseimage-kasmvnc:ubuntunoble
FROM ${BASE_IMAGE} AS app-extract

ARG APP_URL
ARG APP_VERSION
# Optional: pin the download; the build fails when the checksum differs.
ARG APP_SHA256=
# Keep download tools and the temporary AppImage out of the final stage.
RUN test -n "${APP_URL}" && test -n "${APP_VERSION}" \
    && apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates \
    && mkdir -p /opt/3mf-katalog \
    && curl --fail --location --show-error --proto '=https' --proto-redir '=https' "${APP_URL}" -o /tmp/3mf-katalog.AppImage \
    && { [ -z "${APP_SHA256}" ] || echo "${APP_SHA256}  /tmp/3mf-katalog.AppImage" | sha256sum -c -; } \
    && chmod +x /tmp/3mf-katalog.AppImage \
    && cd /opt/3mf-katalog \
    && /tmp/3mf-katalog.AppImage --appimage-extract \
    && cp -a squashfs-root/. . \
    && rm -rf squashfs-root /tmp/3mf-katalog.AppImage /var/lib/apt/lists/* \
    && chmod -R a+rX,go-w /opt/3mf-katalog \
    && chmod a+rx,go-w /opt/3mf-katalog/AppRun /opt/3mf-katalog/AppRun.wrapped

FROM ${BASE_IMAGE}
ARG APP_VERSION
LABEL org.opencontainers.image.version="${APP_VERSION}" \
      org.opencontainers.image.source="https://github.com/Bexxs75/3mf-katalog-manager" \
      org.opencontainers.image.description="Browser-accessible 3MF Katalog Manager with a supervised desktop application" \
      org.opencontainers.image.licenses="MIT"
ENV TITLE="3MF Katalog Manager" \
    LANG=de_DE.UTF-8 LC_ALL=de_DE.UTF-8 \
    THREEMF_CONTAINER=1 UMASK=022 \
    S6_BEHAVIOUR_IF_STAGE2_FAILS=2 S6_SERVICES_GRACETIME=26000 S6_KILL_GRACETIME=1000

RUN apt-get update \
    && apt-get install -y --no-install-recommends zenity locales x11-utils util-linux \
    && locale-gen de_DE.UTF-8 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=app-extract /opt/3mf-katalog /opt/3mf-katalog
COPY root/ /
COPY LICENSES.md /usr/share/doc/3mf-katalog/LICENSES.md
# Fail the build if upstream changes the authentication or desktop service layout.
RUN test -d /etc/s6-overlay/s6-rc.d/init-adduser \
    && test -d /etc/s6-overlay/s6-rc.d/init-kasmvnc/dependencies.d \
    && test -d /etc/s6-overlay/s6-rc.d/init-nginx/dependencies.d \
    && test -d /etc/s6-overlay/s6-rc.d/svc-kasmvnc \
    && touch /etc/s6-overlay/s6-rc.d/init-kasmvnc/dependencies.d/init-3mf \
    && touch /etc/s6-overlay/s6-rc.d/init-nginx/dependencies.d/init-3mf \
    && chmod +x /usr/local/bin/3mf-katalog /usr/local/libexec/3mf-* \
       /etc/s6-overlay/s6-rc.d/svc-3mf/run \
    && printf '%s\n' '#!/bin/sh' > /defaults/autostart \
    && chmod +x /defaults/autostart

# Give the app time to close SQLite/WAL before Docker sends SIGKILL.
STOPSIGNAL SIGTERM
HEALTHCHECK --interval=30s --timeout=8s --start-period=120s --retries=3 \
    CMD ["/usr/local/libexec/3mf-healthcheck"]
