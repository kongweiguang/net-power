/**
 * @author kongweiguang
 * Net Power 主工作台。界面直接面向代理管理工作流，不提供营销页。
 */

import { useEffect, useMemo, useState, type FormEvent } from "react";
import {
  Activity,
  Loader2,
  Network,
  RefreshCw,
  Server,
  Settings,
  ShieldCheck,
  TerminalSquare,
} from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  autostartApi,
  fileDialogApi,
  isTauriRuntime,
  logsApi,
  networkApi,
  servicesApi,
  settingsApi,
  sshProfilesApi,
  systemProxyApi,
  toolServicesApi,
  updaterApi,
} from "../../api";
import type {
  AppSetting,
  AppUpdateProgress,
  LogFilter,
  LogRow,
  ServiceSummary,
  TestResult,
  ToolServiceSummary,
} from "../../types";
import {
  defaultServiceDraft,
  defaultSshDraft,
  bindModeFromHost,
  filterServicesByPage,
  pageForServiceKind,
  pageTitle,
  parseServiceDraft,
  parseSshDraft,
  parseToolServiceDraft,
  readError,
  serviceDetailToDraft,
  serviceAddressCopyPayload,
  sshProfileToDraft,
  toolServiceAddressCopyPayload,
  toolServiceConfigToDraft,
  type PageKey,
  type ServiceDraft,
  type SshDraft,
  type ToolServiceDraft,
} from "./workbenchModel";
import { Dashboard, ServicePage, SshPage, ToolServicesPage } from "./WorkbenchSections";
import {
  ServiceLogsDialog,
  SettingsPage,
  SystemProxyPage,
  ToastStack,
  parseSystemProxyProfileDraft,
  systemProxyProfileToDraft,
} from "./WorkbenchPanels";
import {
  cx,
  defaultSystemProxyProfileDraft,
  newToolServiceDraft,
  normalizeDraftForPage,
  serviceKindsForPage,
  type SystemProxyProfileDraft,
} from "./workbenchShared";
import {
  themeModeSettingKey,
  themeModeValueJson,
  useThemeMode,
  type ThemeMode,
} from "./useThemeMode";
import { useWorkbenchData } from "./useWorkbenchData";
import { useWorkbenchToasts } from "./useWorkbenchToasts";

const primaryNavItems: Array<{ key: PageKey; label: string; icon: typeof Activity }> = [
  { key: "dashboard", label: "仪表盘", icon: Activity },
  { key: "services", label: "本地服务", icon: Server },
  { key: "forwarding", label: "网络转发", icon: Network },
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
  const { toasts, toast } = useWorkbenchToasts();
  const {
    services,
    toolServices,
    setToolServices,
    profiles,
    setProfiles,
    proxyProfiles,
    setProxyProfiles,
    settings,
    setSettings,
    autostartStatus,
    setAutostartStatus,
    proxyStatus,
    setProxyStatus,
    loading,
    error,
    reloadAll,
    syncRuntime,
  } = useWorkbenchData(toast);
  const [serviceDraft, setServiceDraft] = useState<ServiceDraft>(defaultServiceDraft);
  const [toolServiceDraft, setToolServiceDraft] = useState<ToolServiceDraft>(() => newToolServiceDraft());
  const [sshDraft, setSshDraft] = useState<SshDraft>(defaultSshDraft);
  const [proxyProfileDraft, setProxyProfileDraft] = useState<SystemProxyProfileDraft>(defaultSystemProxyProfileDraft);
  const [selectedSystemProxySourceKey, setSelectedSystemProxySourceKey] = useState("");
  const [editingServiceId, setEditingServiceId] = useState<string | null>(null);
  const [serviceDialogOpen, setServiceDialogOpen] = useState(false);
  const [toolServiceDialogOpen, setToolServiceDialogOpen] = useState(false);
  const [editingToolServiceId, setEditingToolServiceId] = useState<string | null>(null);
  const [editingSshProfileId, setEditingSshProfileId] = useState<string | null>(null);
  const [sshProfileDialogOpen, setSshProfileDialogOpen] = useState(false);
  const [editingProxyProfileId, setEditingProxyProfileId] = useState<string | null>(null);
  const [proxyProfileDialogOpen, setProxyProfileDialogOpen] = useState(false);
  const [logDialogService, setLogDialogService] = useState<ServiceSummary | null>(null);
  const [serviceLogs, setServiceLogs] = useState<LogRow[]>([]);
  const [serviceLogFilter, setServiceLogFilter] = useState<LogFilter>({ limit: 200 });
  const [busy, setBusy] = useState<string | null>(null);
  const [updateProgress, setUpdateProgress] = useState<AppUpdateProgress | null>(null);
  const { themeMode } = useThemeMode(settings);

  const runningCount = services.filter((service) => service.runtimeStatus.type === "running").length;
  const stoppedCount = services.filter((service) => service.runtimeStatus.type === "stopped").length;
  const failedCount = services.filter((service) => service.runtimeStatus.type === "failed").length;
  const runningToolServiceCount = toolServices.filter((service) => service.runtimeStatus.type === "running").length;
  const visibleServices = useMemo(() => filterServicesByPage(services, page), [services, page]);
  const httpForwardServices = services.filter((service) => service.kind === "http_forward");

  useEffect(() => {
    if (editingServiceId) {
      return;
    }
    setServiceDraft((current) => normalizeDraftForPage(page, current));
  }, [editingServiceId, page]);

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

  async function saveToolService(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const input = parseToolServiceDraft(toolServiceDraft);
    if (typeof input === "string") {
      toast("error", "工具服务校验失败", input);
      return;
    }
    setBusy("save-tool-service");
    try {
      const summary = editingToolServiceId
        ? await toolServicesApi.update(editingToolServiceId, input)
        : await toolServicesApi.create(input);
      setToolServices(await toolServicesApi.list());
      setToolServiceDraft(newToolServiceDraft());
      setEditingToolServiceId(null);
      setToolServiceDialogOpen(false);
      toast("success", editingToolServiceId ? "工具服务配置已更新" : "工具服务已创建并启动", summary.url);
    } catch (err) {
      try {
        setToolServices(await toolServicesApi.list());
      } catch (listErr) {
        console.warn("刷新工具服务列表失败", listErr);
      }
      toast("error", editingToolServiceId ? "更新工具服务失败" : "创建或启动工具服务失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function toggleToolService(service: ToolServiceSummary) {
    const running = service.runtimeStatus.type === "running" || service.runtimeStatus.type === "starting";
    const action = running ? "stop" : "start";
    setBusy(`${action}-tool:${service.id}`);
    try {
      if (running) {
        await toolServicesApi.stop(service.id);
      } else {
        await toolServicesApi.start(service.id);
      }
      setToolServices(await toolServicesApi.list());
      toast("success", running ? "工具服务已暂停" : "工具服务已启动", service.name);
    } catch (err) {
      toast("error", running ? "暂停工具服务失败" : "启动工具服务失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function deleteToolService(id: string) {
    setBusy(`delete-tool:${id}`);
    try {
      await toolServicesApi.delete(id);
      setToolServices(await toolServicesApi.list());
      if (editingToolServiceId === id) {
        setEditingToolServiceId(null);
        setToolServiceDraft(newToolServiceDraft());
        setToolServiceDialogOpen(false);
      }
      toast("success", "工具服务配置已删除");
    } catch (err) {
      toast("error", "删除工具服务失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function resolveLanIpForCopy(host: string): Promise<string | null> {
    if (bindModeFromHost(host) !== "lan") {
      return null;
    }
    try {
      return await networkApi.getLanIp();
    } catch (err) {
      console.warn("读取局域网 IP 失败", err);
      return null;
    }
  }

  async function writeAddressToClipboard(text: string, detail: string) {
    if (!navigator.clipboard) {
      toast("error", "复制失败", "当前环境不支持剪贴板。");
      return;
    }
    try {
      await navigator.clipboard.writeText(text);
      toast("success", "地址已复制", detail);
    } catch (err) {
      toast("error", "复制失败", readError(err));
    }
  }

  async function copyServiceAddress(service: ServiceSummary) {
    const lanIp = await resolveLanIpForCopy(service.listenHost);
    const payload = serviceAddressCopyPayload(service, lanIp);
    await writeAddressToClipboard(payload.text, payload.detail);
  }

  async function copyToolServiceAddress(service: ToolServiceSummary) {
    const lanIp = await resolveLanIpForCopy(service.host);
    const payload = toolServiceAddressCopyPayload(service, lanIp);
    await writeAddressToClipboard(payload.text, payload.detail);
  }

  function openToolServiceDialog() {
    setEditingToolServiceId(null);
    setToolServiceDraft(newToolServiceDraft());
    setToolServiceDialogOpen(true);
  }

  function closeToolServiceDialog() {
    setEditingToolServiceId(null);
    setToolServiceDraft(newToolServiceDraft());
    setToolServiceDialogOpen(false);
  }

  async function editToolService(id: string) {
    setBusy(`edit-tool:${id}`);
    try {
      const config = await toolServicesApi.get(id);
      setToolServiceDraft(toolServiceConfigToDraft(config));
      setEditingToolServiceId(id);
      setToolServiceDialogOpen(true);
      toast("info", "已载入服务配置编辑", config.name);
    } catch (err) {
      toast("error", "读取工具服务配置失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function chooseToolServiceRootDir() {
    setBusy("choose-tool-root");
    try {
      const selected = await fileDialogApi.chooseDirectory();
      if (selected) {
        setToolServiceDraft((current) => ({ ...current, staticRootDir: selected }));
      }
    } catch (err) {
      toast("error", "选择文件夹失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function chooseToolServiceResponseFile(routeId: string) {
    setBusy(`choose-tool-file:${routeId}`);
    try {
      const selected = await fileDialogApi.chooseFile();
      if (selected) {
        setToolServiceDraft((current) => ({
          ...current,
          routes: current.routes.map((route) =>
            route.id === routeId ? { ...route, contentSource: "file", filePath: selected } : route,
          ),
        }));
      }
    } catch (err) {
      toast("error", "选择响应文件失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function chooseSshPrivateKey() {
    setBusy("choose-ssh-private-key");
    try {
      const selected = await fileDialogApi.chooseFile();
      if (selected) {
        setSshDraft((current) => ({ ...current, privateKeyPath: selected }));
      }
    } catch (err) {
      toast("error", "选择私钥文件失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function chooseSshKnownHosts() {
    setBusy("choose-ssh-known-hosts");
    try {
      const selected = await fileDialogApi.chooseFile();
      if (selected) {
        setSshDraft((current) => ({ ...current, knownHostsPath: selected }));
      }
    } catch (err) {
      toast("error", "选择 known_hosts 文件失败", readError(err));
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
          toast("error", "系统代理配置不存在");
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
          service.runtimeStatus.type === "running" ? status.message : "服务已启动，系统代理已启用",
          service.name,
        );
        return;
      }

      if (sourceKey.startsWith(profilePrefix)) {
        const profileId = sourceKey.slice(profilePrefix.length);
        const profile = proxyProfiles.find((item) => item.id === profileId);
        if (!profile) {
          toast("error", "系统代理配置不存在");
          return;
        }
        const status = await systemProxyApi.set(profile.id);
        setProxyStatus(status);
        setProxyProfiles(await systemProxyApi.listProfiles());
        toast("success", status.message, profile.name);
        return;
      }

      toast("error", "系统代理配置无效");
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
      toast("error", "系统代理配置校验失败", input);
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
      toast("success", editingProxyProfileId ? "系统代理配置已更新" : "系统代理配置已创建", input.name);
      setProxyProfiles(await systemProxyApi.listProfiles());
    } catch (err) {
      toast("error", editingProxyProfileId ? "更新系统代理配置失败" : "创建系统代理配置失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  async function proxyProfileAction(id: string, action: "edit" | "delete") {
    const profile = proxyProfiles.find((item) => item.id === id);
    if (!profile) {
      toast("error", "系统代理配置不存在");
      return;
    }
    if (action === "edit") {
      setProxyProfileDraft(systemProxyProfileToDraft(profile));
      setEditingProxyProfileId(id);
      setProxyProfileDialogOpen(true);
      toast("info", "已载入系统代理配置编辑", profile.name);
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
      toast("success", "系统代理配置已删除", profile.name);
    } catch (err) {
      toast("error", "系统代理配置操作失败", readError(err));
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
      setSettings(upsertSetting(await settingsApi.list(), key, valueJson));
      toast("success", "设置已保存", key);
    } catch (err) {
      toast("error", "保存设置失败", readError(err));
    } finally {
      setBusy(null);
    }
  }

  function updateThemeMode(mode: ThemeMode) {
    void updateSetting(themeModeSettingKey, themeModeValueJson(mode));
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

  return (
    <main className="app-frame">
      <aside className="sidebar">
        <div className="brand">
          <Network className="brand-icon" size={32} aria-hidden="true" />
          <div>
            <strong>Net Power</strong>
            <span>代理控制台</span>
          </div>
        </div>
        <nav aria-label="主导航">
          {primaryNavItems.map((item) => {
            const Icon = item.icon;
            return (
              <div className={cx("nav-group", item.key === "settings" && "nav-group-bottom")} key={item.key}>
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
            onCopyAddress={copyServiceAddress}
            onEdit={editService}
            onLogs={openServiceLogs}
          />
        )}

        {page === "services" && (
          <ToolServicesPage
            services={toolServices}
            draft={toolServiceDraft}
            runningCount={runningToolServiceCount}
            busy={busy}
            dialogOpen={toolServiceDialogOpen}
            editingServiceId={editingToolServiceId}
            onDraftChange={setToolServiceDraft}
            onOpenCreate={openToolServiceDialog}
            onCloseDialog={closeToolServiceDialog}
            onCreate={saveToolService}
            onToggle={toggleToolService}
            onEdit={editToolService}
            onDelete={deleteToolService}
            onCopyAddress={copyToolServiceAddress}
            onChooseDirectory={chooseToolServiceRootDir}
            onChooseFile={chooseToolServiceResponseFile}
          />
        )}

        {page === "forwarding" && (
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
            onCopyAddress={copyServiceAddress}
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
            onServiceCopyAddress={copyServiceAddress}
            onServiceEdit={editService}
            onServiceLogs={openServiceLogs}
            onSshAction={sshAction}
            onChoosePrivateKey={chooseSshPrivateKey}
            onChooseKnownHosts={chooseSshKnownHosts}
          />
        )}

        {page === "system" && (
          <SystemProxyPage
            status={proxyStatus}
            candidates={httpForwardServices}
            profiles={proxyProfiles}
            profileDraft={proxyProfileDraft}
            selectedSourceKey={selectedSystemProxySourceKey}
            editingProfileId={editingProxyProfileId}
            profileDialogOpen={proxyProfileDialogOpen}
            busy={busy}
            onProfileDraftChange={setProxyProfileDraft}
            onSelectedSourceChange={setSelectedSystemProxySourceKey}
            onUseSource={enableSystemProxySource}
            onSaveProfile={saveSystemProxyProfile}
            onOpenCreateProfile={openSystemProxyProfileCreateDialog}
            onCancelProfileEdit={closeSystemProxyProfileDialog}
            onProfileAction={proxyProfileAction}
            onClear={clearSystemProxy}
          />
        )}

        {page === "settings" && (
          <SettingsPage
            settings={settings}
            autostart={autostartStatus}
            themeMode={themeMode}
            busy={busy}
            updateProgress={updateProgress}
            onUpdate={updateSetting}
            onAutostartChange={updateAutostart}
            onThemeModeChange={updateThemeMode}
            onCheckUpdate={checkForUpdate}
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

function upsertSetting(settings: AppSetting[], key: string, valueJson: string): AppSetting[] {
  if (settings.some((setting) => setting.key === key)) {
    return settings.map((setting) => (setting.key === key ? { ...setting, valueJson } : setting));
  }
  return [...settings, { key, valueJson }];
}
