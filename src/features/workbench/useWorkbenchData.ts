/**
 * @author kongweiguang
 * Workbench 数据加载、运行态同步和 Tauri 事件订阅。
 */

import { useCallback, useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  autostartApi,
  isTauriRuntime,
  servicesApi,
  settingsApi,
  sshProfilesApi,
  systemProxyApi,
  toolServicesApi,
} from "../../api";
import type {
  AppSetting,
  AutostartStatus,
  ConnectionEventPayload,
  ServiceEventPayload,
  ServiceRuntimeSummary,
  ServiceSummary,
  SshProfile,
  SystemProxyProfile,
  SystemProxyStatus,
  ToolServiceSummary,
} from "../../types";
import { readError } from "./workbenchModel";
import type { ToastFn } from "./useWorkbenchToasts";

/** Workbench 后端数据和运行态同步入口。 */
export function useWorkbenchData(toast: ToastFn) {
  const [services, setServices] = useState<ServiceSummary[]>([]);
  const [runtime, setRuntime] = useState<ServiceRuntimeSummary[]>([]);
  const [toolServices, setToolServices] = useState<ToolServiceSummary[]>([]);
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
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const reloadAll = useCallback(async () => {
    setLoading(true);
    setError(null);
    const [
      serviceResult,
      runtimeResult,
      toolServiceResult,
      profileResult,
      proxyProfileResult,
      settingResult,
      proxyResult,
      autostartResult,
    ] = await Promise.allSettled([
      servicesApi.list(),
      servicesApi.listRuntimeStatus(),
      toolServicesApi.list(),
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
    if (toolServiceResult.status === "fulfilled") {
      setToolServices(toolServiceResult.value);
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
  }, []);

  const syncRuntime = useCallback(async () => {
    const [nextServices, nextRuntime, nextToolServices, nextProxy, nextProxyProfiles] =
      await Promise.all([
        servicesApi.list(),
        servicesApi.listRuntimeStatus(),
        toolServicesApi.list(),
        systemProxyApi.getStatus(),
        systemProxyApi.listProfiles(),
      ]);
    setServices(nextServices);
    setRuntime(nextRuntime);
    setToolServices(nextToolServices);
    setProxyStatus(nextProxy);
    setProxyProfiles(nextProxyProfiles);
  }, []);

  useEffect(() => {
    void reloadAll();
  }, [reloadAll]);

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
  }, [syncRuntime, toast]);

  return {
    services,
    runtime,
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
  };
}
