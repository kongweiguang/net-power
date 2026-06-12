<!-- @author kongweiguang -->

# 产品规范

net-power 是面向开发、调试和日常网络切换的桌面代理管理工具。产品目标是让用户在图形界面里完成代理服务配置、运行、日志排查和系统代理切换，不再依赖多组命令行参数或分散配置文件。

## 产品目标

- 启动后直接进入工作台，用户可以新增、编辑、复制、删除、启动、停止和重启代理服务。
- 支持同时运行多个本地代理服务，并在仪表盘和列表中展示运行状态、连接数、字节统计和最近错误。
- 所有服务配置、规则、SSH Profile、系统代理配置档、运行事件和连接事件都持久化到 SQLite。
- 应用重启后恢复服务配置；当全局服务自动启动和单个服务 `auto_start` 同时开启时，应用启动后自动恢复对应服务。
- 系统代理、托盘常驻、开机启动、日志筛选和发布 smoke 形成完整桌面应用体验。

## 使用原则

- net-power 是工具型应用，不做 landing page；第一屏必须是可操作工作台。
- 界面以中文为主，HTTP、HTTPS、CONNECT、TCP、UDP、SSH、SOCKS5、known_hosts、keepalive 等专业名词可保留英文。
- 视觉风格保持浅色、克制、专业，优先清晰扫描和重复操作效率；主操作使用稳定蓝色，成功/危险/告警状态使用一致语义色。
- 布局必须跟随窗口宽度变化，最小 390px 宽度下仍能查看核心内容；窄屏下服务表格、日志详情和表单需要切换为更适合阅读的布局。
- 默认不保存 request/response body 和敏感 header；涉及抓包级 payload 的能力必须作为显式高级开关另行设计。

## 功能范围

已落地的核心能力：

- HTTP Reverse：固定上游转发、request/response header set/remove、JSON/form body rewrite、chunked 请求体解码、HTTP/1.1 keep-alive 顺序复用。
- HTTP Forward：普通 HTTP 代理和 HTTPS CONNECT，普通 HTTP 请求支持 keep-alive 顺序复用。
- TCP Forward 与 UDP Forward：本地监听到目标地址转发，包含连接统计、idle 清理和端口释放。
- SSH：Profile 加密存储、password/private key/agent 认证、local forward、remote forward、SOCKS5 动态代理、known_hosts 校验、跳板链、多级跳板和 remote 自动重连。
- System Proxy：Windows、macOS、Linux 平台系统代理设置/清理/状态读取，Linux 同步维护 shell 代理环境文件。
- App Shell：无边框自定义系统标题栏、系统托盘、关闭隐藏到托盘、开机启动应用、应用启动后的服务自动启动策略。
- 配置日志：服务事件、连接事件、按配置查看日志、级别/协议/时间/关键词筛选、流量详情面板和完整 meta JSON 查看。
- Packaging：Windows MSI/NSIS 打包、release/WebView/tray/installer smoke 验证脚本。

## 非目标

- 不提供用户账号、云同步、远程协作或多设备共享。
- 不继续扩展 YAML/JSON 文件作为主运行配置。
- 不让前端绕过 Rust Command 直接修改核心业务表。
- 不把敏感 header、SSH secret、request body 或 response body 默认写入日志。
- 不使用 Electron，也不保留 Go sidecar 作为代理核心。
- 不把所有代理类型塞进一个巨大 handler；代理协议、服务生命周期、数据库和 UI 状态需要保持分层。

## 信息架构

主工作台按“仪表盘、HTTP、端口转发、SSH、系统代理、设置”组织。HTTP、端口转发和 SSH 独立展示已保存配置列表，新增和编辑配置通过弹框完成。

| 页面 | 核心任务 |
| --- | --- |
| 仪表盘 | 查看运行概览、系统代理状态、最近错误和服务列表。 |
| HTTP | 查看、启动、修改、删除 HTTP Reverse / HTTP Forward 服务，并通过弹框维护 header/body rewrite 规则和超时参数。 |
| 端口转发 | 查看、启动、修改、删除 TCP/UDP 转发服务，并通过弹框维护目标地址、超时和 idle 清理。 |
| SSH | 管理 SSH Profile、local/remote/SOCKS5 隧道、known_hosts、跳板和 keepalive。 |
| 系统代理 | 选择 running HTTP Forward、手动目标或配置档并设置/清理系统代理。 |
| 设置 | 管理开机启动应用和应用启动后的服务自动启动策略。 |

## 数据与配置原则

- SQLite 是唯一持久化配置来源，数据库文件为 `proxy-tool.db`，默认放在 Tauri `app_data_dir()`。
- Rust 后端负责数据库读写、事务、校验、敏感数据引用和服务生命周期。
- 前端只负责展示、表单、筛选、事件监听和调用封装后的 Tauri Command。
- SSH 密码、私钥 passphrase 和未来 token 只以 secret id 关联业务记录，明文不回传到前端。
- 删除服务和 SSH Profile 使用软删除，避免历史日志引用失效。

## 交付标准

功能验收：

- 用户能在图形界面里完成代理服务配置，不需要命令行。
- 新增、编辑、复制、删除服务可保存到 SQLite，重启应用后配置仍存在。
- HTTP Reverse、HTTP Forward、CONNECT、TCP、UDP、SSH local/remote/SOCKS5 都能端到端转发。
- Header rewrite、JSON/form body rewrite、chunked body rewrite、keep-alive 和日志记录符合业务文档。
- Windows/macOS/Linux 系统代理后端具备设置、清理和状态读取能力。
- 托盘常驻、关闭隐藏、开机启动应用和服务自动启动策略不互相混淆。

质量验收：

- Rust：`cargo fmt --check`、`cargo check`、`cargo test`、`cargo clippy -- -D warnings` 通过。
- 前端：`npm test`、`npm run build`、`npm run test:visual` 通过。
- 打包：`npm run tauri -- build` 通过。
- Smoke：`npm run verify:release`、`npm run verify:webview`、`npm run verify:tray`、`npm run verify:installer` 或串行 `npm run verify:smoke` 通过。
- 外部 SSH 测试使用 WSL Docker 时，Docker 命令统一写成 `wsl.exe docker ...`。

## 发布前剩余验收

当前自动化和本机 smoke 已覆盖主要路径，但发布前仍保留人工验收项：

- 在干净 Windows 环境执行 MSI/NSIS 安装器完整交互流程。
- 在真实 Windows 托盘区域验证左键恢复、右键中文菜单和退出。
- 在真实 Windows 登录项验证开机启动开启、重新登录自动启动和关闭后清理。
- 在真实 macOS、GNOME Linux 和非 GNOME Linux 环境验证系统代理写入、清理和异常提示。

## 历史迁移对照

- 旧 `codex-proxy` 的 HTTP 反向代理、请求头改写、JSON/form 请求体改写，已沉淀为 HTTP Reverse、rewrite engine、HTTP 规则编辑器和对应 Rust 测试。
- 旧 `http-proxy` 的 HTTP/HTTPS 代理、TCP/UDP 转发和系统代理设置/清理，已沉淀为 HTTP Forward、CONNECT、TCP/UDP proxy core 和 System Proxy 后端。
- 新增 SSH 隧道能力已超出旧 Go 项目范围，当前包含 local、remote、SOCKS5、known_hosts、跳板链和 remote 自动重连。

## 文档归属

- 业务能力细节见 [biz/README.md](biz/README.md)。
- 架构边界见 [ARCHITECTURE.md](ARCHITECTURE.md)。
- SQLite 和数据校验见 [DATABASE.md](DATABASE.md)。
- Secret、日志隐私和系统权限见 [SECURITY.md](SECURITY.md)。
- 自动化测试、外部 SSH、smoke 和人工验收见 [TESTING.md](TESTING.md)。
- 完成态与剩余风险分别见 [completed.md](completed.md) 和 [in-progress.md](in-progress.md)。
