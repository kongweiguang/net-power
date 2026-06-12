/**
 * @author kongweiguang
 * Workbench 页面纯模型层。这里集中表单 draft、校验和协议转换，方便单元测试保护 UI 行为。
 */

import type {
  BodyRewriteRuleInput,
  CreateServiceInput,
  HeaderRuleInput,
  ServiceDetail,
  ServiceKind,
  ServiceSummary,
  SshAuthType,
  SshProfile,
  SshProfileInput,
  ToolServiceContentSource,
  ToolServiceConfig,
  ToolServiceInput,
  ToolServiceSummary,
  ToolServiceRouteInput,
  ToolServiceStaticMode,
} from "../../types";

/** 工具 HTTP 服务支持的方法。 */
export type ToolServiceMethod = ToolServiceRouteInput["method"];

/** Workbench 导航页签。 */
export type PageKey =
  | "dashboard"
  | "services"
  | "forwarding"
  | "ssh"
  | "system"
  | "settings";

/** 本地工具服务表单草稿。 */
export interface ToolServiceDraft {
  /** 用户可读服务名。 */
  name: string;
  /** 监听主机。 */
  host: string;
  /** 监听端口，表单内以字符串保存。 */
  port: string;
  /** 静态目录路径。 */
  staticRootDir: string;
  /** 静态目录访问模式。 */
  staticMode: ToolServiceStaticMode;
  /** 静态目录挂载路径前缀。 */
  staticPathPrefix: string;
  /** 接口路由草稿。 */
  routes: ToolServiceRouteDraft[];
}

/** 本地工具 HTTP 服务接口路由草稿。 */
export interface ToolServiceRouteDraft {
  /** 前端稳定 key。 */
  id: string;
  /** HTTP 方法。 */
  method: ToolServiceMethod;
  /** 精确匹配的请求路径。 */
  path: string;
  /** 响应状态码，表单内以字符串保存。 */
  responseStatus: string;
  /** 响应 Content-Type。 */
  contentType: string;
  /** 响应体来源。 */
  contentSource: ToolServiceContentSource;
  /** 手写响应体。 */
  body: string;
  /** 响应文件路径。 */
  filePath: string;
}

/** 服务表单草稿。 */
export interface ServiceDraft {
  /** 用户可读服务名。 */
  name: string;
  /** 服务类型。 */
  kind: ServiceKind;
  /** 监听主机。 */
  listenHost: string;
  /** 监听端口，表单内以字符串保存。 */
  listenPort: string;
  /** HTTP 反向代理目标 URL。 */
  targetUrl: string;
  /** TCP/UDP/SSH 目标主机。 */
  targetHost: string;
  /** TCP/UDP/SSH 目标端口，表单内以字符串保存。 */
  targetPort: string;
  /** SSH 隧道引用的 profile id。 */
  sshProfileId: string;
  /** 是否随应用启动。 */
  autoStart: boolean;
  /** HTTP reverse 是否保留 Host。 */
  preserveHost: boolean;
  /** HTTP reverse 请求超时毫秒。 */
  requestTimeoutMs: string;
  /** HTTP reverse 最大改写 body 字节数。 */
  maxRewriteBodyBytes: string;
  /** HTTP reverse 是否跳过压缩 body。 */
  skipCompressedBody: boolean;
  /** HTTP forward 是否允许普通 HTTP。 */
  allowHttp: boolean;
  /** HTTP forward 是否允许 CONNECT。 */
  allowConnect: boolean;
  /** TCP/HTTP forward 连接超时毫秒。 */
  connectTimeoutMs: string;
  /** HTTP forward/UDP 空闲超时毫秒。 */
  idleTimeoutMs: string;
  /** Header 改写规则草稿。 */
  headerRules: HeaderRuleInput[];
  /** Body 改写规则草稿。 */
  bodyRewriteRules: BodyRewriteRuleInput[];
  /** 备注。 */
  notes: string;
}

/** SSH 配置表单草稿。 */
export interface SshDraft {
  /** 用户可读 profile 名称。 */
  name: string;
  /** SSH 主机。 */
  host: string;
  /** SSH 端口，表单内以字符串保存。 */
  port: string;
  /** SSH 用户名。 */
  username: string;
  /** 认证类型。 */
  authType: SshAuthType;
  /** 新密码，空值表示更新时保留旧 secret。 */
  password: string;
  /** 是否已有密码 secret。 */
  hasPassword: boolean;
  /** 私钥路径。 */
  privateKeyPath: string;
  /** 新私钥口令，空值表示更新时保留旧 secret。 */
  privateKeyPassphrase: string;
  /** known_hosts 校验模式。 */
  knownHostsMode: "strict" | "accept_new" | "insecure_skip";
  /** known_hosts 文件路径。 */
  knownHostsPath: string;
  /** SSH 连接超时毫秒。 */
  connectTimeoutMs: string;
  /** SSH keepalive 间隔毫秒。 */
  keepaliveIntervalMs: string;
  /** 可选跳板 SSH profile id。 */
  jumpProfileId: string;
}

/** 服务类型展示名。 */
export const kindLabels: Record<ServiceKind, string> = {
  http_reverse: "HTTP 反向代理",
  http_forward: "HTTP 正向代理",
  tcp_forward: "TCP 转发",
  udp_forward: "UDP 转发",
  ssh_local: "SSH 本地隧道",
  ssh_remote: "SSH 远程隧道",
  ssh_socks: "SSH SOCKS5 动态代理",
};

/** 工具 HTTP 服务方法展示名。 */
export const toolServiceMethodLabels: Record<ToolServiceMethod, string> = {
  GET: "GET",
  POST: "POST",
  PUT: "PUT",
  PATCH: "PATCH",
  DELETE: "DELETE",
  HEAD: "HEAD",
  OPTIONS: "OPTIONS",
  ANY: "ANY",
};

/** 工具 HTTP 服务响应来源展示名。 */
export const toolServiceContentSourceLabels: Record<ToolServiceContentSource, string> = {
  inline: "手写内容",
  file: "选择文件",
};

/** 工具 HTTP 服务静态目录访问模式展示名。 */
export const toolServiceStaticModeLabels: Record<ToolServiceStaticMode, string> = {
  directory: "目录浏览",
  site: "静态网站",
};

/** 监听绑定模式。local 只监听本机，lan 监听所有网卡，custom 用于兼容已有自定义主机。 */
export type BindMode = "local" | "lan" | "custom";

/** 本机监听地址。 */
export const localBindHost = "127.0.0.1";

/** 局域网监听地址。 */
export const lanBindHost = "0.0.0.0";

/** 绑定模式展示名。 */
export const bindModeLabels: Record<Exclude<BindMode, "custom">, string> = {
  local: "本地 (127.0.0.1)",
  lan: "局域网 (0.0.0.0)",
};

/** 根据已保存 host 推断表单绑定模式。 */
export function bindModeFromHost(host: string): BindMode {
  const normalized = host.trim().toLowerCase();
  if (normalized === lanBindHost) return "lan";
  if (!normalized || normalized === localBindHost || normalized === "localhost") return "local";
  return "custom";
}

/** 根据绑定模式返回后端实际监听 host；custom 保留已有值。 */
export function hostForBindMode(mode: BindMode, currentHost = localBindHost): string {
  if (mode === "lan") return lanBindHost;
  if (mode === "local") return localBindHost;
  return currentHost.trim() || localBindHost;
}

/** 绑定模式下拉选项。自定义 host 只在编辑旧数据时出现，用于避免无意改写。 */
export function bindModeOptionsForHost(host: string): Array<{ value: BindMode; label: string }> {
  const options: Array<{ value: BindMode; label: string }> = [
    { value: "local", label: bindModeLabels.local },
    { value: "lan", label: bindModeLabels.lan },
  ];
  if (bindModeFromHost(host) === "custom") {
    options.push({ value: "custom", label: `自定义 (${host.trim()})` });
  }
  return options;
}

/** 复制地址时的单条地址。 */
export interface AccessAddress {
  /** 地址类型标签。 */
  label: "本地" | "局域网" | "自定义";
  /** 可复制地址。 */
  value: string;
}

/** 复制地址动作的剪贴板内容和用户提示。 */
export interface AddressCopyPayload {
  /** 写入剪贴板的纯地址。 */
  text: string;
  /** 复制成功后展示给用户的说明。 */
  detail: string;
}

/** 构造服务可访问地址。局域网监听会同时给出本地地址和实际局域网地址。 */
export function accessAddressEntries(
  host: string,
  port: number,
  format: "hostPort" | "url" = "hostPort",
  path = "/",
  lanIp?: string | null,
): AccessAddress[] {
  const mode = bindModeFromHost(host);
  if (mode === "lan") {
    const lanHost = normalizeLanCopyHost(lanIp);
    return [
      { label: "本地", value: formatAccessAddress(localBindHost, port, format, path) },
      { label: "局域网", value: formatAccessAddress(lanHost, port, format, path) },
    ];
  }
  if (mode === "custom") {
    return [{ label: "自定义", value: formatAccessAddress(host.trim(), port, format, path) }];
  }
  return [{ label: "本地", value: formatAccessAddress(localBindHost, port, format, path) }];
}

/** 构造普通代理服务的复制地址载荷。 */
export function serviceAddressCopyPayload(
  service: Pick<ServiceSummary, "kind" | "listenHost" | "listenPort">,
  lanIp?: string | null,
): AddressCopyPayload {
  const format = service.kind === "http_reverse" || service.kind === "http_forward" ? "url" : "hostPort";
  return addressCopyPayload(accessAddressEntries(service.listenHost, service.listenPort, format, "/", lanIp), lanIp);
}

/** 格式化普通代理服务的纯复制地址文本。 */
export function formatServiceCopyText(
  service: Pick<ServiceSummary, "kind" | "listenHost" | "listenPort">,
  lanIp?: string | null,
): string {
  return serviceAddressCopyPayload(service, lanIp).text;
}

/** 构造工具 HTTP 服务的复制地址载荷。 */
export function toolServiceAddressCopyPayload(
  service: Pick<ToolServiceSummary, "host" | "port" | "url">,
  lanIp?: string | null,
): AddressCopyPayload {
  return addressCopyPayload(accessAddressEntries(service.host, service.port, "url", urlPathSuffix(service.url), lanIp), lanIp);
}

/** 格式化工具 HTTP 服务的纯复制地址文本。 */
export function formatToolServiceCopyText(
  service: Pick<ToolServiceSummary, "host" | "port" | "url">,
  lanIp?: string | null,
): string {
  return toolServiceAddressCopyPayload(service, lanIp).text;
}

/** 默认服务草稿。 */
export const defaultServiceDraft: ServiceDraft = {
  name: "",
  kind: "http_reverse",
  listenHost: "127.0.0.1",
  listenPort: "7890",
  targetUrl: "http://127.0.0.1:8080",
  targetHost: "127.0.0.1",
  targetPort: "8080",
  sshProfileId: "",
  autoStart: false,
  preserveHost: false,
  requestTimeoutMs: "30000",
  maxRewriteBodyBytes: "10485760",
  skipCompressedBody: true,
  allowHttp: true,
  allowConnect: true,
  connectTimeoutMs: "10000",
  idleTimeoutMs: "60000",
  headerRules: [],
  bodyRewriteRules: [],
  notes: "",
};

/** 默认本地工具服务接口路由草稿。 */
export const defaultToolServiceRouteDraft: ToolServiceRouteDraft = {
  id: "route-1",
  method: "GET",
  path: "/api/hello",
  responseStatus: "200",
  contentType: "application/json; charset=utf-8",
  contentSource: "inline",
  body: "{\n  \"ok\": true\n}",
  filePath: "",
};

/** 默认本地工具服务草稿。 */
export const defaultToolServiceDraft: ToolServiceDraft = {
  name: "HTTP",
  host: "127.0.0.1",
  port: "18080",
  staticRootDir: "",
  staticMode: "directory",
  staticPathPrefix: "/",
  routes: [defaultToolServiceRouteDraft],
};

/** 默认 SSH 配置草稿。 */
export const defaultSshDraft: SshDraft = {
  name: "",
  host: "",
  port: "22",
  username: "",
  authType: "password",
  password: "",
  hasPassword: false,
  privateKeyPath: "",
  privateKeyPassphrase: "",
  knownHostsMode: "accept_new",
  knownHostsPath: "",
  connectTimeoutMs: "10000",
  keepaliveIntervalMs: "30000",
  jumpProfileId: "",
};

/** 新增 Header 规则的默认值。 */
export const defaultHeaderRuleDraft: HeaderRuleInput = {
  phase: "request",
  action: "set",
  name: "",
  value: "",
  enabled: true,
  sortOrder: 0,
};

/** 新增 Body 改写规则的默认值。 */
export const defaultBodyRewriteRuleDraft: BodyRewriteRuleInput = {
  bodyType: "auto",
  path: "",
  valueJson: "\"\"",
  enabled: true,
  sortOrder: 0,
};

/** 把服务表单转换为后端 create/update 输入；返回字符串表示用户可见校验错误。 */
export function parseServiceDraft(draft: ServiceDraft): CreateServiceInput | string {
  if (!draft.name.trim()) return "服务名称不能为空。";
  const listenPort = parsePort(draft.listenPort);
  if (!listenPort) return "监听端口必须是 1-65535。";

  const headerRules = normalizeHeaderRules(draft.headerRules);
  if (typeof headerRules === "string") return headerRules;
  const bodyRules = normalizeBodyRewriteRules(draft.bodyRewriteRules);
  if (typeof bodyRules === "string") return bodyRules;

  const input: CreateServiceInput = {
    name: draft.name.trim(),
    kind: draft.kind,
    enabled: true,
    autoStart: draft.autoStart,
    listenHost: draft.listenHost.trim() || "127.0.0.1",
    listenPort,
    notes: draft.notes.trim(),
    headerRules,
    bodyRewriteRules: bodyRules,
  };

  if (draft.kind === "http_reverse") {
    if (!isValidHttpUrl(draft.targetUrl)) {
      return "HTTP 反向代理目标 URL 必须是合法的 http 或 https 地址。";
    }
    const requestTimeoutMs = parsePositiveInteger(draft.requestTimeoutMs, "请求超时");
    if (typeof requestTimeoutMs === "string") return requestTimeoutMs;
    const maxRewriteBodyBytes = parsePositiveInteger(draft.maxRewriteBodyBytes, "最大改写 body 字节数");
    if (typeof maxRewriteBodyBytes === "string") return maxRewriteBodyBytes;
    input.httpReverse = {
      targetUrl: draft.targetUrl.trim(),
      preserveHost: draft.preserveHost,
      requestTimeoutMs,
      maxRewriteBodyBytes,
      skipCompressedBody: draft.skipCompressedBody,
    };
  } else if (draft.kind === "http_forward") {
    const connectTimeoutMs = parsePositiveInteger(draft.connectTimeoutMs, "连接超时");
    if (typeof connectTimeoutMs === "string") return connectTimeoutMs;
    const idleTimeoutMs = parseNonNegativeInteger(draft.idleTimeoutMs, "空闲超时");
    if (typeof idleTimeoutMs === "string") return idleTimeoutMs;
    input.httpForward = {
      allowHttp: draft.allowHttp,
      allowConnect: draft.allowConnect,
      connectTimeoutMs,
      idleTimeoutMs,
    };
  } else if (draft.kind === "tcp_forward") {
    const targetPort = parsePort(draft.targetPort);
    if (!draft.targetHost.trim() || !targetPort) return "TCP 目标 host 和 port 必填。";
    const connectTimeoutMs = parsePositiveInteger(draft.connectTimeoutMs, "连接超时");
    if (typeof connectTimeoutMs === "string") return connectTimeoutMs;
    const idleTimeoutMs = parseNonNegativeInteger(draft.idleTimeoutMs, "空闲超时");
    if (typeof idleTimeoutMs === "string") return idleTimeoutMs;
    input.tcpForward = {
      targetHost: draft.targetHost.trim(),
      targetPort,
      connectTimeoutMs,
      idleTimeoutMs,
    };
  } else if (draft.kind === "udp_forward") {
    const targetPort = parsePort(draft.targetPort);
    if (!draft.targetHost.trim() || !targetPort) return "UDP 目标 host 和 port 必填。";
    const idleTimeoutMs = parsePositiveInteger(draft.idleTimeoutMs, "空闲超时");
    if (typeof idleTimeoutMs === "string") return idleTimeoutMs;
    input.udpForward = {
      targetHost: draft.targetHost.trim(),
      targetPort,
      idleTimeoutMs,
    };
  } else if (draft.kind === "ssh_local") {
    const targetPort = parsePort(draft.targetPort);
    if (!draft.sshProfileId || !draft.targetHost.trim() || !targetPort) {
      return "SSH 隧道需要 SSH 配置、目标主机和目标端口。";
    }
    input.sshTunnel = {
      sshProfileId: draft.sshProfileId,
      tunnelType: "local",
      targetHost: draft.targetHost.trim(),
      targetPort,
      remoteBindHost: null,
      remoteBindPort: null,
    };
  } else if (draft.kind === "ssh_remote") {
    const targetPort = parsePort(draft.targetPort);
    if (!draft.sshProfileId || !draft.targetHost.trim() || !targetPort) {
      return "SSH 远程隧道需要 SSH 配置、本地目标主机和本地目标端口。";
    }
    input.sshTunnel = {
      sshProfileId: draft.sshProfileId,
      tunnelType: "remote",
      targetHost: draft.targetHost.trim(),
      targetPort,
      remoteBindHost: input.listenHost,
      remoteBindPort: listenPort,
    };
  } else if (draft.kind === "ssh_socks") {
    if (!draft.sshProfileId) {
      return "SSH SOCKS5 动态代理需要 SSH 配置。";
    }
    input.sshTunnel = {
      sshProfileId: draft.sshProfileId,
      tunnelType: "socks",
      targetHost: null,
      targetPort: null,
      remoteBindHost: null,
      remoteBindPort: null,
    };
  }

  return input;
}

/** 把 SSH 配置表单转换为后端输入；返回字符串表示用户可见校验错误。 */
export function parseSshDraft(draft: SshDraft): SshProfileInput | string {
  const port = parsePort(draft.port);
  if (!draft.name.trim() || !draft.host.trim() || !draft.username.trim() || !port) {
    return "SSH 名称、主机、端口和用户名必填。";
  }
  if (draft.authType === "private_key" && !draft.privateKeyPath.trim()) {
    return "私钥认证需要填写私钥路径。";
  }
  if (draft.authType === "password" && !draft.hasPassword && !draft.password.trim()) {
    return "密码认证需要填写密码。";
  }
  const connectTimeoutMs = parsePositiveInteger(draft.connectTimeoutMs, "SSH 连接超时");
  if (typeof connectTimeoutMs === "string") return connectTimeoutMs;
  const keepaliveIntervalMs = parsePositiveInteger(draft.keepaliveIntervalMs, "SSH keepalive 间隔");
  if (typeof keepaliveIntervalMs === "string") return keepaliveIntervalMs;
  return {
    name: draft.name.trim(),
    host: draft.host.trim(),
    port,
    username: draft.username.trim(),
    authType: draft.authType,
    password: draft.password || null,
    privateKeyPath: draft.privateKeyPath.trim() || null,
    privateKeyPassphrase: draft.privateKeyPassphrase || null,
    knownHostsMode: draft.knownHostsMode,
    knownHostsPath: draft.knownHostsPath.trim() || null,
    connectTimeoutMs,
    keepaliveIntervalMs,
    jumpProfileId: draft.jumpProfileId || null,
  };
}

/** 把本地工具服务表单转换为后端启动输入；返回字符串表示用户可见校验错误。 */
export function parseToolServiceDraft(draft: ToolServiceDraft): ToolServiceInput | string {
  if (!draft.name.trim()) return "服务名称不能为空。";
  const port = parsePort(draft.port);
  if (!port) return "监听端口必须是 1-65535。";
  const staticRootDir = draft.staticRootDir.trim();
  const routes = normalizeToolServiceRoutes(draft.routes);
  if (typeof routes === "string") return routes;
  if (!staticRootDir && routes.length === 0) return "请至少配置静态目录或一个接口。";
  return {
    name: draft.name.trim(),
    host: draft.host.trim() || "127.0.0.1",
    port,
    staticRootDir: staticRootDir || null,
    staticMode: draft.staticMode,
    staticPathPrefix: normalizeHttpPath(draft.staticPathPrefix),
    routes,
  };
}

/** 把本地工具服务完整配置回填到编辑表单。 */
export function toolServiceConfigToDraft(config: ToolServiceConfig): ToolServiceDraft {
  return {
    name: config.name,
    host: config.host,
    port: String(config.port),
    staticRootDir: config.staticRootDir ?? "",
    staticMode: config.staticMode,
    staticPathPrefix: config.staticPathPrefix,
    routes: config.routes.map((route, index) => ({
      id: `route-${index + 1}`,
      method: route.method,
      path: route.path,
      responseStatus: String(route.responseStatus),
      contentType: route.contentType,
      contentSource: route.contentSource,
      body: route.body ?? "",
      filePath: route.filePath ?? "",
    })),
  };
}

/** 把后端服务详情回填到编辑表单。 */
export function serviceDetailToDraft(detail: ServiceDetail): ServiceDraft {
  const remoteBindHost =
    detail.kind === "ssh_remote" ? (detail.sshTunnel?.remoteBindHost ?? detail.listenHost) : detail.listenHost;
  const remoteBindPort =
    detail.kind === "ssh_remote" ? (detail.sshTunnel?.remoteBindPort ?? detail.listenPort) : detail.listenPort;
  return {
    name: detail.name,
    kind: detail.kind,
    listenHost: remoteBindHost,
    listenPort: String(remoteBindPort),
    targetUrl: detail.httpReverse?.targetUrl ?? defaultServiceDraft.targetUrl,
    targetHost:
      detail.tcpForward?.targetHost ??
      detail.udpForward?.targetHost ??
      detail.sshTunnel?.targetHost ??
      defaultServiceDraft.targetHost,
    targetPort: String(
      detail.tcpForward?.targetPort ??
        detail.udpForward?.targetPort ??
        detail.sshTunnel?.targetPort ??
        defaultServiceDraft.targetPort,
    ),
    sshProfileId: detail.sshTunnel?.sshProfileId ?? "",
    autoStart: detail.autoStart,
    preserveHost: detail.httpReverse?.preserveHost ?? false,
    requestTimeoutMs: String(detail.httpReverse?.requestTimeoutMs ?? defaultServiceDraft.requestTimeoutMs),
    maxRewriteBodyBytes: String(detail.httpReverse?.maxRewriteBodyBytes ?? defaultServiceDraft.maxRewriteBodyBytes),
    skipCompressedBody: detail.httpReverse?.skipCompressedBody ?? true,
    allowHttp: detail.httpForward?.allowHttp ?? true,
    allowConnect: detail.httpForward?.allowConnect ?? true,
    connectTimeoutMs: String(
      detail.httpForward?.connectTimeoutMs ??
        detail.tcpForward?.connectTimeoutMs ??
        defaultServiceDraft.connectTimeoutMs,
    ),
    idleTimeoutMs: String(
      detail.httpForward?.idleTimeoutMs ??
        detail.tcpForward?.idleTimeoutMs ??
        detail.udpForward?.idleTimeoutMs ??
        defaultServiceDraft.idleTimeoutMs,
    ),
    headerRules: detail.headerRules.map(toHeaderRuleInput),
    bodyRewriteRules: detail.bodyRewriteRules.map(toBodyRuleInput),
    notes: detail.notes,
  };
}

/** 把后端 SSH 配置回填到编辑表单，不回填 secret。 */
export function sshProfileToDraft(profile: SshProfile): SshDraft {
  return {
    name: profile.name,
    host: profile.host,
    port: String(profile.port),
    username: profile.username,
    authType: profile.authType,
    password: "",
    hasPassword: profile.hasPassword,
    privateKeyPath: profile.privateKeyPath ?? "",
    privateKeyPassphrase: "",
    knownHostsMode: profile.knownHostsMode,
    knownHostsPath: profile.knownHostsPath ?? "",
    connectTimeoutMs: String(profile.connectTimeoutMs),
    keepaliveIntervalMs: String(profile.keepaliveIntervalMs),
    jumpProfileId: profile.jumpProfileId ?? "",
  };
}

/** 按当前页面筛选服务列表。 */
export function filterServicesByPage(services: ServiceSummary[], page: PageKey): ServiceSummary[] {
  if (page === "forwarding") {
    return services.filter(
      (service) =>
        service.kind === "http_reverse" ||
        service.kind === "http_forward" ||
        service.kind === "tcp_forward" ||
        service.kind === "udp_forward",
    );
  }
  if (page === "ssh") {
    return services.filter(
      (service) => service.kind === "ssh_local" || service.kind === "ssh_remote" || service.kind === "ssh_socks",
    );
  }
  return services;
}

/** 根据服务类型定位编辑页面。 */
export function pageForServiceKind(kind: ServiceKind): PageKey {
  if (kind === "http_reverse" || kind === "http_forward" || kind === "tcp_forward" || kind === "udp_forward") {
    return "forwarding";
  }
  if (kind === "ssh_local" || kind === "ssh_remote" || kind === "ssh_socks") return "ssh";
  return "dashboard";
}

/** 页面标题。 */
export function pageTitle(page: PageKey): string {
  const titles: Record<PageKey, string> = {
    dashboard: "仪表盘",
    services: "本地服务",
    forwarding: "网络转发",
    ssh: "SSH",
    system: "系统代理",
    settings: "设置",
  };
  return titles[page];
}

/** 日志中展示服务名，找不到时展示 id 或 system。 */
export function serviceName(services: ServiceSummary[], id?: string | null): string {
  return services.find((service) => service.id === id)?.name ?? id ?? "system";
}

/** 把未知错误转换为用户可读短句。 */
export function readError(error: unknown): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "未知错误";
}

/** 格式化时间，非法时间保持原值。 */
export function formatTime(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

/** 格式化字节数。 */
export function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

function toHeaderRuleInput(rule: ServiceDetail["headerRules"][number]): HeaderRuleInput {
  return {
    phase: rule.phase,
    action: rule.action,
    name: rule.name,
    value: rule.value ?? null,
    enabled: rule.enabled,
    sortOrder: rule.sortOrder,
  };
}

function toBodyRuleInput(rule: ServiceDetail["bodyRewriteRules"][number]): BodyRewriteRuleInput {
  return {
    bodyType: rule.bodyType,
    path: rule.path,
    valueJson: rule.valueJson,
    enabled: rule.enabled,
    sortOrder: rule.sortOrder,
  };
}

function normalizeHeaderRules(rules: HeaderRuleInput[]): HeaderRuleInput[] | string {
  for (const [index, rule] of rules.entries()) {
    if (!rule.name.trim()) {
      return `Header 规则 #${index + 1} 缺少名称。`;
    }
    if (rule.action === "set" && !String(rule.value ?? "").trim()) {
      return `Header 规则 #${index + 1} 设置值不能为空。`;
    }
  }
  return rules.map((rule) => ({
    ...rule,
    name: rule.name.trim(),
    value: rule.action === "remove" ? null : String(rule.value ?? "").trim(),
    sortOrder: Number.isFinite(rule.sortOrder) ? rule.sortOrder : 0,
  }));
}

function normalizeBodyRewriteRules(rules: BodyRewriteRuleInput[]): BodyRewriteRuleInput[] | string {
  for (const [index, rule] of rules.entries()) {
    if (!rule.path.trim()) {
      return `Body 改写规则 #${index + 1} 缺少路径。`;
    }
    try {
      JSON.parse(rule.valueJson);
    } catch (error) {
      return `Body 改写规则 #${index + 1} 的 value 不是合法 JSON: ${readError(error)}`;
    }
  }
  return rules.map((rule) => ({
    ...rule,
    path: rule.path.trim(),
    valueJson: rule.valueJson.trim(),
    sortOrder: Number.isFinite(rule.sortOrder) ? rule.sortOrder : 0,
  }));
}

function normalizeToolServiceRoutes(routes: ToolServiceRouteDraft[]): ToolServiceRouteInput[] | string {
  const normalized: ToolServiceRouteInput[] = [];
  for (const [index, route] of routes.entries()) {
    const label = `接口 #${index + 1}`;
    if (!isToolServiceMethod(route.method)) {
      return `${label} 的请求方法无效。`;
    }
    if (!route.path.trim()) {
      return `${label} 的请求路径不能为空。`;
    }
    const responseStatus = parseHttpStatus(route.responseStatus);
    if (typeof responseStatus === "string") return `${label} ${responseStatus}`;
    const contentType = route.contentType.trim() || "application/json; charset=utf-8";
    if (route.contentSource === "file" && !route.filePath.trim()) {
      return `${label} 需要选择响应文件。`;
    }
    if (route.contentSource === "inline" && contentType.toLowerCase().includes("json")) {
      try {
        JSON.parse(route.body);
      } catch (error) {
        return `${label} 的响应结果不是合法 JSON: ${readError(error)}`;
      }
    }
    normalized.push({
      method: route.method,
      path: normalizeHttpPath(route.path),
      responseStatus,
      contentType,
      contentSource: route.contentSource,
      body: route.contentSource === "inline" ? route.body : null,
      filePath: route.contentSource === "file" ? route.filePath.trim() : null,
    });
  }
  return normalized;
}

function isToolServiceMethod(value: string): value is ToolServiceMethod {
  return ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "ANY"].includes(value);
}

function parsePort(value: string): number | null {
  const port = Number(value);
  return Number.isInteger(port) && port >= 1 && port <= 65535 ? port : null;
}

function parseHttpStatus(value: string): number | string {
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed < 100 || parsed > 599) {
    return "响应状态码必须是 100-599。";
  }
  return parsed;
}

function normalizeHttpPath(value: string): string {
  const trimmed = value.trim();
  if (!trimmed || trimmed === "/") return "/";
  const withLeading = trimmed.startsWith("/") ? trimmed : `/${trimmed}`;
  return withLeading.replace(/\/+$/u, "") || "/";
}

function formatAccessAddress(host: string, port: number, format: "hostPort" | "url", path: string): string {
  const hostPort = `${formatAddressHost(host)}:${port}`;
  if (format === "hostPort") return hostPort;
  return `http://${hostPort}${normalizeUrlPathSuffix(path)}`;
}

function normalizeLanCopyHost(lanIp?: string | null): string {
  return lanIp?.trim() || lanBindHost;
}

function addressCopyPayload(entries: AccessAddress[], lanIp?: string | null): AddressCopyPayload {
  const localEntry = entries.find((entry) => entry.label === "本地");
  const lanEntry = entries.find((entry) => entry.label === "局域网");
  if (localEntry && lanEntry) {
    if (lanIp?.trim()) {
      return {
        text: lanEntry.value,
        detail: `已复制局域网地址；本机也可用 ${localEntry.value}`,
      };
    }
    return {
      text: localEntry.value,
      detail: `未识别到局域网 IP，已复制本地地址 ${localEntry.value}`,
    };
  }
  const entry = entries[0] ?? { label: "本地" as const, value: "" };
  return {
    text: entry.value,
    detail: `${entry.label}地址：${entry.value}`,
  };
}

function formatAddressHost(host: string): string {
  return host.includes(":") && !host.startsWith("[") ? `[${host}]` : host;
}

function normalizeUrlPathSuffix(path: string): string {
  if (!path || path === "/") return "/";
  return path.startsWith("/") ? path : `/${path}`;
}

function urlPathSuffix(url: string): string {
  try {
    const parsed = new URL(url);
    return `${parsed.pathname}${parsed.search}${parsed.hash}`;
  } catch {
    return "/";
  }
}

function parsePositiveInteger(value: string, label: string): number | string {
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    return `${label}必须是大于 0 的整数。`;
  }
  return parsed;
}

function parseNonNegativeInteger(value: string, label: string): number | string {
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed < 0) {
    return `${label}必须是大于等于 0 的整数。`;
  }
  return parsed;
}

function isValidHttpUrl(value: string): boolean {
  try {
    const url = new URL(value.trim());
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}
