# 开发与发布 / Development and releases

Use Node.js 22.12+ and stable Rust. Windows desktop development also needs the
Microsoft C++ build tools and WebView2. Linux server development needs a C/C++ compiler.

```sh
npm ci
npm run check:version
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --locked --manifest-path src-tauri/Cargo.toml --no-default-features --features server --lib --bin tg-grab-server
```

`CI` checks the Web build, server tests, Windows desktop compilation and the Docker
image. `tests/container-smoke.py IMAGE` checks a locally built image with disposable
data and no Telegram credentials. The installer check runs only on an isolated
GitHub-hosted Windows runner.

## 发布版本 / Publish a version

1. Keep `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`,
   `src-tauri/Cargo.lock` and `src-tauri/tauri.conf.json` on the same version.
2. Add the version's release notes to `CHANGELOG.md`. Run the checks above.
3. Push the changes and wait for `CI` to pass.
4. Create and push the matching tag, for example `v0.1.0`.
5. `Release Windows preview` reruns checks, builds and tests the Windows installer,
   attaches the installer and `SHA256SUMS`, then publishes a preview release.
6. The release workflow dispatches `Publish Docker image`. It builds and tests the
   exact tagged source before pushing the version, `preview` and source-commit tags.

Tag-triggered releases default to previews. To publish a stable release, run the
release workflow manually with an existing tag and turn off the preview option.
Use a new version tag for new code; do not move published tags or replace released
installers. Existing published release assets are retained on workflow reruns.

The container workflow can also be run manually with an existing tag and either
the `preview` or `latest` channel. It currently publishes `linux/amd64` images.
Ensure the GHCR package is public so anonymous pulls work.

## 依赖与反馈 / Dependencies and feedback

Dependency updates arrive as grouped weekly pull requests. Review and run CI before
merging. The grammers revision is intentionally pinned; test its API and login,
browsing and downloads before changing it manually.

Use the issue templates for bugs and feature requests. Never attach account session
files, API Hash, login codes, passwords or server access tokens to public issues.
