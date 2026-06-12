<!-- @author kongweiguang -->

# 测试与验收

本文档记录 net-power 的测试分层、当前可运行检查和发布前人工验收材料。

## 当前已验证命令

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
cargo test -- --nocapture
cargo clippy -- -D warnings
```

Tauri 打包：

```bash
npm run tauri -- build
```

Release smoke：

```bash
npm run verify:release
```

Installer smoke：

```bash
npm run verify:installer
```

WebView smoke：

```bash
npm run verify:webview
```

Tray smoke：

```bash
npm run verify:tray
```

串行 smoke：

```bash
npm run verify:smoke
```

当前测试覆盖按能力归档，避免在本文档重复维护每个测试函数名：

- Rust 覆盖数据库迁移与默认设置补齐、服务配置事务、工具服务创建/编辑、ServiceManager/ToolServiceManager 生命周期、HTTP/TCP/UDP/SSH 代理核心、系统代理后端构造、日志查询和日志保留。
- 前端 Vitest 覆盖 Workbench 表单校验、协议转换、编辑回填、页面筛选、工具服务编辑、静态目录模式、复制地址、日志筛选、系统代理配置、主题切换、应用更新入口和关于信息。
- 外部 gated 测试覆盖 Windows 系统代理真实注册表 roundtrip，以及 WSL Docker OpenSSH 的密码登录、私钥口令登录、local/remote/SOCKS5、两级跳板链和 strict known_hosts mismatch。
- Playwright 视觉冒烟覆盖浏览器预览模式下的仪表盘、本地服务、网络转发、SSH、系统代理、设置页面和配置日志弹框，并在桌面、平板和 390px 窄屏 viewport 检查关键中文内容、流量详情面板和页面级横向溢出。
- Release/WebView/Tray/Installer smoke 覆盖 release exe 启动、隔离 SQLite 初始化、真实 WebView resize 截图、关闭隐藏到托盘、NSIS 静默安装、安装后启动和静默卸载。

## 当前缺口

- Playwright 已自动复核桌面、平板和 390px 窄屏浏览器预览布局；真实 Tauri release WebView 已自动复核桌面宽度和 390px 窄宽度 resize 截图。
- 系统托盘关闭隐藏已有自动化 smoke，托盘中文菜单项和菜单事件动作映射已有 Rust 单测；仍需要在真实 Windows 托盘区域人工验收托盘恢复、右键菜单和退出行为。
- 应用内服务自动启动选择已有 Rust 单测覆盖；系统登录启动仍需要在真实 Windows 登录项人工验收开启、登录自动启动和关闭后清理。
- macOS/Linux 系统代理需要人工验收平台实际写入、清理和异常恢复。
- Release exe 启动和 SQLite 初始化已由 smoke 脚本覆盖；NSIS 安装/启动/WebView 响应式截图/关闭隐藏/卸载已在当前 Windows 临时目录自动验收，仍建议在干净 Windows 环境人工验收安装器完整交互流程。
- GitHub release workflow 已写入仓库，仍需要在远端仓库配置 `TAURI_SIGNING_PRIVATE_KEY` 和 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secrets，并通过一次 `v*.*.*` tag 构建验证三平台产物、`latest.json` 和客户端检查更新链路。

## 外部 SSH 集成测试

默认 `cargo test` 会跳过外部 SSH 测试；显式设置环境变量后才会连接真实 SSH server。当前已在 WSL Docker 的 `lscr.io/linuxserver/openssh-server:latest` 上通过以下场景：

- `external_ssh_password_auth_and_direct_tcpip_bridge`：密码认证成功，并通过 SSH `direct-tcpip` 读取远端 SSH banner。
- `external_ssh_private_key_passphrase_auth`：使用带 passphrase 的 RSA 私钥认证成功。
- `external_ssh_remote_forward_reaches_local_target`：通过 OpenSSH remote forward 远程监听端口，回连到本机 TCP 目标并完成双向数据传输。
- `external_ssh_socks5_dynamic_proxy_reaches_remote_target`：通过 SOCKS5 no-auth CONNECT，经 SSH `direct-tcpip` 访问远端 TCP 服务。
- `external_ssh_jump_chain_reaches_target_through_openssh`：通过两级 SSH 跳板链逐跳认证，再从最终 session 建立 `direct-tcpip` 读取远端 SSH banner。
- `external_ssh_strict_known_hosts_rejects_changed_host_key`：使用真实 OpenSSH host key 和故意错配的 known_hosts 文件，验证 strict 模式拒绝连接。

测试容器需要启用密码登录、`AllowTcpForwarding yes` 和 `GatewayPorts yes`，并为 libssh2/OpenSSH 兼容性追加常见 KEX/hostkey 算法。示例 PowerShell 命令：

```powershell
wsl.exe docker run --name net-power-sshd-test -d `
  -p 127.0.0.1:22222:2222 -p 127.0.0.1:29080:29080 `
  -e PUID=1000 -e PGID=1000 -e TZ=Asia/Shanghai `
  -e PASSWORD_ACCESS=true -e USER_NAME=netpower -e USER_PASSWORD=netpower-pass `
  lscr.io/linuxserver/openssh-server:latest

wsl.exe docker exec net-power-sshd-test sh -lc "grep -q '^AllowTcpForwarding' /config/sshd/sshd_config && sed -i 's/^AllowTcpForwarding .*/AllowTcpForwarding yes/' /config/sshd/sshd_config || printf '\nAllowTcpForwarding yes\n' >> /config/sshd/sshd_config; grep -q '^GatewayPorts' /config/sshd/sshd_config && sed -i 's/^GatewayPorts .*/GatewayPorts yes/' /config/sshd/sshd_config || printf 'GatewayPorts yes\n' >> /config/sshd/sshd_config; printf '\nKexAlgorithms +diffie-hellman-group14-sha256,diffie-hellman-group14-sha1\nHostKeyAlgorithms +ssh-rsa,rsa-sha2-512,rsa-sha2-256\nPubkeyAcceptedAlgorithms +ssh-rsa,rsa-sha2-512,rsa-sha2-256\n' >> /config/sshd/sshd_config"
wsl.exe docker restart net-power-sshd-test
```

密码、local forward、remote forward、SOCKS5 和 strict known_hosts mismatch 验证：

```powershell
cd src-tauri
$env:NET_POWER_SSH_TEST='1'
$env:NET_POWER_SSH_TEST_HOST='127.0.0.1'
$env:NET_POWER_SSH_TEST_PORT='22222'
$env:NET_POWER_SSH_TEST_USER='netpower'
$env:NET_POWER_SSH_TEST_PASSWORD='netpower-pass'
$env:NET_POWER_SSH_TEST_TARGET_HOST='127.0.0.1'
$env:NET_POWER_SSH_TEST_TARGET_PORT='2222'
$env:NET_POWER_SSH_TEST_SOCKS_TARGET_HOST='127.0.0.1'
$env:NET_POWER_SSH_TEST_SOCKS_TARGET_PORT='2222'
$env:NET_POWER_SSH_TEST_REMOTE_PORT='29080'
$env:NET_POWER_SSH_TEST_REMOTE_BIND_HOST='0.0.0.0'
$env:NET_POWER_SSH_TEST_REMOTE_CONNECT_HOST='127.0.0.1'
$env:NET_POWER_SSH_TEST_JUMP_FINAL_HOST='127.0.0.1'
$env:NET_POWER_SSH_TEST_JUMP_FINAL_PORT='2222'
$env:NET_POWER_SSH_TEST_JUMP_TARGET_HOST='127.0.0.1'
$env:NET_POWER_SSH_TEST_JUMP_TARGET_PORT='2222'
cargo test external_ssh -- --nocapture
```

私钥 passphrase 验证需要把测试公钥写入容器用户 `authorized_keys`，并额外设置：

```powershell
$env:NET_POWER_SSH_TEST_PRIVATE_KEY_PATH='C:\tmp\net-power-ssh-test-key'
$env:NET_POWER_SSH_TEST_PRIVATE_KEY_PASSPHRASE='netpower-key-pass'
cargo test external_ssh_private_key_passphrase_auth -- --nocapture
```

## 测试分层

| 层级 | 覆盖对象 | 推荐方式 |
| --- | --- | --- |
| Rust 单元测试 | JSON path、body rewrite、header rules、secret crypto、配置校验。 | `cargo test` |
| Rust 集成测试 | SQLite migration、repository transaction、ServiceManager 生命周期。 | `cargo test` + 临时数据库 |
| 工具服务测试 | 已保存 HTTP 工具服务的创建、编辑、静态目录模式、内联接口、文件接口、端口、目录/文件校验和暂停后配置保留。 | `cargo test` + 本地 loopback socket |
| 代理集成测试 | HTTP reverse、HTTP keep-alive、HTTP forward、CONNECT、TCP/UDP forwarding。 | 本地 test upstream + tokio 测试 |
| SSH 集成测试 | SSH auth、known_hosts、direct-tcpip local forward、remote forward、SOCKS5 动态代理。 | Docker/OpenSSH 或人工测试机 |
| 前端模型测试 | 表单校验、协议转换、编辑回填、页面筛选。 | Vitest |
| 前端组件测试 | 服务编辑、SSH secret、日志筛选、系统代理配置列表、列表行内启用、配置管理和设置页主题切换。 | Vitest + Testing Library，mock Tauri API |
| 前端视觉冒烟 | 中文工作台、服务表格、日志详情和响应式横向溢出。 | Playwright + 浏览器预览 fallback |
| 安装器 smoke | NSIS 静默安装、安装后启动、SQLite 初始化、安装后 WebView 响应式截图、关闭隐藏到托盘和静默卸载。 | 临时安装目录 + 隔离数据目录 |
| WebView smoke | 真实 Tauri release 窗口启动、resize、截图非空和 SQLite 初始化。 | Win32 窗口控制 + 隔离数据目录 |
| Tray smoke | 真实 Tauri release 主窗口完成 SQLite 初始化后，关闭请求隐藏到托盘且进程不退出。 | Win32 窗口控制 + 隔离数据目录 |
| 人工验收 | 系统代理、打包安装和跨平台行为。 | 记录步骤、环境、结果 |

## 可选 gated 集成测试

- Windows 系统代理真实注册表 roundtrip 默认跳过，显式设置 `NET_POWER_SYSTEM_PROXY_TEST=1` 后执行；测试会快照并恢复当前用户的 `ProxyEnable`、`ProxyServer` 和 `ProxyOverride`。
- 外部 SSH 集成测试默认跳过，显式设置 `NET_POWER_SSH_TEST=1` 后连接真实 SSH server。
- Release smoke 会启动 `src-tauri/target/release/net-power.exe`，通过 `NET_POWER_APP_DATA_DIR` 使用临时数据目录，确认 `proxy-tool.db` 是有效 SQLite 文件后终止进程并清理临时目录。
- Installer smoke 会拒绝在已有 net-power 安装记录时运行，避免修改用户现有安装；正常运行时只使用临时安装目录和隔离数据目录，启动安装后的 exe 验证 SQLite 初始化，再复用 WebView smoke 验证安装后 exe 的桌面宽度/390px 窄宽度截图，复用 Tray smoke 验证关闭隐藏到托盘，并在结束后清理安装文件、进程和临时注册表记录。
- WebView smoke 会短暂打开真实 release 窗口，移动到屏幕左上角附近，分别调整到桌面宽度和 390px 窄宽度并截图；脚本结束后清理进程、截图和临时数据目录。
- Tray smoke 会短暂打开真实 release 窗口，等待隔离数据目录的 SQLite 初始化成功后向主窗口发送关闭请求，确认窗口隐藏且进程仍存活；脚本结束后清理进程和临时数据目录。

Windows 系统代理注册表验证：

```powershell
cd src-tauri
$env:NET_POWER_SYSTEM_PROXY_TEST='1'
cargo test windows_system_proxy_registry_roundtrip_restores_original_values -- --nocapture
```

Release smoke 验证：

```powershell
npm run tauri -- build
npm run verify:smoke
```

这些 smoke 会按 exe 路径清理同源进程，不能并行执行；`npm run verify:smoke` 会按 release、WebView、托盘、安装器顺序串行运行，避免互相清理造成误报。

## 人工验收清单

基础应用：

- `npm run tauri dev` 可启动桌面窗口。
- 应用启动后直接进入工具界面。
- 关闭并重启后配置仍存在。
- 点击主窗口关闭按钮后窗口隐藏到托盘，正在运行的服务不被停止。
- 托盘左键点击或双击可恢复并聚焦主窗口。
- 托盘右键中文菜单可执行显示窗口、隐藏到托盘和退出 net-power。
- 设置页开启“开机启动应用”后，重新登录系统会自动启动 net-power。
- 设置页关闭“开机启动应用”后，系统登录启动项被清理。
- 设置页选择浅色、深色或跟随系统后，工作台、表格、弹框、表单、toast 和状态组件颜色随主题切换。

服务配置：

- 本地服务页可从服务列表打开添加弹框，展示服务类型并创建统一 HTTP 工具服务。
- 本地服务页可从服务列表编辑已保存 HTTP 工具服务配置，运行中的服务编辑后会按新配置重启。
- 本地服务页启动的 HTTP 工具服务可通过静态目录路径前缀返回文件内容。
- 静态目录选择“目录浏览”时，目录请求返回文件列表；选择“静态网站”时，目录请求优先返回 `index.html`。
- 本地服务页启动的 HTTP 工具服务可通过接口路由返回配置的状态码、Content-Type 和手写或文件响应内容。
- 本地服务页暂停工具服务后，原端口可再次使用，配置仍留在列表中。
- 本地或网络转发服务选择局域网启动范围后，复制地址应写入可访问的纯局域网地址；无法识别局域网 IP 时应回退本地地址并提示原因。
- 新增、编辑、复制、删除服务可保存到 SQLite。
- 启动服务后状态变为 Running。
- 停止服务后状态变为 Stopped。
- 重启服务不会泄漏旧端口。

配置日志：

- 服务启动、停止、错误会出现在对应配置的日志弹框。
- 配置日志弹框支持按级别、协议、时间范围和关键词过滤。
- 配置日志弹框展示 meta JSON 摘要。
- 选择日志行的“详情”后可查看流量详情面板和完整 meta JSON。
- 默认看不到敏感 header 和 body。

SSH local forward：

- 密码登录可连接测试 SSH server。已由 WSL Docker OpenSSH 外部测试覆盖。
- 私钥登录可连接测试 SSH server。已由 WSL Docker OpenSSH 外部测试覆盖 passphrase 私钥。
- passphrase 私钥可连接测试 SSH server。已由 WSL Docker OpenSSH 外部测试覆盖。
- local forward 能访问远端 TCP 服务。已由 WSL Docker OpenSSH direct-tcpip 测试覆盖。
- 认证失败和远端不可达有明确错误。

SSH remote forward：

- 创建 `ssh_remote` 服务时需要 SSH 配置、远程绑定主机/端口和本地目标主机/端口。
- ServiceManager 不探测本地监听端口，冲突域按 SSH profile + 远程绑定地址判断。
- 远程监听 session 断开后自动重连，退避时间从 1 秒增长并封顶 30 秒。
- 远端 sshd 允许 TCP forwarding 和 GatewayPorts 时，远程监听端口可访问本机目标服务。已由 WSL Docker OpenSSH external test 覆盖。
- 连接事件 protocol 为 `ssh`，method 为 `REMOTE`，target 记录 `远程绑定 -> 本地目标`。

SSH 跳板：

- SSH Profile 可选择另一个 Profile 作为跳板，多级跳板按引用链解析。
- 数据库层已覆盖跳板链解析顺序、自引用、循环引用和删除保护。
- 真实 OpenSSH 多级跳板链已由 WSL Docker external test 覆盖，测试会连续建立两级跳板和最终 session，再通过最终 session 的 `direct-tcpip` 读取目标 SSH banner。

SSH SOCKS5：

- 创建 `ssh_socks` 服务时只需要 SSH 配置、监听主机和监听端口。
- SOCKS5 no-auth 客户端可通过 CONNECT 访问 IPv4、域名和 IPv6 目标。域名 CONNECT 到真实 OpenSSH 远端目标已由 WSL Docker external test 覆盖。
- 连接事件 protocol 为 `ssh`，method 为 `SOCKS5 CONNECT`，target 只记录目标地址，不记录请求体或凭据。

Windows 系统代理：

- 可从代理配置列表选择 HTTP forward proxy，必要时先启动服务再设置为系统代理。
- 可保存、编辑、启用和删除系统代理配置档。
- 可一键清理系统代理。
- 设置失败时 UI 展示明确错误。
- 应用重启后能读取当前系统代理状态。

macOS 系统代理：

- `networksetup -listallnetworkservices` 可列出网络服务。
- 设置代理后，活动网络服务的 HTTP/HTTPS proxy 指向目标 host/port。
- 清理后 HTTP/HTTPS proxy state 为 off。

Linux 系统代理：

- GNOME 环境下 `gsettings get org.gnome.system.proxy mode` 设置后为 `manual`，清理后为 `none`。
- 设置代理后 `~/.config/environment.d/net-power-proxy.conf` 或 `$XDG_CONFIG_HOME/environment.d/net-power-proxy.conf` 包含 `HTTP_PROXY`、`HTTPS_PROXY`、`NO_PROXY` 等变量。
- 非 GNOME 环境没有 `gsettings` 时，UI 应提示桌面代理未修改，但 shell 代理环境文件仍被写入或清理。

打包：

- `npm run tauri build` 成功。
- GitHub Actions `desktop-ci.yml` 可在 pull request/push 上运行前端构建、Vitest、Rust fmt/test/clippy。
- GitHub Actions `release.yml` 可在 `v*.*.*` tag 上构建 Windows NSIS、macOS Intel/Apple Silicon DMG、Linux AppImage/DEB，并上传 updater `latest.json`。
- GitHub 仓库已配置 `TAURI_SIGNING_PRIVATE_KEY` 和 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 后，release workflow 的 updater artifacts 签名成功。
- 发布 Release 后，已安装旧版本客户端在设置页点击“检查更新”可读取 GitHub latest release、下载更新包并完成安装/重启。
- `npm run verify:release` 可验证 release exe 启动和首次 SQLite 初始化。
- `npm run verify:installer` 可验证 NSIS 安装包静默安装、启动安装后 exe、首次 SQLite 初始化、安装后 WebView 响应式截图、关闭隐藏到托盘和静默卸载。
- `npm run verify:webview` 可验证真实 Tauri release 窗口启动、resize 到 390px 窄宽度、截图非空和首次 SQLite 初始化。
- `npm run verify:tray` 可验证真实 Tauri release 主窗口完成 SQLite 初始化后，关闭会隐藏到托盘并保持进程运行。
- 安装器完整流程仍建议在干净 Windows 环境做一次人工验收。
- 打包产物不包含本地开发数据库或测试 secret。

## 验收记录模板

```text
日期：
提交/版本：
平台：
命令：
场景：
步骤：
期望结果：
实际结果：
结论：通过 / 不通过
备注：
```
