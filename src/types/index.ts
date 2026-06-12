/**
 * @author kongweiguang
 * 前端与 Tauri Command 共享的协议类型。字段名与 Rust serde camelCase 输出保持一致。
 */

/** 服务类型，对应后端 ServiceKind。 */
export type ServiceKind =
  | "http_reverse"
  | "http_forward"
  | "tcp_forward"
  | "udp_forward"
  | "ssh_local"
  | "ssh_remote"
  | "ssh_socks";

/** 服务运行态。 */
export type RuntimeStatus =
  | { type: "stopped" }
  | { type: "starting" }
  | { type: "running" }
  | { type: "stopping" }
  | { type: "failed"; message: string };

/** 工具 HTTP 服务响应内容来源。 */
export type ToolServiceContentSource = "inline" | "file";

/** 工具 HTTP 服务接口路由输入。 */
export interface ToolServiceRouteInput {
  /** HTTP 方法。 */
  method: "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" | "ANY";
  /** 精确匹配的请求路径。 */
  path: string;
  /** 响应状态码。 */
  responseStatus: number;
  /** 响应 Content-Type。 */
  contentType: string;
  /** 响应体来源。 */
  contentSource: ToolServiceContentSource;
  /** 手写响应体。 */
  body?: string | null;
  /** 响应文件路径。 */
  filePath?: string | null;
}

/** 本地工具服务配置输入。 */
export interface ToolServiceInput {
  /** 用户可读名称。 */
  name: string;
  /** 监听主机。 */
  host: string;
  /** 监听端口。 */
  port: number;
  /** 静态目录路径；为空表示不挂载静态文件。 */
  staticRootDir?: string | null;
  /** 静态目录挂载路径前缀。 */
  staticPathPrefix: string;
  /** 接口路由列表。 */
  routes: ToolServiceRouteInput[];
}

/** 本地工具服务摘要。配置来自 SQLite，运行态来自后端内存管理器。 */
export interface ToolServiceSummary {
  /** 服务主键。 */
  id: string;
  /** 用户可读名称。 */
  name: string;
  /** 监听主机。 */
  host: string;
  /** 监听端口。 */
  port: number;
  /** 可直接访问的本地 URL。 */
  url: string;
  /** 静态目录路径。 */
  staticRootDir?: string | null;
  /** 静态目录挂载路径前缀。 */
  staticPathPrefix: string;
  /** 接口路由数量。 */
  routeCount: number;
  /** 启动时间；已暂停服务为空。 */
  startedAt?: string | null;
  /** 累计请求数。 */
  totalRequests: number;
  /** 运行态。 */
  runtimeStatus: RuntimeStatus;
}

/** HTTP 反向代理配置。 */
export interface HttpReverseConfig {
  /** 上游基础 URL。 */
  targetUrl: string;
  /** 是否保留原始 Host。 */
  preserveHost: boolean;
  /** 请求超时时间，毫秒。 */
  requestTimeoutMs: number;
  /** 最大可改写 body 字节数。 */
  maxRewriteBodyBytes: number;
  /** 遇到压缩 body 时是否跳过改写。 */
  skipCompressedBody: boolean;
}

/** HTTP forward proxy 配置。 */
export interface HttpForwardConfig {
  /** 是否允许 HTTP 请求。 */
  allowHttp: boolean;
  /** 是否允许 CONNECT 隧道。 */
  allowConnect: boolean;
  /** CONNECT 建连超时，毫秒。 */
  connectTimeoutMs: number;
  /** 空闲超时，毫秒。 */
  idleTimeoutMs: number;
}

/** TCP 转发配置。 */
export interface TcpForwardConfig {
  /** 目标主机。 */
  targetHost: string;
  /** 目标端口。 */
  targetPort: number;
  /** 建连超时，毫秒。 */
  connectTimeoutMs: number;
  /** 空闲超时，毫秒。 */
  idleTimeoutMs: number;
}

/** UDP 转发配置。 */
export interface UdpForwardConfig {
  /** 目标主机。 */
  targetHost: string;
  /** 目标端口。 */
  targetPort: number;
  /** 空闲超时，毫秒。 */
  idleTimeoutMs: number;
}

/** SSH 隧道配置。 */
export interface SshTunnelConfig {
  /** SSH profile 主键。 */
  sshProfileId: string;
  /** 隧道类型，当前支持 local、remote 和 socks。 */
  tunnelType: "local" | "remote" | "socks";
  /** 目标主机。 */
  targetHost: string | null;
  /** 目标端口。 */
  targetPort: number | null;
  /** 远程绑定主机。 */
  remoteBindHost: string | null;
  /** 远程绑定端口。 */
  remoteBindPort: number | null;
}

/** Header 改写规则。 */
export interface HeaderRule {
  /** 规则主键。 */
  id: string;
  /** request 或 response。 */
  phase: "request" | "response";
  /** set 或 remove。 */
  action: "set" | "remove";
  /** Header 名称。 */
  name: string;
  /** Header 值。 */
  value?: string | null;
  /** 是否启用。 */
  enabled: boolean;
  /** 排序值。 */
  sortOrder: number;
}

/** Header 改写规则输入。 */
export type HeaderRuleInput = Omit<HeaderRule, "id">;

/** Body 改写规则。 */
export interface BodyRewriteRule {
  /** 规则主键。 */
  id: string;
  /** body 类型。 */
  bodyType: "auto" | "json" | "form";
  /** 改写路径。 */
  path: string;
  /** JSON 字符串表示的目标值。 */
  valueJson: string;
  /** 是否启用。 */
  enabled: boolean;
  /** 排序值。 */
  sortOrder: number;
}

/** Body 改写规则输入。 */
export type BodyRewriteRuleInput = Omit<BodyRewriteRule, "id">;

/** 服务详情。 */
export interface ServiceDetail {
  /** 服务主键。 */
  id: string;
  /** 用户可读名称。 */
  name: string;
  /** 服务类型。 */
  kind: ServiceKind;
  /** 是否启用。 */
  enabled: boolean;
  /** 是否随应用启动。 */
  autoStart: boolean;
  /** 监听主机。 */
  listenHost: string;
  /** 监听端口。 */
  listenPort: number;
  /** 备注。 */
  notes: string;
  /** 创建时间。 */
  createdAt: string;
  /** 更新时间。 */
  updatedAt: string;
  /** HTTP 反向代理配置。 */
  httpReverse?: HttpReverseConfig | null;
  /** HTTP forward proxy 配置。 */
  httpForward?: HttpForwardConfig | null;
  /** TCP 转发配置。 */
  tcpForward?: TcpForwardConfig | null;
  /** UDP 转发配置。 */
  udpForward?: UdpForwardConfig | null;
  /** SSH 隧道配置。 */
  sshTunnel?: SshTunnelConfig | null;
  /** Header 改写规则。 */
  headerRules: HeaderRule[];
  /** Body 改写规则。 */
  bodyRewriteRules: BodyRewriteRule[];
}

/** 服务列表摘要。 */
export interface ServiceSummary {
  /** 服务主键。 */
  id: string;
  /** 用户可读名称。 */
  name: string;
  /** 服务类型。 */
  kind: ServiceKind;
  /** 是否启用。 */
  enabled: boolean;
  /** 是否随应用启动。 */
  autoStart: boolean;
  /** 监听主机。 */
  listenHost: string;
  /** 监听端口。 */
  listenPort: number;
  /** 目标摘要。 */
  targetLabel: string;
  /** 运行态。 */
  runtimeStatus: RuntimeStatus;
  /** 当前活跃连接数。 */
  activeConnections: number;
  /** 累计连接数。 */
  totalConnections: number;
}

/** 创建/更新服务输入。 */
export interface CreateServiceInput {
  /** 用户可读名称。 */
  name: string;
  /** 服务类型。 */
  kind: ServiceKind;
  /** 是否启用。 */
  enabled: boolean;
  /** 是否随应用启动。 */
  autoStart: boolean;
  /** 监听主机。 */
  listenHost: string;
  /** 监听端口。 */
  listenPort: number;
  /** 备注。 */
  notes: string;
  /** HTTP 反向代理配置。 */
  httpReverse?: HttpReverseConfig | null;
  /** HTTP forward proxy 配置。 */
  httpForward?: HttpForwardConfig | null;
  /** TCP 转发配置。 */
  tcpForward?: TcpForwardConfig | null;
  /** UDP 转发配置。 */
  udpForward?: UdpForwardConfig | null;
  /** SSH 隧道配置。 */
  sshTunnel?: SshTunnelConfig | null;
  /** Header 改写规则。 */
  headerRules: HeaderRuleInput[];
  /** Body 改写规则。 */
  bodyRewriteRules: BodyRewriteRuleInput[];
}

/** 运行态摘要。 */
export interface ServiceRuntimeSummary {
  /** 服务主键。 */
  serviceId: string;
  /** 服务类型。 */
  kind: ServiceKind;
  /** 监听地址。 */
  listenAddr: string;
  /** 运行态。 */
  runtimeStatus: RuntimeStatus;
  /** 启动时间。 */
  startedAt?: string | null;
  /** 当前活跃连接数。 */
  activeConnections: number;
  /** 累计连接数。 */
  totalConnections: number;
  /** 累计入口字节。 */
  bytesIn: number;
  /** 累计出口字节。 */
  bytesOut: number;
}

/** 操作测试结果。 */
export interface TestResult {
  /** 是否成功。 */
  ok: boolean;
  /** 用户可读消息。 */
  message: string;
  /** 耗时，毫秒。 */
  durationMs: number;
}

/** SSH 认证类型。 */
export type SshAuthType = "password" | "private_key" | "agent";

/** SSH Profile。 */
export interface SshProfile {
  /** Profile 主键。 */
  id: string;
  /** 用户可读名称。 */
  name: string;
  /** SSH 主机。 */
  host: string;
  /** SSH 端口。 */
  port: number;
  /** 用户名。 */
  username: string;
  /** 认证方式。 */
  authType: SshAuthType;
  /** 是否已有密码。 */
  hasPassword: boolean;
  /** 私钥路径。 */
  privateKeyPath?: string | null;
  /** 是否已有 passphrase。 */
  hasPassphrase: boolean;
  /** known_hosts 模式。 */
  knownHostsMode: "strict" | "accept_new" | "insecure_skip";
  /** known_hosts 路径。 */
  knownHostsPath?: string | null;
  /** 连接超时，毫秒。 */
  connectTimeoutMs: number;
  /** keepalive 间隔，毫秒。 */
  keepaliveIntervalMs: number;
  /** 可选跳板 SSH Profile 主键；为空表示直连。 */
  jumpProfileId?: string | null;
}

/** 创建/更新 SSH Profile 输入。 */
export interface SshProfileInput {
  /** 用户可读名称。 */
  name: string;
  /** SSH 主机。 */
  host: string;
  /** SSH 端口。 */
  port: number;
  /** 用户名。 */
  username: string;
  /** 认证方式。 */
  authType: SshAuthType;
  /** 新密码。 */
  password?: string | null;
  /** 私钥路径。 */
  privateKeyPath?: string | null;
  /** 新 passphrase。 */
  privateKeyPassphrase?: string | null;
  /** known_hosts 模式。 */
  knownHostsMode: "strict" | "accept_new" | "insecure_skip";
  /** known_hosts 路径。 */
  knownHostsPath?: string | null;
  /** 连接超时，毫秒。 */
  connectTimeoutMs: number;
  /** keepalive 间隔，毫秒。 */
  keepaliveIntervalMs: number;
  /** 可选跳板 SSH Profile 主键；为空表示直连。 */
  jumpProfileId?: string | null;
}

/** 应用设置行。 */
export interface AppSetting {
  /** 设置 key。 */
  key: string;
  /** JSON 字符串形式的值。 */
  valueJson: string;
}

/** 应用更新下载进度。 */
export interface AppUpdateProgress {
  /** 进度阶段。 */
  phase: "started" | "progress" | "finished";
  /** 用户可读进度说明。 */
  message: string;
  /** 已下载字节数。 */
  downloadedBytes?: number;
  /** 总字节数，服务端未返回时为空。 */
  totalBytes?: number | null;
}

/** 应用更新检查与安装结果。 */
export interface AppUpdateResult {
  /** 是否发现并安装了新版本。 */
  updated: boolean;
  /** 当前版本。 */
  currentVersion?: string;
  /** 新版本。 */
  version?: string;
  /** 更新说明。 */
  body?: string | null;
  /** 是否已成功触发应用重启。 */
  relaunchTriggered?: boolean;
  /** 安装完成但重启失败时的用户可读错误。 */
  relaunchError?: string;
}

/** 系统登录时自动启动应用的状态。 */
export interface AutostartStatus {
  /** 当前是否已注册系统开机启动项。 */
  enabled: boolean;
  /** 当前平台或运行环境是否支持开机启动。 */
  supported: boolean;
  /** 面向用户展示的状态说明。 */
  message: string;
}

/** 日志筛选条件。 */
export interface LogFilter {
  /** 服务主键。 */
  serviceId?: string | null;
  /** 日志级别。 */
  level?: "trace" | "debug" | "info" | "warn" | "error" | null;
  /** 协议筛选。 */
  protocol?: string | null;
  /** 关键词。 */
  keyword?: string | null;
  /** 起始时间，ISO 或后端可解析字符串。 */
  createdAfter?: string | null;
  /** 结束时间，ISO 或后端可解析字符串。 */
  createdBefore?: string | null;
  /** 最大返回行数。 */
  limit?: number | null;
}

/** 服务日志行。 */
export interface LogRow {
  /** 日志主键。 */
  id: number;
  /** 服务主键。 */
  serviceId?: string | null;
  /** 日志级别。 */
  level: "trace" | "debug" | "info" | "warn" | "error";
  /** 消息。 */
  message: string;
  /** 元数据 JSON。 */
  metaJson: string;
  /** 创建时间。 */
  createdAt: string;
}

/** 系统代理状态。 */
export interface SystemProxyStatus {
  /** 是否启用。 */
  enabled: boolean;
  /** 代理主机。 */
  proxyHost: string;
  /** 代理端口。 */
  proxyPort?: number | null;
  /** 绕过列表。 */
  bypass: string;
  /** 用户可读状态消息。 */
  message: string;
}

/** 手动系统代理目标。 */
export interface SystemProxyTarget {
  /** 代理主机。 */
  proxyHost: string;
  /** 代理端口。 */
  proxyPort: number;
  /** 绕过地址列表。 */
  bypass: string;
}

/** 系统代理配置档。 */
export interface SystemProxyProfile {
  /** 配置档主键。 */
  id: string;
  /** 用户可读名称。 */
  name: string;
  /** 代理主机。 */
  proxyHost: string;
  /** 代理端口。 */
  proxyPort: number;
  /** 绕过地址列表。 */
  bypass: string;
  /** 是否为最近一次通过配置档启用的目标。 */
  active: boolean;
  /** 创建时间。 */
  createdAt: string;
  /** 更新时间。 */
  updatedAt: string;
}

/** 创建或更新系统代理配置档输入。 */
export type SystemProxyProfileInput = Pick<SystemProxyProfile, "name" | "proxyHost" | "proxyPort" | "bypass">;

/** Rust 后端推送的服务事件。 */
export interface ServiceEventPayload {
  /** 服务主键，系统级事件为空。 */
  serviceId?: string | null;
  /** 事件级别。 */
  level: "trace" | "debug" | "info" | "warn" | "error";
  /** 用户可读消息。 */
  message: string;
}

/** Rust 后端推送的连接事件。 */
export interface ConnectionEventPayload {
  /** 服务主键。 */
  serviceId: string;
  /** 协议名称。 */
  protocol: string;
  /** 事件类型。 */
  eventType: string;
  /** 目标地址。 */
  targetAddr: string;
}
