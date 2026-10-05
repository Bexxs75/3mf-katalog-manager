# Local E2E runner. Ubuntu 24.04 matches the GitHub runner image, so WebKitGTK
# and WebKitWebDriver behave like in CI. Source and the app under test are
# mounted at run time; nothing of the project is baked into the image.
FROM ubuntu:24.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl xz-utils build-essential pkg-config libssl-dev \
      xvfb xauth dbus dbus-x11 webkit2gtk-driver libwebkit2gtk-4.1-0 \
      fonts-noto fonts-noto-color-emoji fontconfig libgtk-3-0 libfuse2t64 \
      procps x11-utils \
    && rm -rf /var/lib/apt/lists/*
# Node 22 (same major as CI); >= 22.13 so node:sqlite needs no flag (used to seed the catalog).
RUN curl -fsSL https://nodejs.org/dist/v22.14.0/node-v22.14.0-linux-x64.tar.xz | tar -xJ -C /usr/local --strip-components=1
# Rust is only needed once, to build tauri-driver.
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal \
    && cargo install tauri-driver --locked \
    && rm -rf /opt/cargo/registry /opt/cargo/git
# A window manager so the app window gets a real size and focus handling, as on a desktop.
RUN apt-get update && apt-get install -y --no-install-recommends openbox xdotool && rm -rf /var/lib/apt/lists/*
