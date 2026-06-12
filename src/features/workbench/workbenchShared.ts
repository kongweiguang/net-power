/**
 * @author kongweiguang
 * Workbench 跨组件共享的 UI 常量、草稿类型和轻量工具函数。
 */

import type {
  LogRow,
  RuntimeStatus,
  ServiceKind,
  ServiceSummary,
  SshAuthType,
  SystemProxyProfile,
} from "../../types";
import {
  defaultToolServiceDraft,
  defaultToolServiceRouteDraft,
  type PageKey,
  type ServiceDraft,
  type SshDraft,
  type ToolServiceDraft,
  type ToolServiceRouteDraft,
} from "./workbenchModel";

/** 轻量 toast 展示数据。 */
export interface Toast {
  id: number;
  kind: "success" | "error" | "info";
  title: string;
  detail?: string;
}

/** 系统代理配置表单草稿。 */
export interface SystemProxyProfileDraft {
  name: string;
  proxyHost: string;
  proxyPort: string;
  bypass: string;
}

/** 系统代理可启用来源：运行中的 HTTP Forward 服务或保存的配置档。 */
export type SystemProxySource =
  | {
      key: string;
      type: "service";
      id: string;
      name: string;
      target: string;
      detail: string;
      enabled: boolean;
      service: ServiceSummary;
    }
  | {
      key: string;
      type: "profile";
      id: string;
      name: string;
      target: string;
      detail: string;
      enabled: boolean;
      profile: SystemProxyProfile;
    };

/** 自定义选择器选项。 */
export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

/** 系统代理配置表单默认值。 */
export const defaultSystemProxyProfileDraft: SystemProxyProfileDraft = {
  name: "",
  proxyHost: "127.0.0.1",
  proxyPort: "7890",
  bypass: "localhost;127.*",
};

/** 运行状态用户可见标签。 */
export const runtimeStatusLabels: Record<RuntimeStatus["type"], string> = {
  stopped: "已停止",
  starting: "启动中",
  running: "运行中",
  stopping: "停止中",
  failed: "失败",
};

/** 日志级别用户可见标签。 */
export const logLevelLabels: Record<LogRow["level"], string> = {
  trace: "跟踪",
  debug: "调试",
  info: "信息",
  warn: "警告",
  error: "错误",
};

/** SSH 认证方式用户可见标签。 */
export const sshAuthLabels: Record<SshAuthType, string> = {
  password: "密码",
  private_key: "私钥",
  agent: "Agent",
};

/** known_hosts 校验策略用户可见标签。 */
export const knownHostModeLabels: Record<SshDraft["knownHostsMode"], string> = {
  accept_new: "首次连接自动信任",
  strict: "严格校验",
  insecure_skip: "跳过校验",
};

/** 合并 className，过滤空值。 */
export function cx(...classes: Array<string | false | null | undefined>): string {
  return classes.filter(Boolean).join(" ");
}

/** 返回某个页面允许创建的代理服务类型。 */
export function serviceKindsForPage(page: PageKey): ServiceKind[] | null {
  if (page === "forwarding") return ["http_reverse", "http_forward", "tcp_forward", "udp_forward"];
  if (page === "ssh") return ["ssh_local", "ssh_remote", "ssh_socks"];
  return null;
}

/** 将服务草稿规整到当前页面允许的服务类型范围。 */
export function normalizeDraftForPage(page: PageKey, draft: ServiceDraft): ServiceDraft {
  const allowedKinds = serviceKindsForPage(page);
  if (!allowedKinds || allowedKinds.includes(draft.kind)) {
    return draft;
  }
  return { ...draft, kind: allowedKinds[0] };
}

/** 创建新的已保存 HTTP 工具服务草稿。 */
export function newToolServiceDraft(): ToolServiceDraft {
  return {
    ...defaultToolServiceDraft,
    routes: defaultToolServiceDraft.routes.map((route) => ({ ...route })),
  };
}

/** 创建新的 HTTP 工具服务路由草稿。 */
export function newToolServiceRoute(index: number): ToolServiceRouteDraft {
  return {
    ...defaultToolServiceRouteDraft,
    id: `route-${Date.now()}-${index}`,
    path: `/api/route-${index}`,
  };
}

/** 构造系统代理来源的稳定 key。 */
export function systemProxySourceKey(type: SystemProxySource["type"], id: string): string {
  return `${type}:${id}`;
}
