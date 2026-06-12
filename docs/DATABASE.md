<!-- @author kongweiguang -->

# 数据库设计

net-power 使用 SQLite 作为唯一运行配置来源。前端通过 Tauri Command 调 Rust 后端，Rust 后端负责校验、事务、迁移和 secret 引用管理。

## 当前实现

- 数据库文件：`proxy-tool.db`。
- 默认位置：用户目录下的 `~/.net-power/proxy-tool.db`。
- 测试/验收可通过 `NET_POWER_APP_DATA_DIR` 覆盖数据目录；未设置时使用 `~/.net-power`。
- 迁移文件：`src-tauri/migrations/0001_initial.sql`、`0002_indexes.sql`、`0003_ssh_jump_profiles.sql`、`0004_tool_services.sql`。
- Rust 数据访问入口：`src-tauri/src/database.rs`，基于 `rusqlite` 和 `Mutex<Connection>`。
- 数据库领域实现：`src-tauri/src/database/`，按 migration、校验、行映射、服务配置读写和测试拆分。
- 默认设置：首次启动写入 `app.initialized`、`logs.retention_days`、`logs.max_rows`、`services.auto_start_enabled`。

## 访问边界

- Rust 后端是核心业务表的唯一写入入口。
- 创建、更新服务使用事务。
- 删除服务采用软删除，设置 `deleted_at`。
- 查询服务默认过滤 `deleted_at IS NULL`。
- 前端不直接执行 SQL。
- SSH 密码和 passphrase 只保存 secret id，不返回明文。

## 核心表

| 表 | 职责 |
| --- | --- |
| `app_settings` | 应用级设置。 |
| `services` | 所有代理服务的通用字段。 |
| `http_reverse_configs` | HTTP reverse 目标、超时、body rewrite 限制。 |
| `http_forward_configs` | HTTP forward 与 CONNECT 开关。 |
| `tcp_forward_configs` | TCP 转发目标与超时。 |
| `udp_forward_configs` | UDP 转发目标与 idle 清理。 |
| `ssh_profiles` | SSH 登录配置，不含明文 secret；`jump_profile_id` 表示可选跳板 Profile。 |
| `ssh_tunnel_configs` | SSH local/remote/socks 隧道配置。 |
| `http_header_rules` | HTTP request/response header set/remove 规则。 |
| `body_rewrite_rules` | JSON/form body rewrite 规则。 |
| `tool_services` | 服务页 HTTP 工具服务的名称、监听地址和静态目录配置。 |
| `tool_service_routes` | HTTP 工具服务的接口路由、响应状态、Content-Type 和响应来源。 |
| `system_proxy_profiles` | 系统代理配置档；UI 支持保存、编辑、启用和删除常用 host/port/bypass。 |
| `secrets` | AES-GCM 加密后的密码、passphrase、token。 |
| `service_events` | 服务启动、停止、错误和告警日志。 |
| `connection_events` | 连接级协议、目标、状态、耗时、字节统计。 |

## 数据校验

数据库 CHECK 负责第二道保护，Rust 层负责用户可读错误：

- 服务名不能为空。
- 监听端口和目标端口必须在 1-65535。
- HTTP reverse 必须有合法 `target_url`。
- HTTP forward 至少允许 HTTP 或 CONNECT。
- TCP/UDP/SSH local/SSH remote 必须有目标 host/port。
- SSH local、SSH remote、SSH SOCKS5 必须引用未删除的 SSH profile。
- SSH Profile 跳板引用必须指向未删除 profile，不能引用自己，不能形成循环，最多支持 8 级跳板链。
- Header/body rewrite 规则必须具备合法 action/type/path/value。
- 工具服务必须具备名称、监听主机和端口，并至少配置静态目录或一个接口；文件响应必须通过路径字段引用本地文件。

## 日志保留

当前已落库 `service_events` 和 `connection_events`；日志查询会合并服务事件与连接事件，并支持按 service、level、protocol 和关键词筛选。应用启动时会按 `logs.retention_days` 删除过期日志，并按 `logs.max_rows` 在两个日志表之间保留最新总行数，避免长期运行数据库无限增长。

## 当前测试覆盖

- migration 创建默认设置。
- 应用数据目录默认解析和环境变量覆盖。
- 服务 CRUD roundtrip 保留 header/body rules。
- 工具服务配置 roundtrip 保留静态目录、接口路由和暂停状态摘要。
- SSH local/remote/SOCKS 服务配置 roundtrip 保留 profile 引用和目标地址。
- SSH 跳板链 roundtrip、解析顺序、自引用/循环引用和删除保护。
- service_events 与 connection_events 合并查询、protocol 筛选和按服务清理。
- 服务更新事务失败回滚。
- 日志保留天数和最大总行数清理。

真实 SSH server 验收已覆盖 SSH local direct-tcpip、remote forward、SOCKS5 和 known_hosts mismatch；Windows 系统代理注册表 roundtrip 已覆盖 set/status/clear 和恢复。数据库层当前剩余风险主要依赖 macOS/Linux 系统代理和安装包人工验收。
