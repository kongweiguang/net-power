/**
 * @author kongweiguang
 * Net Power 主工作台。界面直接面向代理管理工作流，不提供营销页。
 */

import { useEffect, useMemo, useRef, useState, type ComponentPropsWithoutRef, type FormEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import {
  Activity,
  CheckCircle2,
  ChevronDown,
  ClipboardList,
  Copy,
  Download,
  FileText,
  Globe2,
  Loader2,
  Network,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  RotateCcw,
  Settings,
  ShieldCheck,
  Square,
  TerminalSquare,
  Trash2,
  X,
  XCircle,
} from "lucide-react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  autostartApi,
  isTauriRuntime,
  logsApi,
  servicesApi,
  settingsApi,
  sshProfilesApi,
  systemProxyApi,
  updaterApi,
} from "../../api";
import type {
  AppSetting,
  AppUpdateProgress,
  AutostartStatus,
  ConnectionEventPayload,
  LogFilter,
  LogRow,
  RuntimeStatus,
  ServiceEventPayload,
  ServiceKind,
  ServiceRuntimeSummary,
  ServiceSummary,
  SshAuthType,
  SshProfile,
  SystemProxyProfile,
  SystemProxyProfileInput,
  SystemProxyStatus,
  SystemProxyTarget,
  TestResult,
} from "../../types";
import {
  defaultBodyRewriteRuleDraft,
  defaultHeaderRuleDraft,
  defaultServiceDraft,
  defaultSshDraft,
  filterServicesByPage,
  formatBytes,
  formatTime,
  kindLabels,
  pageForServiceKind,
  pageTitle,
  parseServiceDraft,
  parseSshDraft,
  readError,
  serviceDetailToDraft,
  serviceName,
  sshProfileToDraft,
  type PageKey,
  type ServiceDraft,
  type SshDraft,
} from "./workbenchModel";

interface Toast {
  id: number;
  kind: "success" | "error" | "info";
  title: string;
  detail?: string;
}

interface SystemProxyProfileDraft {
  name: string;
  proxyHost: string;
  proxyPort: string;
  bypass: string;
}

type SystemProxySource =
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

const defaultSystemProxyProfileDraft: SystemProxyProfileDraft = {
  name: "",
  proxyHost: "127.0.0.1",
  proxyPort: "7890",
  bypass: "localhost;127.*",
};

const primaryNavItems: Array<{ key: PageKey; label: string; icon: typeof Activity }> = [
  { key: "dashboard", label: "仪表盘", icon: Activity },
  { key: "http", label: "HTTP", icon: Globe2 },
  { key: "forwarding", label: "端口转发", icon: Network },
  { key: "ssh", label: "SSH", icon: TerminalSquare },
  { key: "system", label: "系统代理", icon: ShieldCheck },
  { key: "settings", label: "设置", icon: Settings },
];

type WindowControlAction = "minimize" | "toggle-maximize" | "close";

const titleBarControls: Array<{
  action: WindowControlAction;
  label: string;
  symbol: string;
  tone?: "danger";
}> = [
  { action: "minimize", label: "最小化窗口", symbol: "—" },
  { action: "toggle-maximize", label: "最大化或还原窗口", symbol: "□" },
  { action: "close", label: "关闭到托盘", symbol: "×", tone: "danger" },
];

const runtimeStatusLabels: Record<RuntimeStatus["type"], string> = {
  stopped: "已停止",
  starting: "启动中",
  running: "运行中",
  stopping: "停止中",
  failed: "失败",
};

const logLevelLabels: Record<LogRow["level"], string> = {
  trace: "跟踪",
  debug: "调试",
  info: "信息",
  warn: "警告",
  error: "错误",
};

const sshAuthLabels: Record<SshAuthType, string> = {
  password: "密码",
  private_key: "私钥",
  agent: "Agent",
};

const knownHostModeLabels: Record<SshDraft["knownHostsMode"], string> = {
  accept_new: "首次连接自动信任",
  strict: "严格校验",
  insecure_skip: "跳过校验",
};

interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

function cx(...classes: Array<string | false | null | undefined>): string {
  return classes.filter(Boolean).join(" ");
}

function serviceKindsForPage(page: PageKey): ServiceKind[] | null {
  if (page === "http") return ["http_reverse", "http_forward"];
  if (page === "forwarding") return ["tcp_forward", "udp_forward"];
  if (page === "ssh") return ["ssh_local", "ssh_remote", "ssh_socks"];
  return null;
}

function normalizeDraftForPage(page: PageKey, draft: ServiceDraft): ServiceDraft {
  const allowedKinds = serviceKindsForPage(page);
  if (!allowedKinds || allowedKinds.includes(draft.kind)) {
    return draft;
  }
  return { ...draft, kind: allowedKinds[0] };
}

function systemProxySourceKey(type: SystemProxySource["type"], id: string): string {
  return `${type}:${id}`;
}

async function runWindowControl(action: WindowControlAction) {
  if (!isTauriRuntime()) {
    return;
  }
  const appWindow = getCurrentWindow();
  try {
    if (action === "minimize") {
      await appWindow.minimize();
    } else if (action === "toggle-maximize") {
      await appWindow.toggleMaximize();
    } else {
      await appWindow.close();
    }
  } catch (err) {
    console.warn("窗口控制失败", err);
  }
}

function AppTitleBar() {
  return (
    <header
      className="system-titlebar"
      data-tauri-drag-region
      onDoubleClick={() => void runWindowControl("toggle-maximize")}
    >
      <div className="titlebar-drag-region" data-tauri-drag-region aria-hidden="true" />
      <div className="window-controls" aria-label="窗口控制">
        {titleBarControls.map((control) => {
          return (
            <button
              key={control.action}
              type="button"
              className={cx("window-control", `window-control-${control.action}`, control.tone === "danger" && "danger")}
              title={control.label}
              aria-label={control.label}
              onMouseDown={(event) => event.stopPropagation()}
              onDoubleClick={(event) => event.stopPropagation()}
              onClick={(event) => {
                event.stopPropagation();
                void runWindowControl(control.action);
              }}
            >
              <span className="window-control-symbol" aria-hidden="true">{control.symbol}</span>
            </button>
          );
        })}
      </div>
    </header>
  );
}

/** 主工作台组件。 */
export function Workbench() {
  const [page, setPage] = useState<PageKey>("dashboard");
  const [services, setServices] = useState<ServiceSummary[]>([]);
  const [runtime, setRuntime] = useState<ServiceRuntimeSummary[]>([]);
  const [profiles, setProfiles] = useState<SshProfile[]>([]);
  const [proxyProfiles, setProxyProfiles] = useState<SystemProxyProfile[]>([]);
  const [settings, setSettings] = useState<AppSetting[]>([]);
  const [autostartStatus, setAutostartStatus] = useState<AutostartStatus>({
    enabled: false,
    supported: true,
    message: "未读取",
  });
  const [proxyStatus, setProxyStatus] = useState<SystemProxyStatus>({
    enabled: false,
    proxyHost: "",
    proxyPort: null,
    bypass: "",
    message: "未读取",
  });
  const [manualProxyTarget, setManualProxyTarget] = useState<SystemProxyTarget>({
    proxyHost: "127.0.0.1",
    proxyPort: 7890,
    bypass: "localhost;127.*",
  });
  const [serviceDraft, setServiceDraft] = useState<ServiceDraft>(defaultServiceDraft);
  const [sshDraft, setSshDraft] = useState<SshDraft>(defaultSshDraft);
  const [proxyProfileDraft, setProxyProfileDraft] = useState<SystemProxyProfileDraft>(defaultSystemProxyProfileDraft);
  const [selectedSystemProxySourceKey, setSelectedSystemProxySourceKey] = useState("");
  const [editingServiceId, setEditingServiceId] = useState<string | null>(null);
  const [serviceDialogOpen, setServiceDialogOpen] = useState(false);
  const [editingSshProfileId, setEditingSshProfileId] = useState<string | null>(null);
  const [sshProfileDialogOpen, setSshProfileDialogOpen] = useState(false);
  const [editingProxyProfileId, setEditingProxyProfileId] = useState<string | null>(null);
  const [proxyProfileDialogOpen, setProxyProfileDialogOpen] = useState(false);
  const [logDialogService, setLogDialogService] = useState<ServiceSummary | null>(null);
  const [serviceLogs, setServiceLogs] = useState<LogRow[]>([]);
  const [serviceLogFilter, setServiceLogFilter] = useState<LogFilter>({ limit: 200 });
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [updateProgress, setUpdateProgress] = useState<AppUpdateProgress | null>(null);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const toastIdRef = useRef(0);

  const runningCount = services.filter((service) => service.runtimeStatus.type === "running").length;
  const stoppedCount = services.filter((service) => service.runtimeStatus.type === "stopped").length;
  const failedCount = services.filter((service) => service.runtimeStatus.type === "failed").length;
  const visibleServices = useMemo(() => filterServicesByPage(services, page), [services, page]);
  const httpForwardServices = services.filter((service) => service.kind === "http_forward");

  useEffect(() => {
    void reloadAll();
  }, []);

  useEffect(() => {
    if (editingServiceId) {
      return;
    }
    setServiceDraft((current) => normalizeDraftForPage(page, current));
  }, [editingServiceId, page]);

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }
    let disposed = false;
    let timer: number | null = null;
    const unlisteners: UnlistenFn[] = [];
    const scheduleRefresh = () => {
      if (timer !== null) {
        return;
      }
      timer = window.setTimeout(() => {
        timer = null;
        void syncRuntime();
      }, 250);
    };

    void Promise.all([
      listen<string>("service://status-changed", scheduleRefresh),
      listen<ServiceEventPayload>("service://log", scheduleRefresh),
      listen<ConnectionEventPayload>("service://connection", scheduleRefresh),
    ])
      .then((nextUnlisteners) => {
        if (disposed) {
          nextUnlisteners.forEach((unlisten) => unlisten());
        } else {
          unlisteners.push(...nextUnlisteners);
        }
      })
      .catch((err) => toast("error", "运行态监听失败", readError(err)));

    return () => {
      disposed = true;
      if (timer !== null) {
        window.clearTimeout(timer);
      }
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  async function reloadAll() {
    setLoading(true);
    setError(null);
    const [serviceResult, runtimeResult, profileResult, proxyProfileResult, settingResult, proxyResult, autostartResult] =
      await Promise.allSettled([
        servicesApi.list(),
        servicesApi.listRuntimeStatus(),
        sshProfilesApi.list(),
        systemProxyApi.listProfiles(),
        settingsApi.list(),
        systemProxyApi.getStatus(),
        autostartApi.getStatus(),
      ]);

    if (serviceResult.status === "fulfilled") {
      setServices(serviceResult.value);
    } else {
      setError(readError(serviceResult.reason));
    }
    if (runtimeResult.status === "fulfilled") {
      setRuntime(runtimeResult.value);
    }
    if (profileResult.status === "fulfilled") {
      setProfiles(profileResult.value);
    }
    if (proxyProfileResult.status === "fulfilled") {
      setProxyProfiles(proxyProfileResult.value);
    }
    if (settingResult.status === "fulfilled") {
      setSettings(settingResult.value);
    }
    if (proxyResult.status === "fulfilled") {
      setProxyStatus(proxyResult.value);
    }
    if (autostartResult.status === "fulfilled") {
      setAutostartStatus(autostartResult.value);
    }
    setLoading(false);
  }

  async function refreshRuntime() {
    setBusy("refresh");
    try {
      await syncRuntime();
      toast("success", "已刷新运行态");
    } catch (err) {
      toast("error", "刷新失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function syncRuntime() {
    const [nextServices, nextRuntime, nextProxy, nextProxyProfiles] = await Promise.all([
      servicesApi.list(),
      servicesApi.listRuntimeStatus(),
      systemProxyApi.getStatus(),
      systemProxyApi.listProfiles(),
    ]);
    setServices(nextServices);
    setRuntime(nextRuntime);
    setProxyStatus(nextProxy);
    setProxyProfiles(nextProxyProfiles);
  }

  async function saveService(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const parsed = parseServiceDraft(serviceDraft);
    if (typeof parsed === "string") {
      toast("error", "表单校验失败", parsed);
      return;
    }
    setBusy("save-service");
    try {
      if (editingServiceId) {
        await servicesApi.update(editingServiceId, parsed);
      } else {
        await servicesApi.create(parsed);
      }
      setServiceDraft({ ...defaultServiceDraft, kind: serviceDraft.kind });
      setEditingServiceId(null);
      setServiceDialogOpen(false);
      toast("success", editingServiceId ? "服务已更新" : "服务已创建", parsed.name);
      await reloadAll();
    } catch (err) {
      toast("error", editingServiceId ? "更新服务失败" : "创建服务失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function editService(id: string) {
    setBusy(`edit:${id}`);
    try {
      const detail = await servicesApi.get(id);
      setServiceDraft(serviceDetailToDraft(detail));
      setEditingServiceId(id);
      setPage(pageForServiceKind(detail.kind));
      setServiceDialogOpen(true);
      toast("info", "已载入服务编辑", detail.name);
    } catch (err) {
      toast("error", "读取服务详情失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  function cancelServiceEdit() {
    setEditingServiceId(null);
    setServiceDraft({ ...defaultServiceDraft, kind: serviceDraft.kind });
    setServiceDialogOpen(false);
  }

  function openServiceCreateDialog(targetPage: PageKey = page) {
    const allowedKinds = serviceKindsForPage(targetPage);
    const nextKind = allowedKinds?.[0] ?? defaultServiceDraft.kind;
    setEditingServiceId(null);
    setServiceDraft(normalizeDraftForPage(targetPage, { ...defaultServiceDraft, kind: nextKind }));
    setServiceDialogOpen(true);
  }

  async function serviceAction(id: string, action: "start" | "stop" | "restart" | "delete" | "duplicate" | "test") {
    setBusy(`${action}:${id}`);
    try {
      let result: TestResult | null = null;
      if (action === "start") {
        await servicesApi.start(id);
      } else if (action === "stop") {
        await servicesApi.stop(id);
      } else if (action === "restart") {
        await servicesApi.restart(id);
      } else if (action === "delete") {
        await servicesApi.delete(id);
      } else if (action === "duplicate") {
        await servicesApi.duplicate(id);
      } else {
        result = await servicesApi.test(id);
      }
      if (result) {
        toast(result.ok ? "success" : "error", result.ok ? "测试通过" : "测试失败", result.message);
      } else {
        toast("success", "服务操作完成");
        await reloadAll();
      }
    } catch (err) {
      toast("error", "服务操作失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function saveSshProfile(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const input = parseSshDraft(sshDraft);
    if (typeof input === "string") {
      toast("error", "SSH 表单校验失败", input);
      return;
    }
    setBusy("save-ssh");
    try {
      if (editingSshProfileId) {
        await sshProfilesApi.update(editingSshProfileId, input);
      } else {
        await sshProfilesApi.create(input);
      }
      setSshDraft(defaultSshDraft);
      setEditingSshProfileId(null);
      setSshProfileDialogOpen(false);
      toast("success", editingSshProfileId ? "SSH 配置已更新" : "SSH 配置已创建", input.name);
      setProfiles(await sshProfilesApi.list());
    } catch (err) {
      toast("error", editingSshProfileId ? "更新 SSH 配置失败" : "创建 SSH 配置失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function sshAction(id: string, action: "test" | "delete" | "edit") {
    if (action === "edit") {
      const profile = await sshProfilesApi.get(id);
      if (!profile) {
        toast("error", "SSH 配置不存在");
        return;
      }
      setSshDraft(sshProfileToDraft(profile));
      setEditingSshProfileId(id);
      setSshProfileDialogOpen(true);
      toast("info", "已载入 SSH 配置编辑", profile.name);
      return;
    }
    setBusy(`${action}-ssh:${id}`);
    try {
      if (action === "test") {
        const result = await sshProfilesApi.test(id);
        toast(result.ok ? "success" : "error", result.ok ? "SSH 测试通过" : "SSH 测试失败", result.message);
      } else {
        await sshProfilesApi.delete(id);
        toast("success", "SSH 配置已删除");
        setProfiles(await sshProfilesApi.list());
      }
    } catch (err) {
      toast("error", "SSH 操作失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  function openSshProfileCreateDialog() {
    setEditingSshProfileId(null);
    setSshDraft(defaultSshDraft);
    setSshProfileDialogOpen(true);
  }

  function closeSshProfileDialog() {
    setEditingSshProfileId(null);
    setSshDraft(defaultSshDraft);
    setSshProfileDialogOpen(false);
  }

  async function enableSystemProxySource(sourceKey: string) {
    const servicePrefix = "service:";
    const profilePrefix = "profile:";
    setSelectedSystemProxySourceKey(sourceKey);
    setBusy(`use-system-proxy-source:${sourceKey}`);
    try {
      if (sourceKey.startsWith(servicePrefix)) {
        const serviceId = sourceKey.slice(servicePrefix.length);
        const service = httpForwardServices.find((item) => item.id === serviceId);
        if (!service) {
          toast("error", "系统代理来源不存在");
          return;
        }
        if (service.runtimeStatus.type !== "running") {
          await servicesApi.start(service.id);
        }
        const status = await systemProxyApi.set(service.id);
        setProxyStatus(status);
        await syncRuntime();
        toast(
          "success",
          service.runtimeStatus.type === "running" ? status.message : "代理已启动并设为系统代理",
          service.name,
        );
        return;
      }

      if (sourceKey.startsWith(profilePrefix)) {
        const profileId = sourceKey.slice(profilePrefix.length);
        const profile = proxyProfiles.find((item) => item.id === profileId);
        if (!profile) {
          toast("error", "系统代理配置档不存在");
          return;
        }
        const status = await systemProxyApi.set(profile.id);
        setProxyStatus(status);
        setProxyProfiles(await systemProxyApi.listProfiles());
        toast("success", status.message, profile.name);
        return;
      }

      toast("error", "系统代理来源无效");
    } catch (err) {
      toast("error", "设置系统代理失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function saveSystemProxyProfile(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const input = parseSystemProxyProfileDraft(proxyProfileDraft);
    if (typeof input === "string") {
      toast("error", "系统代理配置档校验失败", input);
      return;
    }
    setBusy("save-system-proxy-profile");
    try {
      if (editingProxyProfileId) {
        await systemProxyApi.updateProfile(editingProxyProfileId, input);
      } else {
        await systemProxyApi.createProfile(input);
      }
      setProxyProfileDraft(defaultSystemProxyProfileDraft);
      setEditingProxyProfileId(null);
      setProxyProfileDialogOpen(false);
      toast("success", editingProxyProfileId ? "系统代理配置档已更新" : "系统代理配置档已创建", input.name);
      setProxyProfiles(await systemProxyApi.listProfiles());
    } catch (err) {
      toast("error", editingProxyProfileId ? "更新系统代理配置档失败" : "创建系统代理配置档失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function proxyProfileAction(id: string, action: "edit" | "delete") {
    const profile = proxyProfiles.find((item) => item.id === id);
    if (!profile) {
      toast("error", "系统代理配置档不存在");
      return;
    }
    if (action === "edit") {
      setProxyProfileDraft(systemProxyProfileToDraft(profile));
      setEditingProxyProfileId(id);
      setProxyProfileDialogOpen(true);
      toast("info", "已载入系统代理配置档编辑", profile.name);
      return;
    }
    setBusy(`${action}-proxy-profile:${id}`);
    try {
      await systemProxyApi.deleteProfile(id);
      setProxyProfiles(await systemProxyApi.listProfiles());
      if (editingProxyProfileId === id) {
        setEditingProxyProfileId(null);
        setProxyProfileDraft(defaultSystemProxyProfileDraft);
        setProxyProfileDialogOpen(false);
      }
      toast("success", "系统代理配置档已删除", profile.name);
    } catch (err) {
      toast("error", "系统代理配置档操作失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  function openSystemProxyProfileCreateDialog() {
    setEditingProxyProfileId(null);
    setProxyProfileDraft(defaultSystemProxyProfileDraft);
    setProxyProfileDialogOpen(true);
  }

  function closeSystemProxyProfileDialog() {
    setEditingProxyProfileId(null);
    setProxyProfileDraft(defaultSystemProxyProfileDraft);
    setProxyProfileDialogOpen(false);
  }

  async function setManualSystemProxy(target: SystemProxyTarget) {
    if (!target.proxyHost.trim() || !Number.isInteger(target.proxyPort) || target.proxyPort < 1 || target.proxyPort > 65535) {
      toast("error", "系统代理表单校验失败", "主机不能为空，端口必须是 1-65535。");
      return;
    }
    setBusy("system-proxy-manual");
    try {
      const status = await systemProxyApi.setTarget({
        proxyHost: target.proxyHost.trim(),
        proxyPort: target.proxyPort,
        bypass: target.bypass.trim(),
      });
      setProxyStatus(status);
      setProxyProfiles(await systemProxyApi.listProfiles());
      toast("success", status.message);
    } catch (err) {
      toast("error", "设置系统代理失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function clearSystemProxy() {
    setBusy("system-proxy-clear");
    try {
      const status = await systemProxyApi.clear();
      setProxyStatus(status);
      setProxyProfiles(await systemProxyApi.listProfiles());
      toast("success", status.message);
    } catch (err) {
      toast("error", "清理系统代理失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function updateSetting(key: string, valueJson: string) {
    setBusy(`setting:${key}`);
    try {
      await settingsApi.update(key, valueJson);
      setSettings(await settingsApi.list());
      toast("success", "设置已保存", key);
    } catch (err) {
      toast("error", "保存设置失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function updateAutostart(enabled: boolean) {
    setBusy("autostart");
    try {
      const next = await autostartApi.setEnabled(enabled);
      setAutostartStatus(next);
      toast("success", next.enabled ? "已开启开机启动" : "已关闭开机启动", next.message);
    } catch (err) {
      toast("error", enabled ? "开启开机启动失败" : "关闭开机启动失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function checkForUpdate() {
    setBusy("update");
    setUpdateProgress(null);
    try {
      const result = await updaterApi.checkDownloadInstall(setUpdateProgress);
      if (!result.updated) {
        toast("success", "已是最新版本");
        return;
      }
      if (result.relaunchTriggered) {
        toast("success", `已安装 ${result.version ?? "新版本"}`, "应用正在重启以完成更新。");
      } else {
        toast("info", `已安装 ${result.version ?? "新版本"}`, result.relaunchError ?? "请手动重启应用以完成更新。");
      }
    } catch (err) {
      toast("error", "检查更新失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function openServiceLogs(service: ServiceSummary) {
    const nextFilter: LogFilter = { serviceId: service.id, limit: 200 };
    setLogDialogService(service);
    setServiceLogFilter(nextFilter);
    setServiceLogs([]);
    setBusy(`logs:${service.id}`);
    try {
      setServiceLogs(await logsApi.list(nextFilter));
    } catch (err) {
      toast("error", "读取配置日志失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function applyServiceLogFilter(filter: LogFilter) {
    if (!logDialogService) {
      return;
    }
    const nextFilter: LogFilter = { ...filter, serviceId: logDialogService.id, limit: 200 };
    setServiceLogFilter(nextFilter);
    setBusy("filter-service-logs");
    try {
      setServiceLogs(await logsApi.list(nextFilter));
    } catch (err) {
      toast("error", "日志筛选失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function clearServiceLogs() {
    if (!logDialogService) {
      return;
    }
    setBusy("clear-service-logs");
    try {
      await logsApi.clear(logDialogService.id);
      setServiceLogs(await logsApi.list({ ...serviceLogFilter, serviceId: logDialogService.id, limit: 200 }));
      toast("success", "配置日志已清理", logDialogService.name);
    } catch (err) {
      toast("error", "清理配置日志失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  function toast(kind: Toast["kind"], title: string, detail?: string) {
    const id = ++toastIdRef.current;
    setToasts((current) => [...current.slice(-3), { id, kind, title, detail }]);
    window.setTimeout(() => {
      setToasts((current) => current.filter((item) => item.id !== id));
    }, 3600);
  }

  return (
    <main className="app-frame">
      <aside className="sidebar">
        <div className="brand">
          <Network size={24} />
          <div>
            <strong>Net Power</strong>
            <span>代理控制台</span>
          </div>
        </div>
        <nav aria-label="主导航">
          {primaryNavItems.map((item) => {
            const Icon = item.icon;
            return (
              <div className="nav-group" key={item.key}>
                <button
                  type="button"
                  className={page === item.key ? "nav-item active" : "nav-item"}
                  onClick={() => setPage(item.key)}
                >
                  <Icon size={17} />
                  {item.label}
                </button>
              </div>
            );
          })}
        </nav>
      </aside>

      <div className="app-shell">
        <AppTitleBar />
        <section className="workspace">
        <header className="topbar">
          <div>
            <p>{new Date().toLocaleDateString()}</p>
            <h1>{pageTitle(page)}</h1>
          </div>
          <button type="button" className="ghost-button" onClick={refreshRuntime} disabled={busy === "refresh"}>
            {busy === "refresh" ? <Loader2 size={16} className="spin" /> : <RefreshCw size={16} />}
            刷新
          </button>
        </header>

        {error && <div className="error-banner">{error}</div>}

        {page === "dashboard" && (
          <Dashboard
            loading={loading}
            services={services}
            runningCount={runningCount}
            stoppedCount={stoppedCount}
            failedCount={failedCount}
            proxyStatus={proxyStatus}
            busy={busy}
            onAction={serviceAction}
            onEdit={editService}
            onLogs={openServiceLogs}
          />
        )}

        {(page === "http" || page === "forwarding") && (
          <ServicePage
            page={page}
            services={visibleServices}
            draft={serviceDraft}
            busy={busy}
            profiles={profiles}
            editingServiceId={editingServiceId}
            serviceDialogOpen={serviceDialogOpen}
            onDraftChange={setServiceDraft}
            onSubmit={saveService}
            onOpenCreate={() => openServiceCreateDialog(page)}
            onCancelEdit={cancelServiceEdit}
            onAction={serviceAction}
            onEdit={editService}
            onLogs={openServiceLogs}
          />
        )}

        {page === "ssh" && (
          <SshPage
            services={visibleServices}
            profiles={profiles}
            serviceDraft={serviceDraft}
            sshDraft={sshDraft}
            busy={busy}
            sshProfileDialogOpen={sshProfileDialogOpen}
            serviceDialogOpen={serviceDialogOpen}
            onServiceDraftChange={setServiceDraft}
            onSshDraftChange={setSshDraft}
            editingServiceId={editingServiceId}
            editingSshProfileId={editingSshProfileId}
            onOpenCreateService={() => openServiceCreateDialog("ssh")}
            onOpenCreateProfile={openSshProfileCreateDialog}
            onCreateService={saveService}
            onCreateProfile={saveSshProfile}
            onCancelServiceEdit={cancelServiceEdit}
            onCancelProfileEdit={closeSshProfileDialog}
            onServiceAction={serviceAction}
            onServiceEdit={editService}
            onServiceLogs={openServiceLogs}
            onSshAction={sshAction}
          />
        )}

        {page === "system" && (
          <SystemProxyPage
            status={proxyStatus}
            candidates={httpForwardServices}
            profiles={proxyProfiles}
            manualTarget={manualProxyTarget}
            profileDraft={proxyProfileDraft}
            selectedSourceKey={selectedSystemProxySourceKey}
            editingProfileId={editingProxyProfileId}
            profileDialogOpen={proxyProfileDialogOpen}
            busy={busy}
            onManualTargetChange={setManualProxyTarget}
            onProfileDraftChange={setProxyProfileDraft}
            onSelectedSourceChange={setSelectedSystemProxySourceKey}
            onUseSource={enableSystemProxySource}
            onSaveProfile={saveSystemProxyProfile}
            onOpenCreateProfile={openSystemProxyProfileCreateDialog}
            onCancelProfileEdit={closeSystemProxyProfileDialog}
            onProfileAction={proxyProfileAction}
            onSetManual={setManualSystemProxy}
            onClear={clearSystemProxy}
          />
        )}

        {page === "settings" && (
          <SettingsPage
            settings={settings}
            autostart={autostartStatus}
            busy={busy}
            updateProgress={updateProgress}
            onUpdate={updateSetting}
            onAutostartChange={updateAutostart}
            onCheckUpdate={checkForUpdate}
            runtime={runtime}
          />
        )}
        <ServiceLogsDialog
          service={logDialogService}
          logs={serviceLogs}
          services={services}
          busy={busy}
          filter={serviceLogFilter}
          onFilter={applyServiceLogFilter}
          onClear={clearServiceLogs}
          onClose={() => setLogDialogService(null)}
        />
        </section>
      </div>
      <ToastStack toasts={toasts} />
    </main>
  );
}

interface DashboardProps {
  loading: boolean;
  services: ServiceSummary[];
  runningCount: number;
  stoppedCount: number;
  failedCount: number;
  proxyStatus: SystemProxyStatus;
  busy: string | null;
  onAction: (id: string, action: "start" | "stop" | "restart" | "delete" | "duplicate" | "test") => void;
  onEdit: (id: string) => void;
  onLogs: (service: ServiceSummary) => void;
}

function Dashboard({ loading, services, runningCount, stoppedCount, failedCount, proxyStatus, busy, onAction, onEdit, onLogs }: DashboardProps) {
  return (
    <div className="view-stack">
      <section className="metric-grid">
        <MetricCard label="运行中" value={runningCount} tone="good" />
        <MetricCard label="已停止" value={stoppedCount} tone="muted" />
        <MetricCard label="失败" value={failedCount} tone={failedCount > 0 ? "bad" : "muted"} />
        <MetricCard label="系统代理" value={proxyStatus.enabled ? "已开启" : "未开启"} tone={proxyStatus.enabled ? "good" : "muted"} />
      </section>
      <section className="panel">
        <PanelTitle title="服务总览" subtitle="启动、停止、测试和复制所有代理服务。" />
        {loading ? <LoadingRows /> : <ServiceTable services={services} busy={busy} onAction={onAction} onEdit={onEdit} onLogs={onLogs} />}
      </section>
    </div>
  );
}

interface ServicePageProps {
  page: PageKey;
  services: ServiceSummary[];
  draft: ServiceDraft;
  busy: string | null;
  profiles: SshProfile[];
  editingServiceId: string | null;
  serviceDialogOpen: boolean;
  onDraftChange: (draft: ServiceDraft) => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onOpenCreate: () => void;
  onCancelEdit: () => void;
  onAction: (id: string, action: "start" | "stop" | "restart" | "delete" | "duplicate" | "test") => void;
  onEdit: (id: string) => void;
  onLogs: (service: ServiceSummary) => void;
}

function ServicePage({ page, services, draft, busy, profiles, editingServiceId, serviceDialogOpen, onDraftChange, onSubmit, onOpenCreate, onCancelEdit, onAction, onEdit, onLogs }: ServicePageProps) {
  const allowedKinds = serviceKindsForPage(page) ?? ["http_reverse"];
  const currentDraft = normalizeDraftForPage(page, draft);
  return (
    <div className="view-stack">
      <section className="panel service-list-panel">
        <div className="panel-heading">
          <PanelTitle title={`${pageTitle(page)} 配置列表`} subtitle="这里仅展示已保存配置，启动、修改、删除和日志查看都在列表行完成。" />
          <button type="button" className="primary-button" onClick={onOpenCreate}>
            <Plus size={16} />
            添加配置
          </button>
        </div>
        <ServiceTable services={services} busy={busy} onAction={onAction} onEdit={onEdit} onLogs={onLogs} />
      </section>
      <DialogShell
        open={serviceDialogOpen || Boolean(editingServiceId)}
        title={editingServiceId ? "编辑服务配置" : `添加 ${pageTitle(page)} 配置`}
        description="配置会写入 SQLite，启动后由 Rust 后端托管生命周期。"
        onClose={onCancelEdit}
        size="wide"
      >
        <ServiceForm draft={currentDraft} allowedKinds={allowedKinds} profiles={profiles} busy={busy} editing={Boolean(editingServiceId)} onDraftChange={onDraftChange} onSubmit={onSubmit} onCancelEdit={onCancelEdit} />
      </DialogShell>
    </div>
  );
}

interface SshPageProps {
  services: ServiceSummary[];
  profiles: SshProfile[];
  serviceDraft: ServiceDraft;
  sshDraft: SshDraft;
  busy: string | null;
  sshProfileDialogOpen: boolean;
  serviceDialogOpen: boolean;
  editingServiceId: string | null;
  editingSshProfileId: string | null;
  onServiceDraftChange: (draft: ServiceDraft) => void;
  onSshDraftChange: (draft: SshDraft) => void;
  onOpenCreateService: () => void;
  onOpenCreateProfile: () => void;
  onCreateService: (event: FormEvent<HTMLFormElement>) => void;
  onCreateProfile: (event: FormEvent<HTMLFormElement>) => void;
  onCancelServiceEdit: () => void;
  onCancelProfileEdit: () => void;
  onServiceAction: (id: string, action: "start" | "stop" | "restart" | "delete" | "duplicate" | "test") => void;
  onServiceEdit: (id: string) => void;
  onServiceLogs: (service: ServiceSummary) => void;
  onSshAction: (id: string, action: "test" | "delete" | "edit") => void;
}

function SshPage({ services, profiles, serviceDraft, sshDraft, busy, sshProfileDialogOpen, serviceDialogOpen, editingServiceId, editingSshProfileId, onServiceDraftChange, onSshDraftChange, onOpenCreateService, onOpenCreateProfile, onCreateService, onCreateProfile, onCancelServiceEdit, onCancelProfileEdit, onServiceAction, onServiceEdit, onServiceLogs, onSshAction }: SshPageProps) {
  const sshKinds: ServiceKind[] = ["ssh_local", "ssh_remote", "ssh_socks"];
  const tunnelDraft = sshKinds.includes(serviceDraft.kind) ? serviceDraft : { ...serviceDraft, kind: "ssh_local" as ServiceKind };
  return (
    <div className="ssh-workbench">
      <section className="panel ssh-profile-panel">
        <div className="panel-heading">
          <PanelTitle title="SSH 配置" subtitle="密码与私钥口令只传给 Rust 加密保存，前端不回显。" />
          <button type="button" className="primary-button" onClick={onOpenCreateProfile}>
            <Plus size={16} />
            添加 SSH 配置
          </button>
        </div>
        <SshProfileList profiles={profiles} busy={busy} onAction={onSshAction} />
      </section>
      <DialogShell
        open={sshProfileDialogOpen || Boolean(editingSshProfileId)}
        title={editingSshProfileId ? "编辑 SSH 配置" : "添加 SSH 配置"}
        description="敏感字段只在保存时传给 Rust 后端，编辑时不会回显已有密码或私钥口令。"
        onClose={onCancelProfileEdit}
      >
        <SshProfileForm profiles={profiles} editingProfileId={editingSshProfileId} draft={sshDraft} busy={busy} editing={Boolean(editingSshProfileId)} onDraftChange={onSshDraftChange} onSubmit={onCreateProfile} onCancelEdit={onCancelProfileEdit} />
      </DialogShell>
      <section className="panel ssh-tunnel-panel">
        <div className="panel-heading">
          <PanelTitle title="SSH 隧道配置列表" subtitle="支持本地端口转发、远程端口转发和 SOCKS5 动态代理。" />
          <button type="button" className="primary-button" onClick={onOpenCreateService}>
            <Plus size={16} />
            添加 SSH 隧道
          </button>
        </div>
        <ServiceTable services={services} busy={busy} onAction={onServiceAction} onEdit={onServiceEdit} onLogs={onServiceLogs} />
      </section>
      <DialogShell
        open={serviceDialogOpen || Boolean(editingServiceId)}
        title={editingServiceId ? "编辑 SSH 隧道配置" : "添加 SSH 隧道配置"}
        description="选择 SSH 配置后再设置本地、远程或 SOCKS5 隧道参数。"
        onClose={onCancelServiceEdit}
        size="wide"
      >
        <ServiceForm draft={tunnelDraft} allowedKinds={sshKinds} profiles={profiles} busy={busy} editing={Boolean(editingServiceId)} onDraftChange={onServiceDraftChange} onSubmit={onCreateService} onCancelEdit={onCancelServiceEdit} />
      </DialogShell>
    </div>
  );
}

interface ServiceFormProps {
  draft: ServiceDraft;
  allowedKinds: ServiceKind[];
  profiles: SshProfile[];
  busy: string | null;
  editing: boolean;
  onDraftChange: (draft: ServiceDraft) => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onCancelEdit: () => void;
}

function ServiceForm({ draft, allowedKinds, profiles, busy, editing, onDraftChange, onSubmit, onCancelEdit }: ServiceFormProps) {
  const isSshKind = ["ssh_local", "ssh_remote", "ssh_socks"].includes(draft.kind);
  const isSshRemote = draft.kind === "ssh_remote";
  const showFixedTarget = ["tcp_forward", "udp_forward", "ssh_local", "ssh_remote"].includes(draft.kind);
  const listenHostLabel = isSshRemote ? "远程绑定主机" : "监听主机";
  const listenPortLabel = isSshRemote ? "远程绑定端口" : "监听端口";
  const targetHostLabel = isSshRemote ? "本地目标主机" : "目标主机";
  const targetPortLabel = isSshRemote ? "本地目标端口" : "目标端口";
  return (
    <form className="form-grid" onSubmit={onSubmit}>
      <FormInput label="服务名称" value={draft.name} onChange={(event) => onDraftChange({ ...draft, name: event.currentTarget.value })} placeholder="本地反向代理" />
      <FormSelect
        label="服务类型"
        value={draft.kind}
        onChange={(event) => onDraftChange({ ...draft, kind: event.currentTarget.value as ServiceKind })}
        options={allowedKinds.map((kind) => ({ value: kind, label: kindLabels[kind] }))}
      />
      <FormInput label={listenHostLabel} value={draft.listenHost} onChange={(event) => onDraftChange({ ...draft, listenHost: event.currentTarget.value })} />
      <FormInput label={listenPortLabel} inputMode="numeric" value={draft.listenPort} onChange={(event) => onDraftChange({ ...draft, listenPort: event.currentTarget.value })} />

      {draft.kind === "http_reverse" && (
        <>
          <FormInput fieldClassName="span-2" label="目标 URL" value={draft.targetUrl} onChange={(event) => onDraftChange({ ...draft, targetUrl: event.currentTarget.value })} />
          <FormCheckbox label="保留 Host" checked={draft.preserveHost} onChange={(event) => onDraftChange({ ...draft, preserveHost: event.currentTarget.checked })} />
          <FormInput label="请求超时 ms" inputMode="numeric" value={draft.requestTimeoutMs} onChange={(event) => onDraftChange({ ...draft, requestTimeoutMs: event.currentTarget.value })} />
          <FormInput label="最大改写字节" inputMode="numeric" value={draft.maxRewriteBodyBytes} onChange={(event) => onDraftChange({ ...draft, maxRewriteBodyBytes: event.currentTarget.value })} />
          <FormCheckbox fieldClassName="span-2" label="跳过压缩 Body" checked={draft.skipCompressedBody} onChange={(event) => onDraftChange({ ...draft, skipCompressedBody: event.currentTarget.checked })} />
        </>
      )}

      {draft.kind === "http_forward" && (
        <>
          <FormCheckbox label="允许 HTTP" checked={draft.allowHttp} onChange={(event) => onDraftChange({ ...draft, allowHttp: event.currentTarget.checked })} />
          <FormCheckbox label="允许 CONNECT" checked={draft.allowConnect} onChange={(event) => onDraftChange({ ...draft, allowConnect: event.currentTarget.checked })} />
          <FormInput label="连接超时 ms" inputMode="numeric" value={draft.connectTimeoutMs} onChange={(event) => onDraftChange({ ...draft, connectTimeoutMs: event.currentTarget.value })} />
          <FormInput label="空闲超时 ms" inputMode="numeric" value={draft.idleTimeoutMs} onChange={(event) => onDraftChange({ ...draft, idleTimeoutMs: event.currentTarget.value })} />
        </>
      )}

      {["tcp_forward", "udp_forward", "ssh_local", "ssh_remote", "ssh_socks"].includes(draft.kind) && (
        <>
          {isSshKind && (
            <FormSelect
              fieldClassName="span-2"
              label="SSH 配置"
              value={draft.sshProfileId}
              onChange={(event) => onDraftChange({ ...draft, sshProfileId: event.currentTarget.value })}
              options={[
                { value: "", label: "选择 SSH 配置" },
                ...profiles.map((profile) => ({ value: profile.id, label: profile.name })),
              ]}
            />
          )}
          {showFixedTarget && (
            <>
              <FormInput label={targetHostLabel} value={draft.targetHost} onChange={(event) => onDraftChange({ ...draft, targetHost: event.currentTarget.value })} />
              <FormInput label={targetPortLabel} inputMode="numeric" value={draft.targetPort} onChange={(event) => onDraftChange({ ...draft, targetPort: event.currentTarget.value })} />
            </>
          )}
          {draft.kind === "tcp_forward" && (
            <>
              <FormInput label="连接超时 ms" inputMode="numeric" value={draft.connectTimeoutMs} onChange={(event) => onDraftChange({ ...draft, connectTimeoutMs: event.currentTarget.value })} />
              <FormInput label="空闲超时 ms" inputMode="numeric" value={draft.idleTimeoutMs} onChange={(event) => onDraftChange({ ...draft, idleTimeoutMs: event.currentTarget.value })} />
            </>
          )}
          {draft.kind === "udp_forward" && (
            <FormInput fieldClassName="span-2" label="空闲超时 ms" inputMode="numeric" value={draft.idleTimeoutMs} onChange={(event) => onDraftChange({ ...draft, idleTimeoutMs: event.currentTarget.value })} />
          )}
        </>
      )}

      {draft.kind === "http_reverse" && (
        <>
          <HeaderRulesEditor
            rules={draft.headerRules}
            onChange={(headerRules) => onDraftChange({ ...draft, headerRules })}
          />
          <BodyRewriteRulesEditor
            rules={draft.bodyRewriteRules}
            onChange={(bodyRewriteRules) => onDraftChange({ ...draft, bodyRewriteRules })}
          />
        </>
      )}

      <FormCheckbox label="自动启动" checked={draft.autoStart} onChange={(event) => onDraftChange({ ...draft, autoStart: event.currentTarget.checked })} />
      <FormInput fieldClassName="span-2" label="备注" value={draft.notes} onChange={(event) => onDraftChange({ ...draft, notes: event.currentTarget.value })} />
      <div className="button-row span-2">
        <button type="submit" className="primary-button" disabled={busy === "save-service"}>
          {busy === "save-service" ? <Loader2 className="spin" size={16} /> : editing ? <Pencil size={16} /> : <Plus size={16} />}
          {editing ? "保存服务" : "创建服务"}
        </button>
        <button type="button" className="ghost-button" onClick={onCancelEdit}>取消</button>
      </div>
    </form>
  );
}

interface HeaderRulesEditorProps {
  rules: ServiceDraft["headerRules"];
  onChange: (rules: ServiceDraft["headerRules"]) => void;
}

function HeaderRulesEditor({ rules, onChange }: HeaderRulesEditorProps) {
  const updateRule = (index: number, patch: Partial<ServiceDraft["headerRules"][number]>) => {
    onChange(rules.map((rule, current) => (current === index ? { ...rule, ...patch } : rule)));
  };
  return (
    <section className="rule-editor span-2">
      <div className="rule-editor-heading">
        <div>
          <strong>Header 规则</strong>
          <small>按顺序设置或删除 request/response header。</small>
        </div>
        <button
          type="button"
          className="ghost-button"
          onClick={() => onChange([...rules, { ...defaultHeaderRuleDraft, sortOrder: rules.length }])}
        >
          <Plus size={15} />
          添加 Header
        </button>
      </div>
      {rules.length === 0 ? (
        <p className="rule-empty">暂无 Header 改写规则。</p>
      ) : (
        rules.map((rule, index) => (
          <div className="rule-row header-rule-row" key={index}>
            <FormCheckbox
              fieldClassName="compact-check"
              label="启用"
              checked={rule.enabled}
              onChange={(event) => updateRule(index, { enabled: event.currentTarget.checked })}
            />
            <FormSelect
              label="阶段"
              value={rule.phase}
              onChange={(event) =>
                updateRule(index, { phase: event.currentTarget.value as ServiceDraft["headerRules"][number]["phase"] })
              }
              options={[
                { value: "request", label: "Request 请求" },
                { value: "response", label: "Response 响应" },
              ]}
            />
            <FormSelect
              label="动作"
              value={rule.action}
              onChange={(event) => {
                const action = event.currentTarget.value as ServiceDraft["headerRules"][number]["action"];
                updateRule(index, { action, value: action === "remove" ? null : (rule.value ?? "") });
              }}
              options={[
                { value: "set", label: "设置" },
                { value: "remove", label: "删除" },
              ]}
            />
            <FormInput label="名称" value={rule.name} onChange={(event) => updateRule(index, { name: event.currentTarget.value })} />
            <FormInput
              label="值"
              value={rule.value ?? ""}
              disabled={rule.action === "remove"}
              onChange={(event) => updateRule(index, { value: event.currentTarget.value })}
            />
            <FormInput
              label="排序"
              type="number"
              value={rule.sortOrder}
              onChange={(event) => updateRule(index, { sortOrder: Number(event.currentTarget.value) })}
            />
            <button
              type="button"
              className="icon-button danger"
              title="删除 Header 规则"
              onClick={() => onChange(rules.filter((_, current) => current !== index))}
            >
              <Trash2 size={15} />
            </button>
          </div>
        ))
      )}
    </section>
  );
}

interface BodyRewriteRulesEditorProps {
  rules: ServiceDraft["bodyRewriteRules"];
  onChange: (rules: ServiceDraft["bodyRewriteRules"]) => void;
}

function BodyRewriteRulesEditor({ rules, onChange }: BodyRewriteRulesEditorProps) {
  const updateRule = (index: number, patch: Partial<ServiceDraft["bodyRewriteRules"][number]>) => {
    onChange(rules.map((rule, current) => (current === index ? { ...rule, ...patch } : rule)));
  };
  return (
    <section className="rule-editor span-2">
      <div className="rule-editor-heading">
        <div>
          <strong>Body 改写规则</strong>
          <small>支持 JSON 点号路径、数组下标和 form 字段。</small>
        </div>
        <button
          type="button"
          className="ghost-button"
          onClick={() => onChange([...rules, { ...defaultBodyRewriteRuleDraft, sortOrder: rules.length }])}
        >
          <Plus size={15} />
          添加 Body 规则
        </button>
      </div>
      {rules.length === 0 ? (
        <p className="rule-empty">暂无 Body 改写规则。</p>
      ) : (
        rules.map((rule, index) => {
            const valueType = inferBodyValueType(rule.valueJson);
            return (
              <div className="rule-row body-rule-row" key={index}>
                <FormCheckbox
                  fieldClassName="compact-check"
                  label="启用"
                  checked={rule.enabled}
                  onChange={(event) => updateRule(index, { enabled: event.currentTarget.checked })}
                />
                <FormSelect
                  label="Body 类型"
                  value={rule.bodyType}
                  onChange={(event) =>
                    updateRule(index, { bodyType: event.currentTarget.value as ServiceDraft["bodyRewriteRules"][number]["bodyType"] })
                  }
                  options={[
                    { value: "auto", label: "自动" },
                    { value: "json", label: "JSON" },
                    { value: "form", label: "Form 表单" },
                  ]}
                />
                <FormInput label="路径" value={rule.path} onChange={(event) => updateRule(index, { path: event.currentTarget.value })} />
                <FormSelect
                  label="值类型"
                  value={valueType}
                  onChange={(event) =>
                    updateRule(index, {
                      valueJson: encodeBodyValue(
                        event.currentTarget.value as BodyValueType,
                        bodyValueText(rule.valueJson, valueType),
                      ),
                    })
                  }
                  options={[
                    { value: "string", label: "字符串" },
                    { value: "number", label: "数字" },
                    { value: "boolean", label: "布尔值" },
                    { value: "null", label: "空值" },
                    { value: "json", label: "JSON" },
                  ]}
                />
                <FormInput
                  label="值"
                  value={bodyValueText(rule.valueJson, valueType)}
                  disabled={valueType === "null"}
                  onChange={(event) => updateRule(index, { valueJson: encodeBodyValue(valueType, event.currentTarget.value) })}
                />
                <FormInput
                  label="排序"
                  type="number"
                  value={rule.sortOrder}
                  onChange={(event) => updateRule(index, { sortOrder: Number(event.currentTarget.value) })}
                />
                <button
                  type="button"
                className="icon-button danger"
                title="删除 Body 规则"
                onClick={() => onChange(rules.filter((_, current) => current !== index))}
              >
                <Trash2 size={15} />
              </button>
            </div>
          );
        })
      )}
    </section>
  );
}

type BodyValueType = "string" | "number" | "boolean" | "null" | "json";

function inferBodyValueType(valueJson: string): BodyValueType {
  try {
    const parsed = JSON.parse(valueJson);
    if (parsed === null) return "null";
    if (typeof parsed === "string") return "string";
    if (typeof parsed === "number") return "number";
    if (typeof parsed === "boolean") return "boolean";
    return "json";
  } catch {
    return "json";
  }
}

function bodyValueText(valueJson: string, valueType: BodyValueType): string {
  try {
    const parsed = JSON.parse(valueJson);
    if (valueType === "string" || valueType === "number" || valueType === "boolean") {
      return String(parsed);
    }
    if (valueType === "null") return "";
    return JSON.stringify(parsed);
  } catch {
    return valueJson;
  }
}

function encodeBodyValue(valueType: BodyValueType, raw: string): string {
  if (valueType === "string") return JSON.stringify(raw);
  if (valueType === "number") return raw.trim() || "0";
  if (valueType === "boolean") return raw === "true" ? "true" : "false";
  if (valueType === "null") return "null";
  return raw;
}

interface ServiceTableProps {
  services: ServiceSummary[];
  busy: string | null;
  onAction: (id: string, action: "start" | "stop" | "restart" | "delete" | "duplicate" | "test") => void;
  onEdit?: (id: string) => void;
  onLogs: (service: ServiceSummary) => void;
}

function ServiceTable({ services, busy, onAction, onEdit, onLogs }: ServiceTableProps) {
  if (services.length === 0) {
    return <EmptyState title="暂无服务" detail="点击添加配置后，就可以开始管理代理流量。" />;
  }
  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr>
            <th>名称</th>
            <th>类型</th>
            <th>监听</th>
            <th>目标</th>
            <th>状态</th>
            <th>连接</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {services.map((service) => (
            <tr key={service.id}>
              <td data-label="名称">
                <strong>{service.name}</strong>
                <small>{service.id.slice(0, 8)}</small>
              </td>
              <td data-label="类型">{kindLabels[service.kind]}</td>
              <td data-label="监听">{service.listenHost}:{service.listenPort}</td>
              <td data-label="目标">{service.targetLabel || "-"}</td>
              <td data-label="状态"><StatusPill status={service.runtimeStatus} /></td>
              <td data-label="连接">{service.activeConnections}/{service.totalConnections}</td>
              <td data-label="操作">
                <div className="icon-row">
                  <IconButton title="启动" busy={busy === `start:${service.id}`} onClick={() => onAction(service.id, "start")}><Play size={15} /></IconButton>
                  <IconButton title="停止" busy={busy === `stop:${service.id}`} onClick={() => onAction(service.id, "stop")}><Square size={15} /></IconButton>
                  <IconButton title="重启" busy={busy === `restart:${service.id}`} onClick={() => onAction(service.id, "restart")}><RotateCcw size={15} /></IconButton>
                  <IconButton title="测试" busy={busy === `test:${service.id}`} onClick={() => onAction(service.id, "test")}><CheckCircle2 size={15} /></IconButton>
                  {onEdit && <IconButton title="编辑" busy={busy === `edit:${service.id}`} onClick={() => onEdit(service.id)}><Pencil size={15} /></IconButton>}
                  <IconButton title="日志" busy={busy === `logs:${service.id}`} onClick={() => onLogs(service)}><ClipboardList size={15} /></IconButton>
                  <IconButton title="复制" busy={busy === `duplicate:${service.id}`} onClick={() => onAction(service.id, "duplicate")}><Copy size={15} /></IconButton>
                  <IconButton title="删除" danger busy={busy === `delete:${service.id}`} onClick={() => onAction(service.id, "delete")}><Trash2 size={15} /></IconButton>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

interface SshProfileFormProps {
  profiles: SshProfile[];
  editingProfileId: string | null;
  draft: SshDraft;
  busy: string | null;
  editing: boolean;
  onDraftChange: (draft: SshDraft) => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onCancelEdit: () => void;
}

function SshProfileForm({ profiles, editingProfileId, draft, busy, editing, onDraftChange, onSubmit, onCancelEdit }: SshProfileFormProps) {
  const jumpOptions = profiles.filter((profile) => profile.id !== editingProfileId);
  return (
    <form className="form-grid" onSubmit={onSubmit}>
      <FormInput label="名称" value={draft.name} onChange={(event) => onDraftChange({ ...draft, name: event.currentTarget.value })} />
      <FormInput label="主机" value={draft.host} onChange={(event) => onDraftChange({ ...draft, host: event.currentTarget.value })} />
      <FormInput label="端口" inputMode="numeric" value={draft.port} onChange={(event) => onDraftChange({ ...draft, port: event.currentTarget.value })} />
      <FormInput label="用户名" value={draft.username} onChange={(event) => onDraftChange({ ...draft, username: event.currentTarget.value })} />
      <FormSelect
        label="认证方式"
        value={draft.authType}
        onChange={(event) => onDraftChange({ ...draft, authType: event.currentTarget.value as SshAuthType })}
        options={[
          { value: "password", label: sshAuthLabels.password },
          { value: "private_key", label: sshAuthLabels.private_key },
          { value: "agent", label: sshAuthLabels.agent },
        ]}
      />
      {draft.authType === "password" && <FormInput label="密码" type="password" value={draft.password} onChange={(event) => onDraftChange({ ...draft, password: event.currentTarget.value })} />}
      {draft.authType === "private_key" && (
        <>
          <FormInput fieldClassName="span-2" label="私钥路径" value={draft.privateKeyPath} onChange={(event) => onDraftChange({ ...draft, privateKeyPath: event.currentTarget.value })} />
          <FormInput fieldClassName="span-2" label="私钥口令" type="password" value={draft.privateKeyPassphrase} onChange={(event) => onDraftChange({ ...draft, privateKeyPassphrase: event.currentTarget.value })} />
        </>
      )}
      <FormSelect
        label="known_hosts 校验"
        value={draft.knownHostsMode}
        onChange={(event) => onDraftChange({ ...draft, knownHostsMode: event.currentTarget.value as SshDraft["knownHostsMode"] })}
        options={[
          { value: "accept_new", label: knownHostModeLabels.accept_new },
          { value: "strict", label: knownHostModeLabels.strict },
          { value: "insecure_skip", label: knownHostModeLabels.insecure_skip },
        ]}
      />
      <FormInput fieldClassName="span-2" label="known_hosts 路径" value={draft.knownHostsPath} onChange={(event) => onDraftChange({ ...draft, knownHostsPath: event.currentTarget.value })} />
      <FormSelect
        label="跳板配置"
        value={draft.jumpProfileId}
        onChange={(event) => onDraftChange({ ...draft, jumpProfileId: event.currentTarget.value })}
        options={[
          { value: "", label: "直连目标主机" },
          ...jumpOptions.map((profile) => ({
            value: profile.id,
            label: `${profile.name} (${profile.username}@${profile.host}:${profile.port})`,
          })),
        ]}
      />
      <FormInput label="连接超时 ms" inputMode="numeric" value={draft.connectTimeoutMs} onChange={(event) => onDraftChange({ ...draft, connectTimeoutMs: event.currentTarget.value })} />
      <FormInput label="keepalive 间隔 ms" inputMode="numeric" value={draft.keepaliveIntervalMs} onChange={(event) => onDraftChange({ ...draft, keepaliveIntervalMs: event.currentTarget.value })} />
      <div className="button-row span-2">
        <button type="submit" className="primary-button" disabled={busy === "save-ssh"}>
          {busy === "save-ssh" ? <Loader2 className="spin" size={16} /> : editing ? <Pencil size={16} /> : <Plus size={16} />}
          {editing ? "保存 SSH 配置" : "添加 SSH 配置"}
        </button>
        <button type="button" className="ghost-button" onClick={onCancelEdit}>取消</button>
      </div>
    </form>
  );
}

interface SshProfileListProps {
  profiles: SshProfile[];
  busy: string | null;
  onAction: (id: string, action: "test" | "delete" | "edit") => void;
}

function SshProfileList({ profiles, busy, onAction }: SshProfileListProps) {
  if (profiles.length === 0) {
    return <EmptyState title="暂无 SSH 配置" detail="创建 SSH 配置后才能配置 SSH 隧道。" />;
  }
  return (
    <div className="compact-list">
      {profiles.map((profile) => (
        <div className="compact-row" key={profile.id}>
          <div>
            <strong>{profile.name}</strong>
            <small>{profile.username}@{profile.host}:{profile.port} · {sshAuthLabels[profile.authType]} · {sshJumpLabel(profile, profiles)}</small>
          </div>
          <div className="icon-row">
            <IconButton title="测试 SSH" busy={busy === `test-ssh:${profile.id}`} onClick={() => onAction(profile.id, "test")}><CheckCircle2 size={15} /></IconButton>
            <IconButton title="编辑 SSH" busy={busy === `edit-ssh:${profile.id}`} onClick={() => onAction(profile.id, "edit")}><Pencil size={15} /></IconButton>
            <IconButton title="删除 SSH" danger busy={busy === `delete-ssh:${profile.id}`} onClick={() => onAction(profile.id, "delete")}><Trash2 size={15} /></IconButton>
          </div>
        </div>
      ))}
    </div>
  );
}

function sshJumpLabel(profile: SshProfile, profiles: SshProfile[]): string {
  if (!profile.jumpProfileId) return "直连";
  const jump = profiles.find((item) => item.id === profile.jumpProfileId);
  return jump ? `跳板 ${jump.name}` : "跳板已配置";
}

interface SystemProxyPageProps {
  status: SystemProxyStatus;
  candidates: ServiceSummary[];
  profiles: SystemProxyProfile[];
  manualTarget: SystemProxyTarget;
  profileDraft: SystemProxyProfileDraft;
  selectedSourceKey: string;
  editingProfileId: string | null;
  profileDialogOpen: boolean;
  busy: string | null;
  onManualTargetChange: (target: SystemProxyTarget) => void;
  onProfileDraftChange: (draft: SystemProxyProfileDraft) => void;
  onSelectedSourceChange: (key: string) => void;
  onUseSource: (key: string) => void;
  onSaveProfile: (event: FormEvent<HTMLFormElement>) => void;
  onOpenCreateProfile: () => void;
  onCancelProfileEdit: () => void;
  onProfileAction: (id: string, action: "edit" | "delete") => void;
  onSetManual: (target: SystemProxyTarget) => void;
  onClear: () => void;
}

function SystemProxyPage({
  status,
  candidates,
  profiles,
  manualTarget,
  profileDraft,
  selectedSourceKey,
  editingProfileId,
  profileDialogOpen,
  busy,
  onManualTargetChange,
  onProfileDraftChange,
  onSelectedSourceChange,
  onUseSource,
  onSaveProfile,
  onOpenCreateProfile,
  onCancelProfileEdit,
  onProfileAction,
  onSetManual,
  onClear,
}: SystemProxyPageProps) {
  const sources = createSystemProxySources(candidates, profiles, status);
  const selectedSource =
    sources.find((source) => source.key === selectedSourceKey) ??
    sources.find((source) => source.enabled) ??
    sources[0] ??
    null;
  const resolvedSelectedKey = selectedSource?.key ?? "";
  return (
    <div className="system-proxy-layout">
      <section className="panel system-proxy-source-panel">
        <div className="panel-heading">
          <PanelTitle title="系统代理来源" subtitle="HTTP 正向代理和常用配置档统一在这里选择。" />
          <button type="button" className="primary-button" onClick={onOpenCreateProfile}>
            <Plus size={16} />
            添加配置档
          </button>
        </div>
        <SystemProxySourceList
          sources={sources}
          selectedSourceKey={resolvedSelectedKey}
          onSelect={onSelectedSourceChange}
        />
        <DialogShell
          open={profileDialogOpen}
          title={editingProfileId ? "编辑系统代理配置档" : "添加系统代理配置档"}
          description="保存常用系统代理目标后，可在列表中指定启用。"
          onClose={onCancelProfileEdit}
        >
          <SystemProxyProfileForm
            draft={profileDraft}
            busy={busy}
            editing={Boolean(editingProfileId)}
            onDraftChange={onProfileDraftChange}
            onSubmit={onSaveProfile}
            onCancelEdit={onCancelProfileEdit}
          />
        </DialogShell>
      </section>
      <div className="view-stack">
        <section className="panel hero-panel">
          <StatusIcon ok={status.enabled} />
          <h2>{status.enabled ? "系统代理已开启" : "系统代理未开启"}</h2>
          <p>{status.message}</p>
          <dl className="details">
            <div><dt>主机</dt><dd>{status.proxyHost || "-"}</dd></div>
            <div><dt>端口</dt><dd>{status.proxyPort ?? "-"}</dd></div>
            <div><dt>绕过地址</dt><dd>{status.bypass || "-"}</dd></div>
          </dl>
          <div className="button-row">
            <button type="button" className="danger-button" disabled={busy === "system-proxy-clear"} onClick={onClear}><XCircle size={16} />清理</button>
          </div>
        </section>
        <SystemProxySourceActionPanel
          source={selectedSource}
          busy={busy}
          onUse={onUseSource}
          onProfileAction={onProfileAction}
        />
        <section className="panel">
          <PanelTitle title="临时手动目标" subtitle="临时使用本机或局域网代理地址；常用目标建议保存为配置档。" />
          <form className="form-grid" onSubmit={(event) => { event.preventDefault(); onSetManual(manualTarget); }}>
            <FormInput
              label="主机"
              value={manualTarget.proxyHost}
              onChange={(event) => onManualTargetChange({ ...manualTarget, proxyHost: event.currentTarget.value })}
            />
            <FormInput
              label="端口"
              inputMode="numeric"
              value={String(manualTarget.proxyPort)}
              onChange={(event) =>
                onManualTargetChange({ ...manualTarget, proxyPort: Number(event.currentTarget.value) || 0 })
              }
            />
            <FormInput
              fieldClassName="span-2"
              label="绕过地址"
              value={manualTarget.bypass}
              onChange={(event) => onManualTargetChange({ ...manualTarget, bypass: event.currentTarget.value })}
            />
            <div className="button-row span-2">
              <button type="submit" className="primary-button" disabled={busy === "system-proxy-manual"}>
                <ShieldCheck size={16} />使用手动目标设置
              </button>
            </div>
          </form>
        </section>
      </div>
    </div>
  );
}

function createSystemProxySources(
  services: ServiceSummary[],
  profiles: SystemProxyProfile[],
  status: SystemProxyStatus,
): SystemProxySource[] {
  const serviceSources: SystemProxySource[] = services.map((service) => ({
    key: systemProxySourceKey("service", service.id),
    type: "service",
    id: service.id,
    name: service.name,
    target: `${service.listenHost}:${service.listenPort}`,
    detail: `HTTP 正向代理 · ${runtimeStatusLabels[service.runtimeStatus.type]}`,
    enabled: isSystemProxyTargetActive(status, service.listenHost, service.listenPort),
    service,
  }));
  const profileSources: SystemProxySource[] = profiles.map((profile) => ({
    key: systemProxySourceKey("profile", profile.id),
    type: "profile",
    id: profile.id,
    name: profile.name,
    target: `${profile.proxyHost}:${profile.proxyPort}`,
    detail: `配置档 · ${profile.bypass || "无绕过地址"}`,
    enabled: profile.active || isSystemProxyTargetActive(status, profile.proxyHost, profile.proxyPort),
    profile,
  }));
  return [...serviceSources, ...profileSources];
}

function isSystemProxyTargetActive(status: SystemProxyStatus, host: string, port: number): boolean {
  return status.enabled && status.proxyHost === host && status.proxyPort === port;
}

interface SystemProxyProfileFormProps {
  draft: SystemProxyProfileDraft;
  busy: string | null;
  editing: boolean;
  onDraftChange: (draft: SystemProxyProfileDraft) => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onCancelEdit: () => void;
}

function SystemProxyProfileForm({ draft, busy, editing, onDraftChange, onSubmit, onCancelEdit }: SystemProxyProfileFormProps) {
  return (
    <form className="form-grid" onSubmit={onSubmit}>
      <FormInput label="名称" value={draft.name} onChange={(event) => onDraftChange({ ...draft, name: event.currentTarget.value })} />
      <FormInput label="主机" value={draft.proxyHost} onChange={(event) => onDraftChange({ ...draft, proxyHost: event.currentTarget.value })} />
      <FormInput label="端口" inputMode="numeric" value={draft.proxyPort} onChange={(event) => onDraftChange({ ...draft, proxyPort: event.currentTarget.value })} />
      <FormInput label="绕过地址" value={draft.bypass} onChange={(event) => onDraftChange({ ...draft, bypass: event.currentTarget.value })} />
      <div className="button-row span-2">
        <button type="submit" className="primary-button" disabled={busy === "save-system-proxy-profile"}>
          {editing ? <Pencil size={16} /> : <Plus size={16} />}
          {editing ? "保存配置档" : "添加配置档"}
        </button>
        <button type="button" className="ghost-button" onClick={onCancelEdit}>取消</button>
      </div>
    </form>
  );
}

function SystemProxySourceList({
  sources,
  selectedSourceKey,
  onSelect,
}: {
  sources: SystemProxySource[];
  selectedSourceKey: string;
  onSelect: (key: string) => void;
}) {
  if (sources.length === 0) {
    return <EmptyState title="暂无系统代理来源" detail="先添加系统代理配置档，或创建 HTTP 正向代理服务。" />;
  }
  return (
    <div className="compact-list">
      {sources.map((source) => (
        <button
          type="button"
          className={cx("compact-row source-row", selectedSourceKey === source.key && "selected")}
          key={source.key}
          onClick={() => onSelect(source.key)}
        >
          <div>
            <strong>{source.name}</strong>
            <small>{source.target} · {source.detail}</small>
          </div>
          <div className="icon-row">
            {source.enabled && <span className="status-pill running">当前</span>}
            <span className="source-kind">{source.type === "service" ? "服务" : "配置档"}</span>
            {source.type === "service" && <StatusPill status={source.service.runtimeStatus} />}
          </div>
        </button>
      ))}
    </div>
  );
}

function SystemProxySourceActionPanel({
  source,
  busy,
  onUse,
  onProfileAction,
}: {
  source: SystemProxySource | null;
  busy: string | null;
  onUse: (key: string) => void;
  onProfileAction: (id: string, action: "edit" | "delete") => void;
}) {
  if (!source) {
    return (
      <section className="panel">
        <PanelTitle title="启动代理" subtitle="从左侧列表选择一个代理来源。" />
        <EmptyState title="未选择代理来源" detail="选择后可在这里启动或启用系统代理。" />
      </section>
    );
  }
  const isService = source.type === "service";
  const isRunningService = isService && source.service.runtimeStatus.type === "running";
  const useBusy = busy === `use-system-proxy-source:${source.key}`;
  return (
    <section className="panel system-proxy-action-panel">
      <PanelTitle
        title="启动代理"
        subtitle={isService ? "启动对应 HTTP 正向代理并设为系统代理。" : "启用已保存的系统代理配置档。"}
      />
      <div className="selected-source-summary">
        <div>
          <strong>{source.name}</strong>
          <small>{source.target}</small>
        </div>
        <div className="icon-row">
          {source.enabled && <span className="status-pill running">当前</span>}
          {isService && <StatusPill status={source.service.runtimeStatus} />}
        </div>
      </div>
      <dl className="details">
        <div><dt>来源</dt><dd>{isService ? "HTTP 正向代理服务" : "系统代理配置档"}</dd></div>
        <div><dt>目标</dt><dd>{source.target}</dd></div>
        <div><dt>说明</dt><dd>{source.detail}</dd></div>
      </dl>
      <div className="button-row">
        <button type="button" className="primary-button" disabled={useBusy} onClick={() => onUse(source.key)}>
          {useBusy ? <Loader2 size={16} className="spin" /> : <ShieldCheck size={16} />}
          {isService && !isRunningService ? "启动并设为系统代理" : "设为系统代理"}
        </button>
        {source.type === "profile" && (
          <>
            <IconButton title="编辑配置档" busy={busy === `edit-proxy-profile:${source.id}`} onClick={() => onProfileAction(source.id, "edit")}><Pencil size={15} /></IconButton>
            <IconButton title="删除配置档" danger busy={busy === `delete-proxy-profile:${source.id}`} onClick={() => onProfileAction(source.id, "delete")}><Trash2 size={15} /></IconButton>
          </>
        )}
      </div>
    </section>
  );
}

interface ServiceLogsDialogProps {
  service: ServiceSummary | null;
  logs: LogRow[];
  services: ServiceSummary[];
  busy: string | null;
  filter: LogFilter;
  onFilter: (filter: LogFilter) => void;
  onClear: () => void;
  onClose: () => void;
}

function ServiceLogsDialog({ service, logs, services, busy, filter, onFilter, onClear, onClose }: ServiceLogsDialogProps) {
  const [selectedLogId, setSelectedLogId] = useState<number | null>(null);
  const selectedLog = logs.find((log) => log.id === selectedLogId) ?? null;

  useEffect(() => {
    if (selectedLogId !== null && !logs.some((log) => log.id === selectedLogId)) {
      setSelectedLogId(null);
    }
  }, [logs, selectedLogId]);

  if (!service) {
    return null;
  }

  const fixedFilter: LogFilter = { ...filter, serviceId: service.id, limit: 200 };

  return (
    <DialogShell
      open={Boolean(service)}
      title={`${service.name} 日志`}
      description={`${kindLabels[service.kind]} · ${service.listenHost}:${service.listenPort} · 仅显示当前配置运行日志。`}
      onClose={onClose}
      size="wide"
    >
      <form className="filter-bar" onSubmit={(event) => event.preventDefault()}>
        <SelectControl
          aria-label="级别筛选"
          value={filter.level ?? ""}
          onChange={(event) => onFilter({ ...fixedFilter, level: (event.currentTarget.value || null) as LogFilter["level"] })}
          options={[
            { value: "", label: "全部级别" },
            { value: "trace", label: "跟踪" },
            { value: "debug", label: "调试" },
            { value: "info", label: "信息" },
            { value: "warn", label: "警告" },
            { value: "error", label: "错误" },
          ]}
        />
        <SelectControl
          aria-label="协议筛选"
          value={filter.protocol ?? ""}
          onChange={(event) => onFilter({ ...fixedFilter, protocol: event.currentTarget.value || null })}
          options={[
            { value: "", label: "全部协议" },
            { value: "http", label: "HTTP" },
            { value: "https", label: "HTTPS" },
            { value: "tcp", label: "TCP" },
            { value: "udp", label: "UDP" },
            { value: "ssh", label: "SSH" },
          ]}
        />
        <input
          className="form-control"
          aria-label="开始时间"
          type="datetime-local"
          value={filter.createdAfter ?? ""}
          onChange={(event) => onFilter({ ...fixedFilter, createdAfter: event.currentTarget.value || null })}
        />
        <input
          className="form-control"
          aria-label="结束时间"
          type="datetime-local"
          value={filter.createdBefore ?? ""}
          onChange={(event) => onFilter({ ...fixedFilter, createdBefore: event.currentTarget.value || null })}
        />
        <input className="form-control" value={filter.keyword ?? ""} onChange={(event) => onFilter({ ...fixedFilter, keyword: event.currentTarget.value || null })} placeholder="筛选消息或元数据" />
        <button type="button" className="ghost-button" disabled={busy === "filter-service-logs"} onClick={() => onFilter({ serviceId: service.id, limit: 200 })}>重置</button>
        <button type="button" className="ghost-button" disabled={busy === "filter-service-logs"} onClick={() => onFilter(fixedFilter)}><RefreshCw size={15} />刷新</button>
        <button type="button" className="danger-button" disabled={busy === "clear-service-logs"} onClick={onClear}><Trash2 size={15} />清理</button>
      </form>
      <div className="log-workbench">
        <LogList logs={logs} services={services} selectedLogId={selectedLogId} onSelect={setSelectedLogId} />
        <TrafficDetailPanel log={selectedLog} services={services} />
      </div>
    </DialogShell>
  );
}

interface SettingsPageProps {
  settings: AppSetting[];
  autostart: AutostartStatus;
  runtime: ServiceRuntimeSummary[];
  busy: string | null;
  updateProgress: AppUpdateProgress | null;
  onUpdate: (key: string, valueJson: string) => void;
  onAutostartChange: (enabled: boolean) => void;
  onCheckUpdate: () => void;
}

function SettingsPage({ settings, autostart, runtime, busy, updateProgress, onUpdate, onAutostartChange, onCheckUpdate }: SettingsPageProps) {
  const autoStartEnabled = settings.find((setting) => setting.key === "services.auto_start_enabled")?.valueJson === "true";
  return (
    <div className="two-column">
      <section className="panel">
        <PanelTitle title="应用设置" subtitle="区分系统开机启动、应用内服务自动启动和桌面端更新。" />
        <div className="setting-row">
          <div><strong>开机启动应用</strong><small>{autostart.enabled ? "登录系统后自动启动 net-power。" : "登录系统后不自动启动 net-power。"}</small><small>{autostart.message}</small></div>
          <button type="button" className="ghost-button" disabled={busy === "autostart" || !autostart.supported} onClick={() => onAutostartChange(!autostart.enabled)}>
            {autostart.enabled ? "关闭" : "开启"}
          </button>
        </div>
        <div className="setting-row">
          <div><strong>服务自动启动</strong><small>启动应用后自动启动已启用且开启自动启动的服务。</small></div>
          <button type="button" className="ghost-button" disabled={busy === "setting:services.auto_start_enabled"} onClick={() => onUpdate("services.auto_start_enabled", autoStartEnabled ? "false" : "true")}>
            {autoStartEnabled ? "关闭" : "开启"}
          </button>
        </div>
        <div className="setting-row">
          <div>
            <strong>应用更新</strong>
            <small>{updateProgress?.message ?? "从 GitHub Releases 检查、下载并安装桌面端更新。"}</small>
          </div>
          <button type="button" className="ghost-button" disabled={busy === "update"} onClick={onCheckUpdate}>
            {busy === "update" ? <Loader2 size={15} className="spin" /> : <Download size={15} />}
            检查更新
          </button>
        </div>
        <div className="compact-list">
          {settings.map((setting) => <div className="compact-row" key={setting.key}><strong>{setting.key}</strong><small>{setting.valueJson}</small></div>)}
        </div>
      </section>
      <section className="panel">
        <PanelTitle title="运行快照" subtitle="运行服务字节和连接累计。" />
        <div className="compact-list">
          {runtime.map((item) => (
            <div className="compact-row" key={item.serviceId}>
              <strong>{item.listenAddr || item.serviceId}</strong>
              <small>{formatBytes(item.bytesIn)} 入站 · {formatBytes(item.bytesOut)} 出站 · {item.totalConnections} 连接</small>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}

function MetricCard({ label, value, tone }: { label: string; value: string | number; tone: "good" | "bad" | "muted" }) {
  return <article className={`metric-card ${tone}`}><span>{label}</span><strong>{value}</strong></article>;
}

function PanelTitle({ title, subtitle }: { title: string; subtitle: string }) {
  return <div className="panel-title"><h2>{title}</h2><p>{subtitle}</p></div>;
}

type FormInputProps = ComponentPropsWithoutRef<"input"> & {
  label: string;
  fieldClassName?: string;
};

function FormInput({ label, fieldClassName, className, ...props }: FormInputProps) {
  return (
    <label className={cx("form-field", fieldClassName)}>
      <span>{label}</span>
      <input className={cx("form-control", className)} {...props} />
    </label>
  );
}

type SelectControlProps = Omit<ComponentPropsWithoutRef<"select">, "children"> & {
  options: SelectOption[];
};

function SelectControl({ options, className, ...props }: SelectControlProps) {
  return (
    <span className="select-shell">
      <select className={cx("form-control", className)} {...props}>
        {options.map((option) => (
          <option key={option.value} value={option.value} disabled={option.disabled}>
            {option.label}
          </option>
        ))}
      </select>
      <ChevronDown className="select-icon" size={16} aria-hidden="true" />
    </span>
  );
}

type FormSelectProps = SelectControlProps & {
  label: string;
  fieldClassName?: string;
};

function FormSelect({ label, fieldClassName, ...props }: FormSelectProps) {
  return (
    <label className={cx("form-field", fieldClassName)}>
      <span>{label}</span>
      <SelectControl {...props} />
    </label>
  );
}

type FormCheckboxProps = Omit<ComponentPropsWithoutRef<"input">, "type"> & {
  label: string;
  fieldClassName?: string;
};

function FormCheckbox({ label, fieldClassName, className, ...props }: FormCheckboxProps) {
  return (
    <label className={cx("check-row", fieldClassName)}>
      <input type="checkbox" className={cx("form-checkbox", className)} {...props} />
      <span>{label}</span>
    </label>
  );
}

function DialogShell({
  open,
  title,
  description,
  onClose,
  size = "default",
  children,
}: {
  open: boolean;
  title: string;
  description: string;
  onClose: () => void;
  size?: "default" | "wide";
  children: ReactNode;
}) {
  useEffect(() => {
    if (!open) {
      return;
    }
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onClose, open]);

  if (!open) {
    return null;
  }

  const dialog = (
    <div
      className="dialog-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          onClose();
        }
      }}
    >
      <section className={cx("dialog-panel", size === "wide" && "dialog-panel-wide")} role="dialog" aria-modal="true" aria-labelledby="dialog-title">
        <div className="dialog-heading">
          <div>
            <h2 id="dialog-title">{title}</h2>
            <p>{description}</p>
          </div>
          <IconButton title="关闭弹框" busy={false} onClick={onClose}>
            <X size={16} />
          </IconButton>
        </div>
        {children}
      </section>
    </div>
  );

  return createPortal(dialog, document.body);
}

function StatusPill({ status }: { status: RuntimeStatus }) {
  return <span className={`status-pill ${status.type}`}>{status.type === "failed" ? `失败: ${status.message}` : runtimeStatusLabels[status.type]}</span>;
}

function StatusIcon({ ok }: { ok: boolean }) {
  return <div className={ok ? "status-icon ok" : "status-icon"}>{ok ? <CheckCircle2 size={26} /> : <XCircle size={26} />}</div>;
}

function IconButton({ title, busy, danger, onClick, children }: { title: string; busy: boolean; danger?: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button type="button" className={danger ? "icon-button danger" : "icon-button"} title={title} disabled={busy} onClick={onClick}>
      {busy ? <Loader2 size={15} className="spin" /> : children}
    </button>
  );
}

function EmptyState({ title, detail }: { title: string; detail: string }) {
  return <div className="empty-state"><FileText size={24} /><strong>{title}</strong><p>{detail}</p></div>;
}

function LoadingRows() {
  return <div className="loading-rows"><span /><span /><span /></div>;
}

function LogList({
  logs,
  services,
  selectedLogId,
  onSelect,
}: {
  logs: LogRow[];
  services: ServiceSummary[];
  selectedLogId?: number | null;
  onSelect?: (id: number) => void;
}) {
  if (logs.length === 0) {
    return <EmptyState title="暂无日志" detail="启动或测试服务后会生成日志事件。" />;
  }
  return (
    <div className="log-list">
      {logs.map((log) => (
        <article key={log.id} className={`log-row ${log.level} ${selectedLogId === log.id ? "selected" : ""}`}>
          <span>{formatTime(log.createdAt)}</span>
          <strong>{logLevelLabels[log.level]}</strong>
          <div className="log-message">
            <p>{log.message}</p>
            <code className="log-meta">{compactJson(log.metaJson)}</code>
          </div>
          <small>{serviceName(services, log.serviceId)}</small>
          {onSelect && (
            <button type="button" className="ghost-button log-detail-button" onClick={() => onSelect(log.id)}>
              详情
            </button>
          )}
        </article>
      ))}
    </div>
  );
}

function TrafficDetailPanel({ log, services }: { log: LogRow | null; services: ServiceSummary[] }) {
  if (!log) {
    return (
      <aside className="traffic-detail">
        <PanelTitle title="流量详情" subtitle="选择一条日志查看连接级抓包元数据。" />
        <EmptyState title="未选择日志" detail="详情只展示元数据，不保存 request/response body 和敏感 header。" />
      </aside>
    );
  }

  const metaEntries = logMetaEntries(log.metaJson);
  return (
    <aside className="traffic-detail">
      <PanelTitle title="流量详情" subtitle="隐私安全的连接与服务事件元数据。" />
      <dl className="details">
        <div><dt>服务</dt><dd>{serviceName(services, log.serviceId)}</dd></div>
        <div><dt>级别</dt><dd>{logLevelLabels[log.level]}</dd></div>
        <div><dt>时间</dt><dd>{formatTime(log.createdAt)}</dd></div>
        <div><dt>消息</dt><dd>{log.message}</dd></div>
      </dl>
      <div className="traffic-meta-grid">
        {metaEntries.map(([key, value]) => (
          <div key={key}>
            <span>{humanizeMetaKey(key)}</span>
            <strong>{formatMetaValue(key, value)}</strong>
          </div>
        ))}
      </div>
      <div className="traffic-json">
        <strong>完整 meta JSON</strong>
        <code>{prettyJson(log.metaJson)}</code>
      </div>
    </aside>
  );
}

function compactJson(value: string): string {
  try {
    return JSON.stringify(JSON.parse(value));
  } catch {
    return value || "{}";
  }
}

function prettyJson(value: string): string {
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value || "{}";
  }
}

function logMetaEntries(value: string): Array<[string, unknown]> {
  try {
    const parsed = JSON.parse(value) as unknown;
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return [["value", parsed ?? ""]];
    }
    return Object.entries(parsed as Record<string, unknown>);
  } catch {
    return [["value", value || "{}"]];
  }
}

function humanizeMetaKey(key: string): string {
  const labels: Record<string, string> = {
    source: "来源",
    protocol: "协议",
    remoteAddr: "客户端",
    targetAddr: "目标",
    eventType: "事件类型",
    method: "方法",
    host: "Host",
    path: "路径",
    statusCode: "状态码",
    bytesIn: "入站流量",
    bytesOut: "出站流量",
    durationMs: "耗时",
    error: "错误",
  };
  return labels[key] ?? key;
}

function formatMetaValue(key: string, value: unknown): string {
  if (value === null || value === undefined || value === "") {
    return "-";
  }
  if (key === "bytesIn" || key === "bytesOut") {
    return typeof value === "number" ? formatBytes(value) : String(value);
  }
  if (key === "durationMs") {
    return `${value} ms`;
  }
  if (typeof value === "boolean") {
    return value ? "是" : "否";
  }
  if (typeof value === "object") {
    return JSON.stringify(value);
  }
  return String(value);
}

function parseSystemProxyProfileDraft(draft: SystemProxyProfileDraft): SystemProxyProfileInput | string {
  const proxyPort = Number(draft.proxyPort);
  if (!draft.name.trim()) {
    return "配置档名称不能为空。";
  }
  if (!draft.proxyHost.trim()) {
    return "代理主机不能为空。";
  }
  if (!Number.isInteger(proxyPort) || proxyPort < 1 || proxyPort > 65535) {
    return "代理端口必须是 1-65535。";
  }
  return {
    name: draft.name.trim(),
    proxyHost: draft.proxyHost.trim(),
    proxyPort,
    bypass: draft.bypass.trim(),
  };
}

function systemProxyProfileToDraft(profile: SystemProxyProfile): SystemProxyProfileDraft {
  return {
    name: profile.name,
    proxyHost: profile.proxyHost,
    proxyPort: String(profile.proxyPort),
    bypass: profile.bypass,
  };
}

function ToastStack({ toasts }: { toasts: Toast[] }) {
  return <div className="toast-stack">{toasts.map((toast) => <div key={toast.id} className={`toast ${toast.kind}`}><strong>{toast.title}</strong>{toast.detail && <span>{toast.detail}</span>}</div>)}</div>;
}
