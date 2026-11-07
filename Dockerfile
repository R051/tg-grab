# ── stage 1: frontend build ──
FROM node:22-alpine AS frontend
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY index.html vite.config.ts tsconfig.json ./
COPY public ./public
COPY src ./src
RUN npm run build

# ── stage 2: server build (no tauri — pure tokio + axum) ──
FROM rust:1-slim AS server
WORKDIR /build
RUN apt-get update && apt-get install -y --no-install-recommends git pkg-config build-essential     && rm -rf /var/lib/apt/lists/*
COPY src-tauri/Cargo.toml src-tauri/Cargo.lock ./
COPY src-tauri/build.rs build.rs
COPY src-tauri/src ./src
RUN cargo build --release --no-default-features --features server

# ── stage 3: runtime ──
FROM debian:bookworm-slim
WORKDIR /app
COPY --from=server /build/target/release/tg-grab-server /app/tg-grab-server
COPY --from=frontend /app/dist /app/dist
ENV TG_GRAB_DATA_DIR=/data     TG_GRAB_DIST=/app/dist     TG_GRAB_PORT=8080
VOLUME /data
EXPOSE 8080
CMD ["/app/tg-grab-server"]
