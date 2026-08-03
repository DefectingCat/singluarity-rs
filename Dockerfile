# Web build → nginx static serving.
# CI runner is ubuntu-latest (amd64); no cross-arch needed.
# Built in release.yml upload-web-assets, loaded into xun via `docker save | podman load`.
FROM rust:latest AS builder
RUN rustup target add wasm32-unknown-unknown \
 && cargo install --locked trunk
WORKDIR /app
COPY . .
# wasm-opt=z (web/index.html data-wasm-opt) shrinks the .wasm for faster first-load.
RUN trunk build --release

FROM nginx:alpine
# Platform-agnostic static files; nginx default config serves /usr/share/nginx/html at :80.
# Bevy's WasmAssetReader fetches assets/shaders/*.wgsl over HTTP — trunk's copy-dir
# (web/index.html) ships them into dist/ so they're present here.
COPY --from=builder /app/dist /usr/share/nginx/html
EXPOSE 80
