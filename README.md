<p align="center">
  <img src="public/brand/icon.svg" width="88" height="88" alt="tg-grab 标志" />
</p>

<h1 align="center">tg-grab</h1>

Telegram 媒体下载与本地收藏工具，支持 Windows 桌面版和自托管 Web 版。

[English](README.en.md) · [下载 Windows 预览版](https://github.com/R051/tg-grab/releases) · [Docker 部署](#docker-部署) · [更新记录](CHANGELOG.md)

[![自动检查](https://github.com/R051/tg-grab/actions/workflows/ci.yml/badge.svg)](https://github.com/R051/tg-grab/actions/workflows/ci.yml)
[![发行版本](https://img.shields.io/github/v/release/R051/tg-grab?include_prereleases)](https://github.com/R051/tg-grab/releases)
[![MIT 许可](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Windows 安装

1. 从 [Releases](https://github.com/R051/tg-grab/releases) 下载 **Windows x64 安装包**，同页提供 `SHA256SUMS` 文件校验值。
2. 安装并打开软件，从左下角账号入口填写自己的 [Telegram API 凭证](https://my.telegram.org/apps)，完成登录。
3. 在频道页选择已加入频道，搜索并勾选内容下载；也可以在下载任务页粘贴消息链接。

首个版本为预览版，安装包尚未进行 Windows 代码签名。安装程序支持简体中文和英语。覆盖安装会保留应用数据；需要单独备份收藏时，可先从设置导出收藏元数据。

在 **设置 → 关于** 可查看作者、当前版本和仓库地址，或打开版本下载、问题反馈和更新记录。

## 主要功能

- 直接选择已加入的频道，并按 Telegram 文件夹筛选。
- 按关键词、视频 / 图片 / 音频 / 文件类型和指定日期搜索频道历史。
- 手动下载队列、自动监听新内容和历史补抓。
- 本地收藏夹支持分组、备注和 JSON 导入 / 导出。
- 本地文件独立管理，下载后的媒体可预览，Web 视频支持拖动进度。
- 自定义下载目录、媒体规则和分类方式；桌面关闭窗口可选择询问、托盘或退出。

![频道浏览](.github/assets/channels.jpg)

![本地收藏夹](.github/assets/collections.jpg)

*截图均使用演示数据，未展示真实 Telegram 账号内容。*

## Docker 部署

已发布的镜像为 **Linux amd64**：`ghcr.io/r051/tg-grab`，包含完整 Web 页面和服务端。

```sh
git clone https://github.com/R051/tg-grab.git
cd tg-grab
docker compose pull
docker compose up -d --no-build
docker compose logs tg-grab
```

打开 `http://localhost:8080`，输入日志中显示的访问令牌，再在页面中配置 Telegram 账号。从其他电脑访问时，将 `localhost` 换成服务器地址。

| 镜像标签 | 用途 |
| --- | --- |
| `preview` | 跟随最新预览版，Compose 默认使用此标签 |
| `0.1.2` | 固定到当前预览版 |
| `sha-<commit>` | 固定到某次源码提交 |

需要固定版本时，在本地 `.env` 中设置 `TG_GRAB_IMAGE=ghcr.io/r051/tg-grab:0.1.2`。更新当前使用的镜像时，执行 `docker compose pull` 和 `docker compose up -d --no-build`。

需要从源码构建时，执行：

```sh
docker compose up -d --build
```

配置、Telegram 会话、收藏和下载文件均保存在挂载的 `./data` 目录，升级时保留该目录。Web 版的下载目录指服务器路径，保存到浏览器设备需使用文件操作入口。向可信网络之外开放服务时，请配置 HTTPS 反向代理。

## 收藏与文件

收藏保存来源链接、分组、备注及可选的本地文件引用，不会自动下载媒体。导出内容不包含媒体文件、Telegram 会话、API Hash 或 Web 访问令牌；导入会合并条目，并保留已有备注。

删除收藏夹会将条目移到「未分类」；取消收藏不会删除文件。删除本地文件后，收藏仍保留来源链接。修改下载目录不会搬迁旧文件，旧目录的文件引用会显示不可用。

「停止加载」保留浏览结果；「关闭浏览」清空当前频道，两者都不会取消已经加入队列的下载。

## 开发与构建

需要 **Node.js 22.12+**、稳定版 Rust 和对应平台的编译工具。Windows 还需要 Microsoft C++ 构建工具及 WebView2。

```sh
npm ci
npm run check:version
npm test
npm run build
npm run tauri dev
npm run tauri build -- --bundles nsis
```

运行本地 Web 服务：

```sh
cargo run --release --manifest-path src-tauri/Cargo.toml --bin tg-grab-server --no-default-features --features server
```

自动检查、安装包验证、版本发布和依赖维护见 [CONTRIBUTING.md](CONTRIBUTING.md)。常规依赖会每周提出更新建议，固定的 grammers 提交单独验证后升级。

项目使用 Rust、Tauri v2 和 Vue 3，通过 [grammers](https://github.com/Lonami/grammers) 直接连接 Telegram MTProto，不依赖其他下载器的 API 服务。

## 反馈与说明

- [反馈问题或提出建议](https://github.com/R051/tg-grab/issues/new/choose)，请附版本及使用方式，并先隐去日志、截图中的凭证和个人信息。
- 需要自己的 API 凭证及账号访问权限，会话保存在本地，请勿使用泄露或共享的 API 凭证。
- 自动下载类型规则只影响监听和补抓，手动选中的媒体仍可下载。
- 短时限流自动等待；长时间等待会明确报错，可稍后重试。
- 当前桌面安装包只提供 Windows x64，macOS / Linux 桌面版尚未验证。
- 仅用于你有权访问的内容，尊重创作者、版权及 Telegram 服务条款，请勿未经许可传播他人内容。

## 许可

[MIT](LICENSE)
