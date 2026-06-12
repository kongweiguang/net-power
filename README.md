<!-- @author kongweiguang -->

# net-power

net-power 是一个 Rust + Tauri + React 的桌面代理管理工具。应用启动后直接进入工作台，用户可以在界面里配置、启动、停止、重启和查看本地代理服务；Rust 后端负责代理核心、SQLite 持久化、服务生命周期、系统代理和敏感数据保护。

## 当前已实现

- SQLite `proxy-tool.db` 初始化、迁移和默认设置。
- 服务 CRUD、复制、软删除、启动、停止、重启、运行态查询。
- HTTP reverse proxy、request/response header 规则、JSON/form body rewrite，支持 chunked 请求体解码后改写和 HTTP/1.1 keep-alive 顺序复用。
- HTTP forward proxy 与 HTTPS CONNECT；普通 HTTP 请求支持同一客户端连接内的 keep-alive 顺序复用。
- TCP forward、UDP forward。
- SSH 配置加密存储、真实 OpenSSH 密码/私钥口令认证测试、SSH local forward、SSH remote forward、SSH SOCKS5 动态代理、known_hosts strict mismatch 校验、跳板机、多级跳板和 SSH remote 自动重连。
- Windows/macOS/Linux 系统代理设置、清理和状态读取；Linux 同步写入 shell 代理环境文件。
- 仪表盘、HTTP、端口转发、SSH、系统代理、设置工作台页面；HTTP、端口转发和 SSH 独立展示已保存配置列表。
- 服务编辑、SSH 配置编辑、结构化 Header/Body 规则编辑、HTTP/Forwarding/SSH 高级超时字段编辑，新增和编辑配置均通过弹框完成。
- 每个服务配置行可打开日志弹框，按级别、协议、时间范围、关键词筛选当前配置的运行日志并展示 meta。
- 日志弹框支持隐私安全的流量详情面板，查看连接方法、Host、路径、状态、耗时、字节和完整 meta JSON。
- 系统代理支持把 HTTP 正向代理服务和常用配置档合并成来源列表，右侧可启动并设置对应代理，也支持手动 host/port/bypass 目标。
- 工作台中文化、参考 Apple HIG 的浅色系统风格、专业工具型配色和响应式布局优化。
- 系统托盘菜单支持显示窗口、隐藏到托盘和退出；关闭主窗口默认隐藏到托盘，后台服务继续运行。
- 开机启动应用开关，支持通过系统登录启动项自动启动 net-power。
- 设置页支持通过 Tauri updater 从 GitHub Releases 检查、下载和安装桌面端更新，并在安装后尝试重启应用。
- GitHub Actions 已配置桌面端 CI 和 release workflow；推送 `v*.*.*` tag 可构建 Windows NSIS、macOS Intel/Apple Silicon DMG 和 Linux AppImage/DEB，并上传 updater `latest.json`。
- Tauri CSP 已收紧，敏感字段不回显到前端。
- Tauri 主窗口默认以 1280x720 居中打开，直接进入完整桌面工作台；仍允许缩到 390px 宽度，真实 WebView smoke 已覆盖启动尺寸、桌面宽度和窄宽度 resize 截图。
- 前端模型/组件 Vitest、Playwright 响应式视觉冒烟、Rust 单元测试、Clippy、Tauri Windows 打包已通过。
- Release smoke 可验证 `net-power.exe` 启动并在隔离 app data 下初始化 SQLite。
- Installer smoke 可验证 NSIS 安装包静默安装、安装后启动、SQLite 初始化、安装后 exe WebView 响应式截图、关闭隐藏到托盘和静默卸载。
- Tray smoke 可验证真实 release 主窗口完成 SQLite 初始化后，收到关闭请求会隐藏到托盘并保持进程存活。

## 仍待强化

- Linux 非 GNOME 桌面可能无法写入桌面代理，但仍会维护 `environment.d` shell 代理环境文件。
- 抓包详情默认不保存 request/response body 和敏感 header；如后续要做完整 payload 捕获，需要增加显式开关、脱敏和容量限制。
- 系统托盘关闭隐藏已由 `npm run verify:tray` 自动验收；仍建议在真实 Windows 托盘区域做一次左键恢复和右键菜单人工验收。
- 开机启动已接入官方 autostart 插件，仍建议在真实 Windows 登录后人工验收启动项生效和关闭后清理。
- Playwright 已覆盖桌面、平板和 390px 窄屏浏览器预览响应式冒烟；release WebView smoke 已覆盖真实 Tauri 窗口启动尺寸、桌面宽度和 390px 窄宽度 resize 截图。
- NSIS 安装包安装、启动、WebView 响应式截图、托盘关闭隐藏和卸载已由 `npm run verify:installer` 在当前 Windows 环境的临时目录覆盖；干净 Windows 机器安装流程仍建议发布前人工验收。

## 环境要求

- Node.js 与 npm。
- Rust stable toolchain。
- Tauri v2 开发依赖。Windows 需要 WebView2 运行时和常规 C++ 构建工具链。

## 安装依赖

```bash
npm install
```

## 开发运行

仅启动前端 Vite：

```bash
npm run dev
```

启动 Tauri 桌面应用开发模式：

```bash
npm run tauri dev
```

## 构建与验证

前端构建：

```bash
npm run build
```

前端单元测试：

```bash
npm test
```

前端响应式视觉冒烟：

```bash
npx playwright install chromium
npm run test:visual
```

Rust 检查：

```bash
cd src-tauri
cargo fmt --check
cargo test
cargo clippy -- -D warnings
```

构建 Tauri 安装包：

```bash
npm run tauri -- build
```

带 updater 签名产物的本地构建需要先设置 Tauri updater 私钥，再合并 CI 专用配置：

```powershell
$env:CI="true"
$env:TAURI_SIGNING_PRIVATE_KEY="$HOME\.tauri\net-power-release.key"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD="<本机私钥口令>"
npm run tauri -- build --config src-tauri/tauri.updater.conf.json
```

## GitHub Actions 发布与更新

- `.github/workflows/desktop-ci.yml`：在 `main`、`master`、`develop` push 和 pull request 上运行前端构建、Vitest、Rust fmt/test/clippy。
- `.github/workflows/release.yml`：在 `v*.*.*` tag 或手动触发时构建桌面端 release 产物，并由 `tauri-apps/tauri-action` 发布 GitHub Release 和 updater `latest.json`。
- updater endpoint 当前配置为 `https://github.com/kongweiguang/net-power/releases/latest/download/latest.json`；如果实际 GitHub 仓库不同，需要同步修改 `src-tauri/tauri.conf.json`。
- GitHub Secrets 需要配置 `TAURI_SIGNING_PRIVATE_KEY`，值为 Tauri updater 私钥内容；带口令私钥还需要配置 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。
- release workflow 会直接发布 Release；客户端设置页的“检查更新”会命中 GitHub latest release 中的 `latest.json`。

验证 release exe 启动和 SQLite 初始化：

```bash
npm run verify:release
```

验证 NSIS 安装包静默安装、启动、WebView 响应式截图、托盘关闭隐藏和卸载：

```bash
npm run verify:installer
```

验证真实 Tauri WebView 窗口启动、resize 截图和 SQLite 初始化：

```bash
npm run verify:webview
```

验证真实 release 主窗口关闭后隐藏到托盘且进程保持运行：

```bash
npm run verify:tray
```

串行运行 release、WebView、托盘和安装器 smoke：

```bash
npm run verify:smoke
```

## 文档入口

- [docs/README.md](docs/README.md)：文档索引。
- [docs/PRODUCT.md](docs/PRODUCT.md)：产品目标、范围、非目标和交付标准。
- [docs/completed.md](docs/completed.md)：已完成能力。
- [docs/COMPLETION_AUDIT.md](docs/COMPLETION_AUDIT.md)：完成证据、部分证明项和剩余验收。
- [docs/in-progress.md](docs/in-progress.md)：进行中和剩余风险。
- [docs/biz/README.md](docs/biz/README.md)：业务能力说明。
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)：架构和模块边界。
- [docs/DATABASE.md](docs/DATABASE.md)：SQLite 持久化设计。
- [docs/SECURITY.md](docs/SECURITY.md)：权限、CSP、secret 和日志隐私。
- [docs/TESTING.md](docs/TESTING.md)：测试和人工验收口径。
- [plan.md](plan.md)：历史计划迁移索引。
