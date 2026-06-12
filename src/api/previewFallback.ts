/**
 * @author kongweiguang
 * 浏览器预览模式下的轻量 Tauri Command fallback。
 */

import type {
  AppSetting,
  AutostartStatus,
  LogRow,
  ServiceRuntimeSummary,
  ServiceSummary,
  SshProfile,
  SystemProxyProfile,
  SystemProxyStatus,
  ToolServiceSummary,
} from "../types";

interface VisualSmokePreviewData {
  /** 服务列表预览数据。 */
  services: ServiceSummary[];
  /** 服务运行态预览数据。 */
  runtime: ServiceRuntimeSummary[];
  /** 本地工具服务预览数据。 */
  toolServices: ToolServiceSummary[];
  /** SSH Profile 预览数据。 */
  profiles: SshProfile[];
  /** 系统代理配置档预览数据。 */
  proxyProfiles: SystemProxyProfile[];
  /** 日志预览数据。 */
  logs: LogRow[];
  /** 系统代理状态预览数据。 */
  proxyStatus: SystemProxyStatus;
}

const defaultPreviewSettings: AppSetting[] = [
  { key: "app.initialized", valueJson: "true" },
  { key: "logs.retention_days", valueJson: "7" },
  { key: "logs.max_rows", valueJson: "20000" },
  { key: "services.auto_start_enabled", valueJson: "false" },
  { key: "ui.theme_mode", valueJson: "\"system\"" },
];

let previewSettings = [...defaultPreviewSettings];

/** 浏览器开发模式下的 fallback，避免本地视觉检查被 IPC 错误打断。 */
export function localFallback<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const preview = visualSmokePreviewData();
  const readonly: Record<string, unknown> = {
    get_app_settings: previewSettings,
    get_autostart_status: {
      enabled: false,
      supported: false,
      message: "浏览器预览模式：开机启动状态仅在 Tauri 桌面环境读取。",
    } satisfies AutostartStatus,
    list_services: preview?.services ?? [],
    list_runtime_status: preview?.runtime ?? [],
    list_tool_services: preview?.toolServices ?? [],
    list_ssh_profiles: preview?.profiles ?? [],
    list_system_proxy_profiles: preview?.proxyProfiles ?? [],
    list_logs: preview?.logs ?? [],
    get_lan_ip: "192.168.1.23",
    get_system_proxy_status: preview?.proxyStatus ?? {
      enabled: false,
      proxyHost: "",
      proxyPort: null,
      bypass: "",
      message: "浏览器预览模式：系统代理状态仅在 Tauri 桌面环境读取。",
    } satisfies SystemProxyStatus,
    get_system_info: {
      os: "browser",
      arch: "unknown",
      appVersion: "0.1.0",
      dataDir: "",
    },
  };

  if (command in readonly) {
    return Promise.resolve(readonly[command] as T);
  }

  if (command === "update_app_setting") {
    const key = typeof args?.key === "string" ? args.key : "";
    const valueJson = typeof args?.valueJson === "string" ? args.valueJson : "";
    if (!key.trim()) {
      return Promise.reject(new Error("设置 key 不能为空。"));
    }
    try {
      JSON.parse(valueJson);
    } catch {
      return Promise.reject(new Error("设置值必须是合法 JSON。"));
    }
    previewSettings = upsertPreviewSetting(previewSettings, key, valueJson);
    return Promise.resolve(undefined as T);
  }

  const id = typeof args?.id === "string" ? args.id : "preview";
  if (command === "test_service" || command === "test_ssh_profile") {
    return Promise.resolve({
      ok: false,
      message: "该操作需要在 Tauri 桌面环境中执行。",
      durationMs: 0,
    } as T);
  }
  if (command === "get_service") {
    return Promise.reject(new Error(`浏览器预览模式无法读取服务详情: ${id}`));
  }
  if (command === "create_service" || command === "update_service" || command === "duplicate_service") {
    return Promise.reject(new Error("保存服务需要在 Tauri 桌面环境中执行。"));
  }
  if (
    command === "create_tool_service" ||
    command === "get_tool_service" ||
    command === "update_tool_service" ||
    command === "start_tool_service" ||
    command === "stop_tool_service" ||
    command === "delete_tool_service"
  ) {
    return Promise.reject(new Error("工具服务需要在 Tauri 桌面环境中执行。"));
  }
  if (command === "create_ssh_profile" || command === "update_ssh_profile") {
    return Promise.reject(new Error("保存 SSH Profile 需要在 Tauri 桌面环境中执行。"));
  }
  if (
    command === "create_system_proxy_profile" ||
    command === "update_system_proxy_profile" ||
    command === "delete_system_proxy_profile"
  ) {
    return Promise.reject(new Error("管理系统代理配置档需要在 Tauri 桌面环境中执行。"));
  }
  if (command === "clear_logs") {
    return Promise.resolve(undefined as T);
  }
  if (command === "set_autostart") {
    return Promise.reject(new Error("开机启动需要在 Tauri 桌面环境中设置。"));
  }

  return Promise.reject(new Error(`${command} 需要在 Tauri 桌面环境中执行。`));
}

function upsertPreviewSetting(settings: AppSetting[], key: string, valueJson: string): AppSetting[] {
  if (settings.some((setting) => setting.key === key)) {
    return settings.map((setting) => (setting.key === key ? { ...setting, valueJson } : setting));
  }
  return [...settings, { key, valueJson }];
}

/** 显式视觉冒烟预览数据，只在浏览器 URL 带 preview=visual-smoke 时启用。 */
function visualSmokePreviewData(): VisualSmokePreviewData | null {
  if (typeof window === "undefined") {
    return null;
  }
  if (new URLSearchParams(window.location.search).get("preview") !== "visual-smoke") {
    return null;
  }

  const now = new Date().toISOString();
  const services: ServiceSummary[] = [
    {
      id: "preview-http-forward",
      name: "办公 HTTP 正向代理",
      kind: "http_forward",
      enabled: true,
      autoStart: true,
      listenHost: "127.0.0.1",
      listenPort: 7890,
      targetLabel: "HTTP/CONNECT",
      runtimeStatus: { type: "running" },
      activeConnections: 4,
      totalConnections: 128,
    },
    {
      id: "preview-ssh-socks",
      name: "研发 SSH SOCKS5 动态代理",
      kind: "ssh_socks",
      enabled: true,
      autoStart: false,
      listenHost: "127.0.0.1",
      listenPort: 1080,
      targetLabel: "SOCKS5 动态代理",
      runtimeStatus: { type: "running" },
      activeConnections: 2,
      totalConnections: 42,
    },
    {
      id: "preview-reverse",
      name: "内网 API 反向代理",
      kind: "http_reverse",
      enabled: true,
      autoStart: false,
      listenHost: "127.0.0.1",
      listenPort: 8088,
      targetLabel: "https://api.internal.example.com",
      runtimeStatus: { type: "failed", message: "上游连接超时" },
      activeConnections: 0,
      totalConnections: 19,
    },
  ];
  const runtime: ServiceRuntimeSummary[] = services.map((service, index) => ({
    serviceId: service.id,
    kind: service.kind,
    listenAddr: `${service.listenHost}:${service.listenPort}`,
    runtimeStatus: service.runtimeStatus,
    startedAt: service.runtimeStatus.type === "running" ? now : null,
    activeConnections: service.activeConnections,
    totalConnections: service.totalConnections,
    bytesIn: [2_304_512, 1_048_576, 98_304][index] ?? 0,
    bytesOut: [6_904_512, 3_145_728, 120_832][index] ?? 0,
  }));
  const logs: LogRow[] = [
    {
      id: 1,
      serviceId: "preview-http-forward",
      level: "info",
      message: "HTTP forward CONNECT 已完成",
      metaJson: JSON.stringify({
        protocol: "https",
        method: "CONNECT",
        host: "docs.example.com",
        targetAddr: "docs.example.com:443",
        durationMs: 41,
        bytesIn: 4096,
        bytesOut: 16384,
      }),
      createdAt: now,
    },
    {
      id: 2,
      serviceId: "preview-ssh-socks",
      level: "info",
      message: "SOCKS5 CONNECT 已通过 SSH direct-tcpip",
      metaJson: JSON.stringify({
        protocol: "ssh",
        method: "SOCKS5 CONNECT",
        targetAddr: "db.internal.example.com:5432",
        durationMs: 88,
        bytesIn: 2048,
        bytesOut: 8192,
      }),
      createdAt: now,
    },
    {
      id: 3,
      serviceId: "preview-reverse",
      level: "warn",
      message: "上游响应超时，已记录为失败态",
      metaJson: JSON.stringify({
        protocol: "http",
        method: "GET",
        path: "/v1/accounts",
        statusCode: 504,
        durationMs: 30_000,
      }),
      createdAt: now,
    },
  ];

  return {
    services,
    runtime,
    toolServices: [
      {
        id: "preview-tool-http",
        name: "HTTP 预览",
        host: "127.0.0.1",
        port: 4173,
        url: "http://127.0.0.1:4173/public",
        staticRootDir: "C:/workspace/site",
        staticMode: "directory",
        staticPathPrefix: "/public",
        routeCount: 2,
        startedAt: now,
        totalRequests: 12,
        runtimeStatus: { type: "running" },
      },
    ],
    logs,
    profiles: [
      {
        id: "preview-ssh-profile",
        name: "研发堡垒机",
        host: "bastion.internal.example.com",
        port: 22,
        username: "developer",
        authType: "private_key",
        hasPassword: false,
        privateKeyPath: "C:/Users/example/.ssh/id_ed25519",
        hasPassphrase: true,
        knownHostsMode: "strict",
        knownHostsPath: "C:/Users/example/.ssh/known_hosts",
        connectTimeoutMs: 10_000,
        keepaliveIntervalMs: 30_000,
        jumpProfileId: null,
      },
    ],
    proxyProfiles: [
      {
        id: "preview-proxy-profile",
        name: "本机 HTTP 正向代理",
        proxyHost: "127.0.0.1",
        proxyPort: 7890,
        bypass: "localhost;127.*;*.internal",
        active: true,
        createdAt: now,
        updatedAt: now,
      },
    ],
    proxyStatus: {
      enabled: true,
      proxyHost: "127.0.0.1",
      proxyPort: 7890,
      bypass: "localhost;127.*;*.internal",
      message: "浏览器预览：系统代理状态使用视觉冒烟示例数据。",
    },
  };
}
