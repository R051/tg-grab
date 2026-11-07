# tg-grab

[中文说明](README.zh-CN.md)

A native desktop app to download and organize Telegram media locally — built with **Rust + Tauri v2 + Vue 3**.

## Features

- **📥 Manual downloads** — paste one or many `t.me` message links, with a live progress bar
- **📡 Watch mode** — subscribe to channels/groups and download new content using your media-type rules
- **🎞 History browser** — select joined channels by Telegram folder, search history by keyword/type/date, preview and select downloads
- **Local collections** — folders, notes, search, and JSON import/export for channel, media-message and local-file favorites
- **Local media library** — manage files separately from tasks, play downloaded video/audio, view photos and open sources
- **Download preferences** — custom storage location, channel/date and media-type folders, separate auto-download filters
- **Partial-file safety** — interrupted or rate-limited downloads are detected by size and re-downloaded cleanly
- **Gentle by design** — sequential queue (configurable 1–4 workers), automatic flood-wait handling
- **Bilingual UI** — 中文 / English, dark theme
- **True native app** — single installer, no Python/Node runtime required

## Collections and local files

Favorites store source links, notes, folders and optional local-file references. Saving a favorite does not download media. JSON exports contain collection metadata, excluding media bytes, API Hash, Telegram sessions and server tokens. Imports merge new entries while keeping existing notes.

Stopping history loading retains results; closing browsing clears the current channel. Neither action cancels queued downloads. Removing a folder moves its favorites to Unfiled; removing a favorite keeps the file. Deleting a local file keeps its favorite and source link.

Changing the download directory does not move existing files. Favorites from the previous root show as unavailable. In Web mode, this directory is on the server; use the file download action to save to the browser device. Auto-download type preferences apply to watchers and catch-up, while explicit manual selections remain downloadable. Directory rules apply to new downloads.

## Navigation and window behavior

The app opens on Channels, with five main sections: Channels, Collections, Downloads, Local files and Settings. Account and sign-in share one entry in the sidebar footer. The default desktop window is 1100 × 760; content lists scroll within their own regions.

Choose a joined channel directly, optionally filtered by your Telegram folders, or enter a link. History searches query Telegram by keyword, media type and optional day, with pagination through matching results. Videos, photos, audio and documents are supported.

Closing the desktop window asks whether to minimize to the tray or quit. You can cancel or remember the choice. Tray mode keeps downloads and monitoring running; quitting saves unfinished tasks for the next launch. Change this under Settings → Interface and background.

## Upstream capabilities

tg-grab connects directly to Telegram MTProto through [grammers](https://github.com/Lonami/grammers), pinned to `fa7692e49f301f16dc671c2f305ac1a32cad1a8e`. History, media downloads, thumbnails, live updates, dialog lists, Telegram folders and server-side message search are integrated. Message captions are shown in browsing results. Album grouping, message exports, uploads and forwarding are not implemented. All workflows remain subject to account access, permissions and Telegram rate limits.

## Build

Prerequisites: [Rust](https://rustup.rs/), Node.js 18+, and on Windows the [WebView2 runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (preinstalled on Win 10/11).

```bash
npm install
npm run tauri dev     # development
npm run tauri build   # release installer (NSIS/MSI)
```

## Web server + Docker

The same binary codebase also runs as a **web server** — access the full UI from any browser (NAS / home server / VPS):

```bash
# local run (frontend must be built once: npm run build)
cargo run --release --manifest-path src-tauri/Cargo.toml --bin tg-grab-server --no-default-features --features server
# → prints the access token, UI at http://0.0.0.0:8080

# or with Docker
docker compose up -d --build
```

- The access token is auto-generated on first boot and printed to the console (also stored in `data/config.json`). Enter it once in the browser — it is kept in localStorage.
- All downloads, config and the Telegram session live in the mounted `/data` volume.
- After pulling updates, run `docker compose up -d --build` again to rebuild both the Web UI and server. Keep the existing `data` directory.
- Put it behind a reverse proxy (Caddy/nginx) for HTTPS if exposing beyond your LAN.

## Getting API credentials

tg-grab speaks MTProto with your own Telegram account, so it needs **your** API credentials:

1. Open <https://my.telegram.org/apps> and sign in
2. Create an app and copy the **API ID** and **API Hash**
3. Enter them in tg-grab's Settings tab (stored locally in the app data folder, never synced)

> Don't use leaked/shared api_ids floating around the internet — Telegram actively blocks them, and your account may be affected.

## Restricted channels 🔓

tg-grab works with channels that have **Restrict Saving Content** enabled — the restriction is enforced by the official client UI, not by the MTProto protocol. Browse the channel's history and download as usual.

> Please respect content creators. Personal use is fine — mass redistribution is not what this tool is for.

## Notes & limits

- First run requires a one-time login (phone + code + optional 2FA password). The session is stored locally (`app data dir/tggrab.session`).
- Downloading content you can already see in your own client is fine; respect copyright and Telegram's ToS.
- Rate limits (FLOOD_WAIT) are handled automatically for short waits; long waits fail the task with a clear message — retry later.
- Linux/macOS builds are untested but should work via standard Tauri cross-platform flow.

## Disclaimer

This tool is for **personal use only**. Download content from channels you are a member of, and respect copyright and Telegram's Terms of Service. The authors are not responsible for misuse.

## License

[MIT](LICENSE)
