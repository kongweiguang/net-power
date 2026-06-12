/**
 * @author kongweiguang
 * Tauri Command API 封装。组件只依赖这些语义化方法，不散落 invoke 字符串。
 */

import { invoke } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type DownloadEvent } from "@tauri-apps/plugin-updater";
import type {
  AppSetting,
  AppUpdateProgress,
  AppUpdateResult,
  AutostartStatus,
  CreateServiceInput,
  LogFilter,
  LogRow,
  RuntimeStatus,
  ServiceDetail,
  ServiceRuntimeSummary,
  ServiceSummary,
  SshProfile,
  SshProfileInput,
  SystemProxyProfile,
  SystemProxyProfileInput,
  SystemProxyStatus,
  SystemProxyTarget,
  TestResult,
} from "../types";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

/** 应用设置 API。 */
export const settingsApi = {
  /** 读取全部应用设置。 */
  list: () => call<AppSetting[]>("get_app_settings"),
  /** 更新单个设置。valueJson 必须是合法 JSON 字符串。 */
  update: (key: string, valueJson: string) =>
    call<void>("update_app_setting", { key, valueJson }),
};

/** 开机启动 API。 */
export const autostartApi = {
  /** 读取系统登录启动状态。 */
  getStatus: () => call<AutostartStatus>("get_autostart_status"),
  /** 设置系统登录启动。 */
  setEnabled: (enabled: boolean) =>
    call<AutostartStatus>("set_autostart", { enabled }),
};

/** 服务管理 API。 */
export const servicesApi = {
  /** 获取服务摘要列表。 */
  list: () => call<ServiceSummary[]>("list_services"),
  /** 获取服务详情。 */
  get: (id: string) => call<ServiceDetail>("get_service", { id }),
  /** 创建服务。 */
  create: (input: CreateServiceInput) =>
    call<ServiceDetail>("create_service", { input }),
  /** 更新服务。 */
  update: (id: string, input: CreateServiceInput) =>
    call<ServiceDetail>("update_service", { id, input }),
  /** 删除服务。 */
  delete: (id: string) => call<void>("delete_service", { id }),
  /** 复制服务。 */
  duplicate: (id: string) => call<ServiceDetail>("duplicate_service", { id }),
  /** 启动服务。 */
  start: (id: string) => call<RuntimeStatus>("start_service", { id }),
  /** 停止服务。 */
  stop: (id: string) => call<RuntimeStatus>("stop_service", { id }),
  /** 重启服务。 */
  restart: (id: string) => call<RuntimeStatus>("restart_service", { id }),
  /** 查询运行态。 */
  listRuntimeStatus: () =>
    call<ServiceRuntimeSummary[]>("list_runtime_status"),
  /** 测试服务。 */
  test: (id: string) => call<TestResult>("test_service", { id }),
};

/** SSH Profile API。 */
export const sshProfilesApi = {
  /** 获取全部 SSH Profile。 */
  list: () => call<SshProfile[]>("list_ssh_profiles"),
  /** 获取单个 SSH Profile。 */
  get: (id: string) => call<SshProfile>("get_ssh_profile", { id }),
  /** 创建 SSH Profile。 */
  create: (input: SshProfileInput) =>
    call<SshProfile>("create_ssh_profile", { input }),
  /** 更新 SSH Profile。 */
  update: (id: string, input: SshProfileInput) =>
    call<SshProfile>("update_ssh_profile", { id, input }),
  /** 删除 SSH Profile。 */
  delete: (id: string) => call<void>("delete_ssh_profile", { id }),
  /** 测试 SSH Profile。 */
  test: (id: string) => call<TestResult>("test_ssh_profile", { id }),
};

/** 日志 API。 */
export const logsApi = {
  /** 查询服务日志。 */
  list: (filter: LogFilter = { limit: 300 }) =>
    call<LogRow[]>("list_logs", { filter }),
  /** 清理日志。 */
  clear: (serviceId?: string | null) =>
    call<void>("clear_logs", { serviceId: serviceId ?? null }),
};

/** 系统代理 API。 */
export const systemProxyApi = {
  /** 获取全部系统代理配置档。 */
  listProfiles: () => call<SystemProxyProfile[]>("list_system_proxy_profiles"),
  /** 创建系统代理配置档。 */
  createProfile: (input: SystemProxyProfileInput) =>
    call<SystemProxyProfile>("create_system_proxy_profile", { input }),
  /** 更新系统代理配置档。 */
  updateProfile: (id: string, input: SystemProxyProfileInput) =>
    call<SystemProxyProfile>("update_system_proxy_profile", { id, input }),
  /** 删除系统代理配置档。 */
  deleteProfile: (id: string) => call<void>("delete_system_proxy_profile", { id }),
  /** 通过 HTTP forward 服务 id 或系统代理配置 id 设置系统代理。 */
  set: (profileId: string) =>
    call<SystemProxyStatus>("set_system_proxy", { profileId }),
  /** 通过手动目标设置系统代理。 */
  setTarget: (target: SystemProxyTarget) =>
    call<SystemProxyStatus>("set_system_proxy_target", { target }),
  /** 清理系统代理。 */
  clear: () => call<SystemProxyStatus>("clear_system_proxy"),
  /** 查询系统代理状态。 */
  getStatus: () => call<SystemProxyStatus>("get_system_proxy_status"),
};

/** 应用更新 API。 */
export const updaterApi = {
  /** 从 Tauri updater endpoint 检查新版本，下载并安装后尝试重启应用。 */
  async checkDownloadInstall(onProgress?: (progress: AppUpdateProgress) => void): Promise<AppUpdateResult> {
    if (!isTauriRuntime()) {
      throw new Error("应用更新需要在 Tauri 桌面环境中执行。");
    }
    const update = await check();
    if (!update) {
      return { updated: false };
    }

    let downloadedBytes = 0;
    let totalBytes: number | null = null;
    await update.downloadAndInstall((event) => {
      const progress = normalizeUpdateProgress(event, downloadedBytes, totalBytes);
      downloadedBytes = progress.downloadedBytes ?? downloadedBytes;
      totalBytes = progress.totalBytes ?? totalBytes;
      onProgress?.(progress);
    });

    try {
      await relaunch();
      return {
        updated: true,
        currentVersion: update.currentVersion,
        version: update.version,
        body: update.body ?? null,
        relaunchTriggered: true,
      };
    } catch (error) {
      return {
        updated: true,
        currentVersion: update.currentVersion,
        version: update.version,
        body: update.body ?? null,
        relaunchTriggered: false,
        relaunchError: normalizePlainError(error),
      };
    }
  },
};

/** 标准化 invoke 错误，避免 UI 暴露未知对象。 */
async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauriRuntime()) {
    return localFallback<T>(command, args);
  }
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw new Error(normalizeError(command, error));
  }
}

function normalizeUpdateProgress(
  event: DownloadEvent,
  downloadedBytes: number,
  totalBytes: number | null,
): AppUpdateProgress {
  if (event.event === "Started") {
    const nextTotal = event.data.contentLength ?? null;
    return {
      phase: "started",
      message: nextTotal ? `开始下载更新包，共 ${formatUpdateBytes(nextTotal)}。` : "开始下载更新包。",
      downloadedBytes: 0,
      totalBytes: nextTotal,
    };
  }
  if (event.event === "Progress") {
    const nextDownloaded = downloadedBytes + event.data.chunkLength;
    const suffix = totalBytes ? ` / ${formatUpdateBytes(totalBytes)}` : "";
    return {
      phase: "progress",
      message: `已下载 ${formatUpdateBytes(nextDownloaded)}${suffix}。`,
      downloadedBytes: nextDownloaded,
      totalBytes,
    };
  }
  return {
    phase: "finished",
    message: "更新包下载完成，正在安装并重启。",
    downloadedBytes,
    totalBytes,
  };
}

function formatUpdateBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

/** 判断当前页面是否运行在 Tauri WebView 内。 */
export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** 浏览器开发模式下的只读 fallback，避免本地视觉检查被 IPC 错误打断。 */
function localFallback<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const preview = visualSmokePreviewData();
  const readonly: Record<string, unknown> = {
    get_app_settings: [
      { key: "app.initialized", valueJson: "true" },
      { key: "logs.retention_days", valueJson: "7" },
      { key: "logs.max_rows", valueJson: "20000" },
      { key: "services.auto_start_enabled", valueJson: "false" },
    ] satisfies AppSetting[],
    get_autostart_status: {
      enabled: false,
      supported: false,
      message: "浏览器预览模式：开机启动状态仅在 Tauri 桌面环境读取。",
    } satisfies AutostartStatus,
    list_services: preview?.services ?? [],
    list_runtime_status: preview?.runtime ?? [],
    list_ssh_profiles: preview?.profiles ?? [],
    list_system_proxy_profiles: preview?.proxyProfiles ?? [],
    list_logs: preview?.logs ?? [],
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

interface VisualSmokePreviewData {
  services: ServiceSummary[];
  runtime: ServiceRuntimeSummary[];
  profiles: SshProfile[];
  proxyProfiles: SystemProxyProfile[];
  logs: LogRow[];
  proxyStatus: SystemProxyStatus;
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

/** 把 Tauri 返回的任意错误转成短句。 */
function normalizeError(command: string, error: unknown): string {
  if (error instanceof Error && error.message.trim()) {
    return `${command}: ${error.message}`;
  }
  if (typeof error === "string" && error.trim()) {
    return `${command}: ${error}`;
  }
  return `${command}: 后端命令执行失败`;
}

function normalizePlainError(error: unknown): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  if (typeof error === "string" && error.trim()) {
    return error;
  }
  return "未知错误";
}
