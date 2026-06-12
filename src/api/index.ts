/**
 * @author kongweiguang
 * Tauri Command API 封装。组件只依赖这些语义化方法，不散落 invoke 字符串。
 */

import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type DownloadEvent } from "@tauri-apps/plugin-updater";
import { localFallback } from "./previewFallback";
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
  ToolServiceInput,
  ToolServiceConfig,
  ToolServiceSummary,
} from "../types";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

type FileDialogFilter = { name: string; extensions: string[] };

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

/** 本地工具服务 API。 */
export const toolServicesApi = {
  /** 查询已保存的本地工具服务，并合并运行态。 */
  list: () => call<ToolServiceSummary[]>("list_tool_services"),
  /** 读取本地工具服务完整配置。 */
  get: (id: string) => call<ToolServiceConfig>("get_tool_service", { id }),
  /** 创建本地工具服务配置并立即启动。 */
  create: (input: ToolServiceInput) =>
    call<ToolServiceSummary>("create_tool_service", { input }),
  /** 更新本地工具服务配置；运行中服务会用新配置重启。 */
  update: (id: string, input: ToolServiceInput) =>
    call<ToolServiceSummary>("update_tool_service", { id, input }),
  /** 启动已保存的本地工具服务。 */
  start: (id: string) => call<ToolServiceSummary>("start_tool_service", { id }),
  /** 暂停本地工具服务，配置仍保留。 */
  stop: (id: string) => call<RuntimeStatus>("stop_tool_service", { id }),
  /** 删除本地工具服务配置。 */
  delete: (id: string) => call<void>("delete_tool_service", { id }),
  /** 选择本地目录。浏览器预览模式下返回空值。 */
  chooseDirectory: () => fileDialogApi.chooseDirectory(),
  /** 选择本地响应文件。浏览器预览模式下返回空值。 */
  chooseFile: () => fileDialogApi.chooseFile(),
};

/** 本地文件路径选择 API。 */
export const fileDialogApi = {
  /** 选择本地目录。浏览器预览模式下返回空值。 */
  async chooseDirectory(): Promise<string | null> {
    if (!isTauriRuntime()) {
      return null;
    }
    const selected = await open({ directory: true, multiple: false });
    return typeof selected === "string" ? selected : null;
  },
  /** 选择本地文件。浏览器预览模式下返回空值。 */
  async chooseFile(filters?: FileDialogFilter[]): Promise<string | null> {
    if (!isTauriRuntime()) {
      return null;
    }
    const selected = await open({ multiple: false, filters });
    return typeof selected === "string" ? selected : null;
  },
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

/** 本机网络信息 API。 */
export const networkApi = {
  /** 获取默认路由对应的局域网 IPv4；无法判断时返回 null。 */
  getLanIp: () => call<string | null>("get_lan_ip"),
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
