<!-- @author kongweiguang -->

# 完成度审计

本文档把产品目标拆成可验证条目，记录当前证据和剩余缺口。它不是新的需求清单；实际产品范围仍以 [产品规范](README.md)、[业务能力](../business/README.md)、[架构说明](../engineering/architecture.md)、[数据库设计](../engineering/database.md)、[安全说明](../engineering/security.md) 和 [测试与验收](../engineering/testing.md) 为准。

## 审计结论

当前代码和文档已经覆盖原计划中的核心代理工具能力：SQLite 持久化、服务生命周期、本地 HTTP 工具服务、HTTP Reverse/Forward/CONNECT、TCP、UDP、SSH local/remote/SOCKS5、known_hosts、跳板链、SSH remote 自动重连、系统代理、日志、中文工作台、浅色/深色主题、响应式布局、托盘、开机启动和打包 smoke。

目标暂不应标记为最终完成，原因是仍有真实外部环境验收缺口：干净 Windows 安装交互、真实 Windows 托盘菜单、真实 Windows 登录启动项，以及真实 macOS/Linux 系统代理写入。这些缺口不等同于代码未实现，但当前仓库证据不足以证明发布前完整验收已经完成。

## 证据等级

| 等级 | 含义 |
| --- | --- |
| 已证明 | 有当前代码、自动化测试、smoke 脚本或文档记录可直接证明。 |
| 部分证明 | 代码已实现并有构造/解析/本机测试，但缺真实目标平台或真实系统交互证据。 |
| 未证明 | 没有足够证据证明要求满足。 |

## 功能能力

| 要求 | 当前证据 | 状态 |
| --- | --- | --- |
| SQLite 是唯一持久化配置来源 | [数据库设计](../engineering/database.md) 记录数据库位置、核心表、事务边界；`src-tauri/src/database.rs` 使用 `rusqlite`；测试覆盖 migration、CRUD、事务回滚和日志保留。 | 已证明 |
| 服务 CRUD、复制、软删除、启动、停止、重启、运行态查询 | [已完成能力](completed.md) 与 [测试与验收](../engineering/testing.md) 记录 ServiceManager 生命周期、端口冲突、失败态、停止释放端口测试。 | 已证明 |
| 本地 HTTP 工具服务 | [业务能力](../business/README.md) 与 [测试与验收](../engineering/testing.md) 记录工具服务创建、编辑、目录浏览/静态网站模式、接口响应、暂停保留配置和运行中编辑重启。 | 已证明 |
| HTTP Reverse | [业务能力](../business/README.md) 与 [测试与验收](../engineering/testing.md) 记录真实转发、header/body rewrite、chunked decode、Content-Length 和 keep-alive 测试。 | 已证明 |
| HTTP Forward 和 HTTPS CONNECT | [测试与验收](../engineering/testing.md) 记录完整 URL 代理、CONNECT 隧道字节复制和 keep-alive 顺序复用测试。 | 已证明 |
| TCP Forward | [测试与验收](../engineering/testing.md) 记录 socket 级双向复制、端口占用和释放后复用测试。 | 已证明 |
| UDP Forward | [测试与验收](../engineering/testing.md) 记录多客户端请求/响应映射、目标 socket 复用和 idle 清理测试。 | 已证明 |
| SSH Profile 加密存储 | [安全说明](../engineering/security.md) 记录 AES-GCM、OS keychain 和 secret 不回显；[测试与验收](../engineering/testing.md) 记录 SQLite 密文不含明文与 profile secret 校验测试。 | 已证明 |
| SSH local forward | [测试与验收](../engineering/testing.md) 记录 WSL Docker OpenSSH password auth 和 direct-tcpip bridge 外部测试。 | 已证明 |
| SSH remote forward | `src-tauri/src/proxy/ssh.rs` 提供 `run_ssh_remote_forward`；[测试与验收](../engineering/testing.md) 记录真实 OpenSSH remote forward 端到端测试。 | 已证明 |
| SSH SOCKS5 动态代理 | `src-tauri/src/proxy/ssh.rs` 提供 `run_ssh_socks_forward`；[测试与验收](../engineering/testing.md) 记录 SOCKS5 no-auth CONNECT 和真实 OpenSSH 动态代理测试。 | 已证明 |
| known_hosts strict/accept_new/insecure_skip | [安全说明](../engineering/security.md) 记录安全边界；[测试与验收](../engineering/testing.md) 记录缺失文件、未知主机、mismatch、首次写入和真实 OpenSSH mismatch 拒绝连接测试。 | 已证明 |
| SSH 跳板机/多级跳板 | [数据库设计](../engineering/database.md) 记录跳板链校验；[测试与验收](../engineering/testing.md) 记录 roundtrip、解析顺序、自引用、循环引用和删除保护测试；真实 WSL Docker OpenSSH 多级跳板测试已通过，覆盖连续两级跳板和最终 direct-tcpip 访问。 | 已证明 |
| SSH remote 自动重连 | [业务能力](../business/README.md) 记录指数退避；[测试与验收](../engineering/testing.md) 记录 remote forward 自动重连退避上限测试。 | 已证明 |
| Windows 系统代理 | [安全说明](../engineering/security.md) 和 [测试与验收](../engineering/testing.md) 记录 HKCU 注册表 set/status/clear gated 集成测试并恢复原值。 | 已证明 |
| macOS 系统代理 | `src-tauri/src/system_proxy.rs` 使用 `networksetup`；[测试与验收](../engineering/testing.md) 记录命令构造和状态解析测试。仍缺真实 macOS 写入/清理验收记录。 | 部分证明 |
| Linux 系统代理 | `src-tauri/src/system_proxy.rs` 使用 GNOME `gsettings` 并维护 `environment.d`；[测试与验收](../engineering/testing.md) 记录命令构造、解析和非 GNOME fallback。仍缺真实 GNOME/非 GNOME 写入验收记录。 | 部分证明 |
| 系统代理配置档 | [数据库设计](../engineering/database.md) 和 [测试与验收](../engineering/testing.md) 记录配置档 CRUD、target 解析、active 标记和前端组件测试。 | 已证明 |
| 日志和流量详情 | [业务能力](../business/README.md)、[安全说明](../engineering/security.md)、[测试与验收](../engineering/testing.md) 记录 service/connection events、筛选、meta 展示和默认不保存 body/敏感 header。 | 已证明 |
| 托盘常驻和关闭隐藏 | `src-tauri/src/tray.rs` 处理关闭隐藏；托盘菜单 id、中文文案和事件动作映射已有 Rust 单测；[测试与验收](../engineering/testing.md) 记录真实 release 窗口关闭隐藏到托盘 smoke。真实托盘区域左键恢复和右键菜单仍需人工验收。 | 部分证明 |
| 开机启动应用 | `src-tauri/src/commands.rs` 包装 Tauri autostart 插件；[测试与验收](../engineering/testing.md) 记录前端组件测试和服务自动启动选择 Rust 单测。真实 Windows 登录项仍需人工验收。 | 部分证明 |

## UI 与体验

| 要求 | 当前证据 | 状态 |
| --- | --- | --- |
| 界面以中文为主 | [产品规范](README.md) 固化中文原则；Workbench 测试覆盖关键中文标题、按钮和页面。 | 已证明 |
| 专业工具型视觉风格 | [已完成能力](completed.md) 记录工具台配色、浅色/深色/跟随系统主题和响应式布局；[测试与验收](../engineering/testing.md) 记录主题组件测试和 Playwright 视觉冒烟。 | 已证明 |
| 响应式跟随窗口大小 | [架构说明](../engineering/architecture.md) 记录 390px 最小宽度和断点；[测试与验收](../engineering/testing.md) 记录浏览器预览桌面/平板/390px 与真实 WebView 390px smoke。 | 已证明 |
| 启动后直接进入工作台 | [产品规范](README.md) 与 [项目 README](../../README.md) 记录应用启动后直接进入工作台；Playwright 工作台视觉冒烟覆盖页面入口。 | 已证明 |

## 质量与发布验证

| 要求 | 当前证据 | 状态 |
| --- | --- | --- |
| Rust 格式化、检查、测试、Clippy | [测试与验收](../engineering/testing.md) 维护 `cargo fmt --check`、`cargo check`、`cargo test`、`cargo clippy -- -D warnings` 的验证入口和覆盖范围。 | 已证明 |
| 前端测试与构建 | [测试与验收](../engineering/testing.md) 维护 `npm test`、`npm run build`、`npm run test:visual` 的验证入口和覆盖范围。 | 已证明 |
| Tauri 打包 | [测试与验收](../engineering/testing.md) 维护 `npm run tauri -- build` 的打包验证入口。 | 已证明 |
| Release/WebView/Tray/Installer smoke | [测试与验收](../engineering/testing.md) 维护 release、WebView、tray 和 installer smoke 的执行入口和覆盖范围。 | 已证明 |
| 文档沉淀 | [docs/README.md](../README.md) 已指向产品、业务、架构、数据、安全、测试、完成态和进行中文档作为稳定事实来源。 | 已证明 |

## 自动化验证来源

当前验证命令、gated 集成测试和 smoke 脚本以 [测试与验收](../engineering/testing.md) 为准。本文档只记录证据等级，不重复维护每次运行的测试数量，避免测试用例增删后产生过期信息。

## 剩余验收清单

这些项目需要真实环境或人工操作，当前不应被自动化测试结果替代：

- 在干净 Windows 机器上执行 MSI/NSIS 安装器完整交互流程。
- 在真实 Windows 托盘区域验证左键恢复、右键中文菜单显示/隐藏/退出；菜单结构和动作映射已有自动化测试，剩余缺口是 Windows shell 托盘区域真实交互。
- 在真实 Windows 登录项验证开机启动开启、重新登录自动启动和关闭后清理。
- 在真实 macOS 网络服务上验证 `networksetup` 设置、清理和异常恢复。
- 在真实 GNOME Linux 和非 GNOME Linux 环境验证桌面代理、`environment.d` 和 fallback 提示。

## 后续维护

- 每次补齐一项真实环境验收后，同步更新本文档、[in-progress.md](in-progress.md) 和 [测试与验收](../engineering/testing.md)。
- 若新增功能或测试脚本，先在对应业务/架构/测试文档记录事实来源，再更新本文档的证据等级。
