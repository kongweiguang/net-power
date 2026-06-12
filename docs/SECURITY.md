<!-- @author kongweiguang -->

# 安全策略

net-power 是本地代理工具，会处理网络流量、系统代理、SSH 凭据和本地持久化数据。安全策略以最小权限、后端校验、敏感数据加密和日志脱敏为核心。

## 当前配置

当前 capability：

```json
[
  "core:default",
  "core:window:allow-close",
  "core:window:allow-minimize",
  "core:window:allow-start-dragging",
  "core:window:allow-toggle-maximize",
  "opener:default",
  "process:default",
  "updater:default"
]
```

当前 CSP 已从 `null` 收紧为：

```text
default-src 'self';
connect-src 'self' ipc: http://ipc.localhost;
img-src 'self' asset: https://asset.localhost;
style-src 'self' 'unsafe-inline';
script-src 'self'
```

## Capabilities 原则

- 只启用当前功能真正需要的权限。
- 新增插件时同步说明用途、权限 ID 和作用域。
- 文件、shell、通知、更新等权限不得为了“以后可能用”提前开启。
- 当前系统代理能力由 Rust 后端调用平台 API/命令实现，没有向前端暴露 shell 权限。
- 开机启动能力通过 Rust Command 包装 Tauri autostart 插件，未向前端开放 `plugin:autostart` JS 权限。
- 应用更新能力仅开放 `updater:default` 和 `process:default`，用于检查/安装更新和安装后重启，不开放 shell 或文件系统权限。

## 自动更新安全边界

- Tauri updater 公钥写入 `src-tauri/tauri.conf.json`，用于校验 GitHub Release 上的更新包签名。
- updater 私钥不得提交到仓库；GitHub Actions release workflow 只通过 `TAURI_SIGNING_PRIVATE_KEY` secret 在构建时签名。
- 当前 release 私钥使用口令保护，CI 需要同时配置 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secret。
- CI 专用 `src-tauri/tauri.updater.conf.json` 只开启 `bundle.createUpdaterArtifacts=true`，避免普通本地 build 在未设置私钥时被强制签名。
- release workflow 会直接发布 Release，让 updater endpoint 能命中 GitHub latest release；发布前需要先在本地或独立测试仓库验证安装包和 `latest.json`。
- 如果 GitHub 仓库迁移，需要同步修改 updater endpoint，否则客户端无法读取新的 `latest.json`。

## Command 输入校验

Rust command 和 database 层会重新校验前端输入：

- 服务名不能为空。
- 端口必须在 1 到 65535。
- 同一运行时监听地址不允许同时运行两个服务；SSH remote 使用 `SSH profile + remote_bind_host + remote_bind_port` 作为远程冲突域。
- HTTP reverse 的 `target_url` 必须合法。
- TCP/UDP/SSH local/SSH remote 目标 host 与 port 必须合法。
- SSH local、SSH remote 和 SSH SOCKS5 必须引用有效 SSH profile；SSH remote 必须提供远程绑定 host/port；SSH SOCKS5 不接受固定目标配置，目标由客户端 CONNECT 请求决定。
- SSH Profile 的跳板引用必须指向有效 Profile，数据库层阻止自引用、循环引用和超过 8 级的异常链。
- 系统代理操作必须解析为明确的 host/port 配置。

前端校验只用于交互体验，不能作为安全边界。

## 数据目录覆盖

默认数据库位于 Tauri `app_data_dir()`。`NET_POWER_APP_DATA_DIR` 仅作为测试/验收覆盖入口使用，release smoke 会把它指向临时目录，验证后清理；普通运行不需要设置该变量。

## 敏感数据

不得明文保存：

- SSH 密码。
- SSH 私钥 passphrase。
- HTTP Authorization、Proxy-Authorization。
- Cookie、Set-Cookie。
- 未来可能加入的 token 或 API key。

当前实现：

- `secrets` 表保存 `ciphertext`、`nonce`、`kind` 和名称。
- 使用 AES-256-GCM 加密。
- 主密钥存放在 OS keychain。
- keychain 不可用时返回明确错误，不降级为明文保存。
- 读取 SSH profile 时只返回 `hasPassword`、`hasPassphrase`、`jumpProfileId` 等非 secret 状态。

## SSH 安全边界

- `known_hosts_mode=strict`：要求 known_hosts 文件存在且 host key 匹配。
- `known_hosts_mode=accept_new`：首次连接会写入 known_hosts，后续 mismatch 会失败。
- `known_hosts_mode=insecure_skip`：跳过 host key 校验，仅建议本地测试使用。
- strict/accept_new 的缺失文件、未知主机、匹配和 mismatch 判定已有自动化测试；strict 模式也已通过真实 OpenSSH host key 与错配 known_hosts 文件验证拒绝连接。
- SSH local forward 和 SSH SOCKS5 每个本地客户端使用独立 SSH session，避免多通道阻塞互相影响；SSH remote forward 使用单个远程监听 session 接收通道，并要求远端 sshd 允许 TCP forwarding，跨主机访问远程绑定端口时还需要服务端允许 GatewayPorts。
- 多级跳板会对每一跳分别执行 known_hosts 校验和认证；跳板链只保存 Profile id，不复制密码或 passphrase 明文。
- SSH remote forward 断线后自动重连，重连失败只写入摘要错误和退避时间，不记录 secret 或完整私钥路径。
- SSH SOCKS5 当前仅支持 no-auth + CONNECT，不在 SOCKS 层额外保存用户名、密码或目标请求体。

## 日志与隐私

默认可以记录：

- 服务 ID、协议、目标地址、方法、host、path。
- 状态码、耗时、bytes in/out。
- 启动、停止、错误和告警消息。

默认不得记录：

- request body。
- response body。
- Authorization。
- Proxy-Authorization。
- Cookie。
- Set-Cookie。
- SSH 密码、passphrase、私钥内容。

SSH SOCKS5 连接日志只记录 `SOCKS5 CONNECT`、客户端地址、目标地址、耗时和字节数，不记录 SOCKS 之后承载的应用层请求内容。

SSH remote forward 连接日志只记录 `REMOTE`、远程绑定地址、本地目标地址、耗时和字节数；`ssh2` 当前不暴露远端客户端源地址，因此不会记录不存在或不可靠的客户端地址。

配置日志弹框的“流量详情”只展示已经落库的连接元数据和完整 meta JSON，不额外读取或保存 request/response body。完整 payload 抓包如果后续加入，必须以显式开关、脱敏策略和容量限制为前提。

## 系统代理风险

Windows 系统代理写入当前用户注册表：

```text
HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings
```

当前 UI 支持把 running HTTP forward 服务、手动目标或已保存系统代理配置档设置为系统代理目标，并提供清理按钮。macOS 系统代理通过 `networksetup` 修改当前用户可见网络服务的 HTTP/HTTPS proxy。Linux 优先通过 GNOME `gsettings` 修改桌面代理，并写入 `$XDG_CONFIG_HOME/environment.d/net-power-proxy.conf` 或 `~/.config/environment.d/net-power-proxy.conf`，供新的 shell/登录会话加载代理环境变量。

Windows 注册表写入、状态读取和清理已有 gated 集成测试覆盖，测试会恢复执行前的 `ProxyEnable`、`ProxyServer` 和 `ProxyOverride`。

## 托盘与后台常驻

系统托盘只负责窗口显示、隐藏和应用退出，不新增前端 shell 权限，也不改变代理服务的数据库配置。关闭主窗口会隐藏到托盘，后台代理服务继续运行；需要完全退出应用时使用托盘菜单的退出项。

## 开机启动

开机启动只写入或清理当前平台的系统登录启动项，用于自动启动 net-power 应用本身。代理服务是否随应用启动由 `services.auto_start_enabled` 和单个服务的 `auto_start` 控制，两者独立，避免开启系统启动项后无意启动所有代理服务。

## 发布前安全检查

- capability 只包含已实现功能需要的权限。
- CSP 不为 `null`。
- secret 加密解密测试通过。
- SSH profile API 不返回 secret 明文。
- Windows 系统代理 gated 集成测试通过；macOS/Linux 系统代理设置和清理有人工验收记录。
- 打包产物不会把开发密钥、测试凭据或本地数据库带入安装包。
