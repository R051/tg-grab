# tg-grab

[English](README.md)

Telegram 频道/群组视频下载器 —— 原生桌面应用，基于 **Rust + Tauri v2 + Vue 3**。

## 功能

- **📥 手动下载** —— 粘贴一个或多个 `t.me` 消息链接，实时进度条
- **📡 自动监听** —— 订阅频道/群组，新视频发布即自动下载
- **🎞 频道历史** —— 分页浏览频道全部历史视频，勾选批量下载
- **残缺文件防护** —— 按大小校验，限流/中断留下的半截文件自动重下
- **温和设计** —— 串行队列（可调 1–4 并发）、限流自动等待
- **双语界面** —— 中文 / English，暗色主题
- **真原生应用** —— 单安装包，无需 Python/Node 运行时

## 构建

前置要求：[Rust](https://rustup.rs/)、Node.js 18+，Windows 需要 [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/)（Win10/11 自带）。

```bash
npm install
npm run tauri dev     # 开发
npm run tauri build   # 发布安装包（NSIS/MSI）
```

## Web 服务 + Docker

同一套代码也能以 **Web 服务**方式运行 —— 任何浏览器都能远程操作完整界面（NAS / 家庭服务器 / VPS）：

```bash
# 本地运行（需先构建前端：npm run build）
cargo run --release --manifest-path src-tauri/Cargo.toml --bin tg-grab-server --no-default-features --features server
# → 启动时打印访问令牌，界面在 http://0.0.0.0:8080

# 或者用 Docker
docker compose up -d
```

- 访问令牌首次启动自动生成并打印到控制台（同时存于 `data/config.json`），浏览器里输入一次即可（保存在 localStorage）
- 下载、配置、Telegram 会话全部保存在挂载的 `/data` 卷
- 如需公网暴露，请加反向代理（Caddy/nginx）上 HTTPS

## 获取 API 凭证

tg-grab 使用你自己的 Telegram 账号走 MTProto 协议，需要**你自己的** API 凭证：

1. 打开 <https://my.telegram.org/apps> 登录
2. 创建应用，复制 **API ID** 和 **API Hash**
3. 在 tg-grab 的「设置」页填入（仅保存在本机应用数据目录，不会同步）

> 不要使用网上流传的公共 api_id —— Telegram 在主动封禁它们，可能连累你的账号。

## 受限频道 🔓

tg-grab 支持**开启了「禁止保存内容」**的频道——该限制由官方客户端界面执行，不影响 MTProto 协议层的文件传输。正常浏览历史、下载即可。

> 请尊重内容创作者。个人使用没有问题，但请勿批量搬运到其他频道。

## 说明

- 首次运行需登录一次（手机号 + 验证码 + 可选两步验证密码），会话保存在本地应用数据目录（`tggrab.session`）
- 下载你自己客户端里能看到的内容没有任何问题；请遵守版权与 Telegram 服务条款
- 短时限流自动等待；长时限流任务会失败并给出明确提示，稍后重试即可
- Linux/macOS 构建未测试，理论上走标准 Tauri 流程即可

## 免责声明

本工具仅供**个人使用**。请仅下载你已加入的频道中的内容，并遵守版权法规和 Telegram 服务条款。作者不对滥用行为承担责任。

## 许可

[MIT](LICENSE)
