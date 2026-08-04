# Web build → nginx static serving.
# CI runner is ubuntu-latest (amd64); no cross-arch needed.
# Built in release.yml upload-web-assets, loaded into xun via `docker save | podman load`.
FROM rust:latest AS builder

# Configure domestic mirrors for Debian APT (Tsinghua mirror)
RUN if [ -f /etc/apt/sources.list.d/debian.sources ]; then \
      sed -i 's#http://deb.debian.org#https://mirrors.tuna.tsinghua.edu.cn#g' /etc/apt/sources.list.d/debian.sources; \
    fi && \
    if [ -f /etc/apt/sources.list ]; then \
      sed -i 's#http://deb.debian.org#https://mirrors.tuna.tsinghua.edu.cn#g' /etc/apt/sources.list; \
    fi

# Configure domestic mirrors for rustup and cargo (rsproxy)
ENV RUSTUP_DIST_SERVER="https://rsproxy.cn"
ENV RUSTUP_UPDATE_ROOT="https://rsproxy.cn/rustup"

RUN mkdir -p /usr/local/cargo && \
    echo '[source.crates-io]' > /usr/local/cargo/config.toml && \
    echo 'replace-with = "rsproxy-sparse"' >> /usr/local/cargo/config.toml && \
    echo '[source.rsproxy-sparse]' >> /usr/local/cargo/config.toml && \
    echo 'registry = "sparse+https://rsproxy.cn/index/"' >> /usr/local/cargo/config.toml && \
    echo '[net]' >> /usr/local/cargo/config.toml && \
    echo 'git-fetch-with-cli = true' >> /usr/local/cargo/config.toml

# Install system dependencies (binaryen for wasm-opt)
RUN apt-get update && apt-get install -y binaryen && rm -rf /var/lib/apt/lists/*

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    rustup target add wasm32-unknown-unknown \
 && cargo install --locked trunk
WORKDIR /app
COPY . .
# wasm-opt=z (web/index.html data-wasm-opt) shrinks the .wasm for faster first-load.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    trunk build --release

FROM nginx:alpine
# Platform-agnostic static files; nginx default config serves /usr/share/nginx/html at :80.
# Bevy's WasmAssetReader fetches assets/shaders/*.wgsl over HTTP — trunk's copy-dir
# (web/index.html) ships them into dist/ so they're present here.
COPY --from=builder /app/dist /usr/share/nginx/html
EXPOSE 80
