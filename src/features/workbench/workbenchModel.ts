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
} from "../../types";

/** Workbench 导航页签。 */
export type PageKey =
  | "dashboard"
  | "http"
  | "forwarding"
  | "ssh"
  | "system"
  | "settings";

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
  if (page === "http") {
    return services.filter((service) => service.kind === "http_reverse" || service.kind === "http_forward");
  }
  if (page === "forwarding") {
    return services.filter((service) => service.kind === "tcp_forward" || service.kind === "udp_forward");
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
  if (kind === "http_reverse" || kind === "http_forward") return "http";
  if (kind === "tcp_forward" || kind === "udp_forward") return "forwarding";
  if (kind === "ssh_local" || kind === "ssh_remote" || kind === "ssh_socks") return "ssh";
  return "dashboard";
}

/** 页面标题。 */
export function pageTitle(page: PageKey): string {
  const titles: Record<PageKey, string> = {
    dashboard: "仪表盘",
    http: "HTTP",
    forwarding: "端口转发",
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

function parsePort(value: string): number | null {
  const port = Number(value);
  return Number.isInteger(port) && port >= 1 && port <= 65535 ? port : null;
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
