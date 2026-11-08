# 更新记录 / Changelog

## [0.1.0] - 2026-11-08

首个公开预览版 / First public preview.

### 新功能 / Features

- Windows x64 桌面版，以及自托管 Web / Docker 版。
- 直接选择已加入的频道，按 Telegram 文件夹筛选。
- 按关键词、媒体类型和日期搜索频道历史，支持继续加载。
- 下载视频、图片、音频和文件，支持手动队列、自动监听及历史补抓。
- 本地收藏夹支持分组、备注和 JSON 导入 / 导出。
- 本地文件独立管理，已下载的媒体可预览；Web 视频支持拖动进度。
- 自定义保存目录、媒体分类、并发和窗口关闭行为。
- 使用新的 tg-grab 标志，统一桌面图标、网页图标和应用内标识。

### 安装与升级 / Install and update

- 下载 `tg-grab_0.1.0_x64-setup.exe`，运行安装程序。支持简体中文和英语。
- 首次运行需填写自己的 Telegram API 凭证并登录。
- 覆盖安装会保留原有应用数据；升级前可在设置中导出收藏。
- `SHA256SUMS` 可用于核对安装包。此预览版尚未进行 Windows 代码签名。
- 本次提供 Windows x64 安装包；Linux amd64 提供 Docker 镜像。macOS / Linux 桌面版尚未验证。

Downloads require access through your own Telegram account. Collections are stored locally.
The Windows preview is unsigned. Download the installer from this repository's release page.
