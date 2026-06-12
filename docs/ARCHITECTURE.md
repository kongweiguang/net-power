<!-- @author kongweiguang -->

# 架构说明

net-power 是 Tauri v2 双进程桌面应用：React 负责工具型界面，Rust 负责代理核心、SQLite 访问、系统代理操作、敏感数据处理和后台服务生命周期。

## 当前模块

```text
src/
  api/                         Tauri Command 封装；previewFallback 仅服务浏览器视觉冒烟
  components/ErrorBoundary.tsx React 错误边界
  features/workbench/          主工作台
  styles/                      按工作台、表单、表格、响应式等职责拆分的样式入口
  types/                       前后端协议类型

src-tauri/
  migrations/                  SQLite schema 与索引
  src/app_state.rs             全局状态
  src/commands.rs              IPC Command 入口
  src/database.rs              SQLite repository 入口，组合 database/* 领域实现
  src/database/                migration、校验、行映射、服务配置读写和数据库测试
  src/manager.rs               服务生命周期 Facade
  src/proxy/http.rs            HTTP reverse/forward/CONNECT 入口
  src/proxy/http/              HTTP 代理专项回归测试
  src/proxy/rewrite.rs         Header 与 body rewrite
  src/proxy/tcp.rs             TCP forward
  src/proxy/udp.rs             UDP forward
  src/proxy/ssh.rs             SSH local/remote forward 与 SOCKS5 动态代理入口
  src/proxy/ssh/               SSH session、known_hosts、bridge 和工具测试
  src/crypto.rs                keyring + AES-GCM secret
  src/system_proxy.rs          系统代理
  src/tool_services.rs         HTTP 工具服务运行器、静态挂载和接口路由
  src/tray.rs                  系统托盘菜单与主窗口显隐行为
  tauri-plugin-autostart       系统登录启动项注册与清理
```

## 职责边界

| 层 | 职责 | 不应承担 |
| --- | --- | --- |
| React 前端 | 页面、表单、筛选、状态展示、事件监听、调用封装后的 Tauri command。 | 拼 SQL、直接写核心表、保存敏感明文、启动代理 socket。 |
| Tauri Command | IPC 边界、参数接收、错误映射、调用 repository/manager。 | 堆放代理协议细节、绕过业务事务。 |
| Database | migration、事务、查询、软删除、日志和配置持久化。 | 运行时 socket 状态管理。 |
| ServiceManager | 启动、停止、重启、端口冲突保护、取消令牌和运行计数器。 | 业务表 SQL 和 UI 状态。 |
| ToolServiceManager | 启动、暂停和查询已保存 HTTP 工具服务的运行态，校验目录、端口和响应配置。 | 持久化代理配置、系统代理设置和敏感信息处理。 |
| Proxy Core | HTTP/CONNECT/TCP/UDP/SSH local/SSH remote/SSH SOCKS5 的协议处理。 | 返回敏感字段或直接修改前端状态。 |
| Tray | 托盘菜单、主窗口显示/隐藏和关闭按钮后台常驻行为。 | 服务启停、数据库写入和代理协议处理。 |
| Autostart | 通过项目 Command 包装 Tauri autostart 插件，读取和切换系统登录启动项。 | 直接暴露插件 JS 权限、启动具体代理服务。 |

## 核心调用链

```text
React Workbench
  -> src/api invoke 封装
  -> Tauri command
  -> Database 或 ServiceManager
  -> proxy core / system_proxy / crypto
  -> SQLite 事件落库 + service://* 事件推送
```

服务页的工具服务走独立运行链路：`create_tool_service(input)` 写入 SQLite 的 `tool_services` / `tool_service_routes` 后立即启动，`start_tool_service(id)` 从 SQLite 读取已保存配置再交给 `ToolServiceManager.running`；`src/tool_services.rs` 只持有 TCP listener、取消令牌和请求计数。工具服务配置会持久化，暂停只移除运行态，不进入应用启动后的代理服务自动恢复队列。

## 服务生命周期

启动：

1. `start_service(id)` 从 SQLite 读取 `ServiceDetail`。
2. `ServiceManager` 校验 enabled、重复启动和监听端口冲突。
3. 创建 `CancellationToken`、计数器和后台任务。
4. `proxy::run_service` 根据 `ServiceKind` 分发到 HTTP/TCP/UDP/SSH。
5. 写 `service_events` 并推送 `service://status-changed`。

停止：

1. `stop_service(id)` 在 `ServiceManager` 写锁内取出运行任务并触发 cancel token。
2. Command 层释放 `ServiceManager` 写锁后，最多等待 5 秒让后台任务退出。
3. 停止失败时短暂重新加锁写回失败态，失败摘要保留真实服务类型和监听地址。
4. 停止成功时写事件、推送状态变化。

这个两阶段停止模型同样用于服务页的已保存工具服务，避免长时间停止等待阻塞其他运行态查询或服务操作。

## 运行状态模型

- SQLite 保存配置和历史事件。
- `ServiceManager.running` 保存当前运行任务、监听地址和计数器。
- `ToolServiceManager.running` 保存服务页 HTTP 工具服务的当前运行任务、取消令牌和请求计数；配置本身保存在 SQLite。
- 前端通过 `list_services`、`list_runtime_status` 和 `service://status-changed` / `service://log` 刷新 UI。

## 窗口与响应式

Tauri 主窗口使用自定义系统标题栏和可调整尺寸，默认以 1440x900 居中打开，启动后直接进入更舒展的完整桌面工作台；最小宽度为 390px，保证真实桌面窗口仍能进入窄宽度布局。React CSS 在 920px、760px 和 680px 断点下切换侧栏、服务表格和日志详情布局；`npm run verify:webview` 会启动真实 release WebView，先校验初始窗口尺寸，再把窗口调整到桌面宽度和 390px 窄宽度并截图验证非空渲染。托盘模块在主 WebView 窗口上直接绑定关闭拦截，关闭按钮会隐藏窗口并保持后台进程运行；`npm run verify:tray` 会用真实 release 窗口验证该行为。

## 已实现服务类型

- `http_reverse`：固定上游 URL，支持 request/response header、JSON/form body rewrite、chunked 请求体解码和 HTTP/1.1 keep-alive 顺序复用。
- `http_forward`：通用 HTTP proxy，支持普通 HTTP keep-alive 顺序复用和 HTTPS CONNECT。
- `tcp_forward`：本地 TCP 监听到目标 TCP。
- `udp_forward`：本地 UDP 监听到目标 UDP。
- `ssh_local`：类似 `ssh -L` 的本地端口转发，支持 password/private key/agent 认证。
- `ssh_remote`：类似 `ssh -R` 的远程端口转发，远端 SSH server 监听绑定地址后回连到本机目标。
- `ssh_socks`：类似 `ssh -D` 的 SOCKS5 动态代理，支持 no-auth、CONNECT、IPv4/domain/IPv6 目标地址。

服务页还支持持久化的本地工具 HTTP 服务：单个服务可同时挂载静态目录和多条接口路由，路由响应体支持内联内容或本地文件。它们面向本地开发快速文件/接口服务，暂停后保留配置，退出应用后配置仍可再次启动。

SSH Profile 可选择另一个 Profile 作为跳板，多级跳板通过引用链解析为第一跳到最终目标的连接顺序。`ssh2` 不能直接把 channel 作为新 session 的底层 socket，因此运行时用本地 loopback socket pair 桥接上一跳 `direct-tcpip` channel，确保每一跳仍执行 known_hosts 校验和认证。SSH remote forward 外层有自动重连循环，断线后按指数退避重新建立远程监听。

## 发布构建

`src-tauri/Cargo.toml` 配置 release profile 使用 LTO、单 codegen unit、strip 和 `panic = "abort"`，优先降低桌面端发布包体积。Tokio 只启用当前代码实际使用的 `fs`、`io-util`、`macros`、`net`、`rt-multi-thread`、`sync`、`time` 特性，避免 `full` 带入未使用能力。
