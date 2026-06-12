<!-- @author kongweiguang -->

# 已完成能力

## 工作台与桌面体验

- 主工作台按“仪表盘、本地服务、网络转发、SSH、系统代理、设置”组织，启动后直接进入可操作界面。
- 工作台支持新增、编辑、复制、启动、停止、重启、删除服务配置，并按页面展示运行态、连接数、字节统计和最近错误。
- 自定义系统标题栏、系统托盘、关闭隐藏到托盘、托盘显示/隐藏/退出菜单、开机启动应用和服务自动启动策略已接入。
- 设置页支持浅色、深色和跟随系统主题，主题设置持久化到 `app_settings`，工作台组件颜色随主题变量切换。
- 应用更新接入 Tauri updater 和 process 插件，设置页可检查、下载、安装更新并尝试重启应用。
- 响应式工作台已覆盖桌面、平板和 390px 窄屏布局，服务表格在窄窗口下转为卡片式信息行。

## 代理与隧道

- HTTP Reverse 支持固定上游转发、request/response header set/remove、JSON/form body rewrite、chunked 请求体解码和 HTTP/1.1 keep-alive 顺序复用。
- HTTP Forward 支持普通 HTTP 代理和 HTTPS CONNECT，普通 HTTP 请求支持同一客户端连接内的 keep-alive 顺序复用。
- TCP Forward 和 UDP Forward 支持本地监听到目标地址转发，UDP 按客户端地址维护目标 socket 并按 idle timeout 清理。
- 网络转发配置支持本地 `127.0.0.1` 与局域网 `0.0.0.0` 启动范围，复制地址时写入纯地址，局域网模式优先复制自动识别到的局域网 IP。
- SSH Profile 支持 password、private key、agent 认证，密码和 passphrase 加密存储。
- SSH local forward、remote forward、SOCKS5 动态代理、known_hosts strict/accept_new 校验、跳板机、多级跳板和 remote 自动重连已实现。
- 系统代理支持 Windows、macOS、Linux 设置、清理和状态读取；Linux 同时维护 `environment.d/net-power-proxy.conf` shell 代理环境文件。

## 本地工具服务

- 本地服务页以服务列表管理可持久化 HTTP 工具服务，展示类型、监听、访问地址、静态目录、接口数、请求数和运行态。
- HTTP 工具服务支持添加、编辑、启动、暂停和删除；暂停只停止运行态，不删除 SQLite 配置。
- 运行中的工具服务被编辑后会先停止旧运行态，再按新配置重启。
- 工具服务支持本地和局域网启动范围，复制地址时区分本地地址和局域网地址。
- 单个 HTTP 工具服务可同时挂载静态目录和多条接口路由，接口响应体支持手写内容或本地文件。
- 静态目录支持“目录浏览”和“静态网站”两种模式：目录浏览生成文件列表，静态网站模式优先返回 `index.html`。

## 数据、日志与安全

- SQLite migration、默认设置和核心表结构已落地，数据库默认位于 `~/.net-power/proxy-tool.db`。
- 启动时补齐 `app.initialized`、`logs.retention_days`、`logs.max_rows`、`services.auto_start_enabled` 和 `ui.theme_mode`，不会覆盖用户已保存的值。
- 服务配置、工具服务、SSH Profile、系统代理配置档、服务事件和连接事件均持久化到 SQLite。
- 删除服务和 SSH Profile 使用软删除，避免历史日志引用失效。
- 每个服务配置行可打开日志弹框，按级别、协议、关键词和时间范围筛选，并查看连接级 meta JSON。
- 流量详情默认不保存 request/response body，也不记录 Authorization、Cookie、Set-Cookie、Proxy-Authorization 等敏感 header。
- 启动时日志保留清理已实现，按 `logs.retention_days` 和 `logs.max_rows` 清理过期或超量日志。

## 自动化验证

- Rust 测试覆盖数据库迁移、默认设置补齐、服务 CRUD、工具服务创建/编辑、ServiceManager 生命周期、HTTP/TCP/UDP/SSH 代理核心、系统代理命令构造、日志合并查询和日志保留。
- 前端 Vitest 覆盖工作台表单校验、协议转换、编辑回填、页面筛选、工具服务编辑、复制地址、日志筛选、系统代理配置、主题切换、应用更新入口和关于信息。
- Playwright 视觉冒烟覆盖浏览器预览模式下的主要页面、配置日志详情和页面级横向溢出。
- WSL Docker OpenSSH 外部测试覆盖密码登录、私钥口令登录、direct-tcpip local forward、两级跳板链、remote forward、SOCKS5 动态代理和 strict known_hosts mismatch。
- Windows 系统代理注册表 gated 集成测试覆盖 set/status/clear，并恢复测试前 `ProxyEnable`、`ProxyServer`、`ProxyOverride`。
- Release、WebView、Tray 和 NSIS installer smoke 脚本已覆盖 release exe 启动、SQLite 初始化、真实 WebView resize、关闭隐藏到托盘、安装后启动和静默卸载。
