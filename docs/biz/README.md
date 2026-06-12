<!-- @author kongweiguang -->

# 业务能力

## HTTP

- HTTP Reverse：监听本地端口，转发到固定上游 URL，支持 header 规则和 JSON/form body rewrite。
- HTTP Reverse 会解码客户端 `Transfer-Encoding: chunked` 请求体，再按规则改写并以明确 `Content-Length` 转发上游。
- HTTP Reverse 支持 HTTP/1.1 keep-alive，客户端同一 TCP 连接内的多个普通请求会按顺序处理并分别记录连接事件。
- HTTP 页面使用结构化规则编辑器维护 header set/remove 和 body rewrite，不需要手写整段 JSON。
- HTTP 正向代理（Forward）：作为通用 HTTP 代理，支持普通 HTTP 请求和 HTTPS CONNECT。
- HTTP Forward 普通请求支持 keep-alive 顺序复用；CONNECT 建立隧道后进入字节转发，不和后续 HTTP 请求复用。

## Forwarding

- TCP Forward：本地 TCP 监听到目标 TCP，按连接统计 bytes 和连接数。
- UDP Forward：本地 UDP 监听到目标 UDP，按客户端地址维护目标 socket，返回数据写回原客户端，并按 idle timeout 清理映射。

## SSH

- SSH Profile：支持 password、private key、agent，密码和 passphrase 加密存储。
- SSH Local Tunnel：实现类似 `ssh -L` 的本地端口转发。
- SSH Remote Tunnel：实现类似 `ssh -R` 的远程端口转发，由 SSH server 监听远程绑定地址并转发到本机目标。
- SSH SOCKS5：实现类似 `ssh -D` 的动态代理，本地监听 SOCKS5 no-auth 端口，通过 SSH `direct-tcpip` 转发 CONNECT 目标。
- SSH 跳板机/多级跳板：SSH Profile 可选择另一个 Profile 作为跳板，运行时按引用链逐跳认证并建立最终隧道。
- SSH remote 自动重连：远程监听 session 断开后自动按指数退避重连；用户停止服务会取消重连。
- SSH 认证测试：执行真实 SSH 握手、known_hosts 校验和认证。

## System Proxy

- Windows 支持设置/清理当前用户系统代理。
- macOS 支持通过 `networksetup` 设置/清理 HTTP 和 HTTPS 系统代理。
- Linux 支持通过 GNOME `gsettings` 设置/清理桌面系统代理，并写入 `environment.d` shell 代理环境文件。
- UI 支持把 HTTP 正向代理服务和常用系统代理配置档合并为来源列表，选中后可从右侧启动并设置对应系统代理；同时支持手动填写临时目标。
- 非 GNOME Linux 环境找不到 `gsettings` 时不会静默伪装成功，状态消息会说明只写入 shell 代理环境文件。

## App Shell

- 系统托盘提供中文菜单：显示窗口、隐藏到托盘、退出 net-power。
- 点击主窗口关闭按钮时默认隐藏到托盘，不会停止正在运行的代理服务；需要完全退出时使用托盘菜单的退出项。
- 托盘左键点击或双击会恢复并聚焦主窗口。
- 设置页提供“开机启动应用”开关，写入或清理当前系统登录启动项；它只负责启动 net-power 应用本身。
- “服务自动启动”是应用启动后的服务恢复策略，独立于系统开机启动项。

## 配置日志

- 服务事件和连接事件写入 SQLite。
- UI 在每个服务配置行提供“日志”按钮，打开弹框后只显示当前配置的运行日志。
- 日志弹框支持按级别、协议、时间范围和关键词筛选服务事件。
- 日志弹框提供流量详情面板，选择一条日志后可查看服务、级别、时间、消息、连接元数据和完整 meta JSON。
- 流量详情默认不保存 request/response body，也不记录 Authorization、Cookie、Set-Cookie、Proxy-Authorization 等敏感 header。
