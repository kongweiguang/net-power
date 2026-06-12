/**
 * @author kongweiguang
 * Workbench 系统代理、日志、设置和通用 UI 组件。
 */

import { useEffect, useId, useRef, useState, type ComponentPropsWithoutRef, type FormEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import {
  CheckCircle2,
  ChevronDown,
  Download,
  ExternalLink,
  FileText,
  Info,
  Loader2,
  Palette,
  Pencil,
  Power,
  Plus,
  RefreshCw,
  Save,
  ShieldCheck,
  Trash2,
  X,
  XCircle,
} from "lucide-react";
import type {
  AppSetting,
  AppUpdateProgress,
  AutostartStatus,
  LogFilter,
  LogRow,
  RuntimeStatus,
  ServiceSummary,
  SystemProxyProfile,
  SystemProxyProfileInput,
  SystemProxyStatus,
} from "../../types";
import packageJson from "../../../package.json";
import { formatBytes, formatTime, kindLabels, serviceName } from "./workbenchModel";
import {
  cx,
  logLevelLabels,
  runtimeStatusLabels,
  systemProxySourceKey,
  type SelectOption,
  type SystemProxyProfileDraft,
  type SystemProxySource,
  type Toast,
} from "./workbenchShared";
import {
  themeModeOptions,
  themeModeSettingKey,
  type ThemeMode,
} from "./useThemeMode";

const appVersion = packageJson.version;
const githubRepositoryUrl = "https://github.com/kongweiguang/net-power";
const githubRepositoryLabel = "github.com/kongweiguang/net-power";

interface SystemProxyPageProps {
  status: SystemProxyStatus;
  candidates: ServiceSummary[];
  profiles: SystemProxyProfile[];
  profileDraft: SystemProxyProfileDraft;
  selectedSourceKey: string;
  editingProfileId: string | null;
  profileDialogOpen: boolean;
  busy: string | null;
  onProfileDraftChange: (draft: SystemProxyProfileDraft) => void;
  onSelectedSourceChange: (key: string) => void;
  onUseSource: (key: string) => void;
  onSaveProfile: (event: FormEvent<HTMLFormElement>) => void;
  onOpenCreateProfile: () => void;
  onCancelProfileEdit: () => void;
  onProfileAction: (id: string, action: "edit" | "delete") => void;
  onClear: () => void;
}

export function SystemProxyPage({
  status,
  candidates,
  profiles,
  profileDraft,
  selectedSourceKey,
  editingProfileId,
  profileDialogOpen,
  busy,
  onProfileDraftChange,
  onSelectedSourceChange,
  onUseSource,
  onSaveProfile,
  onOpenCreateProfile,
  onCancelProfileEdit,
  onProfileAction,
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
    <div className="system-proxy-layout system-proxy-layout-simple">
      <section className="panel system-proxy-source-panel">
        <div className="panel-heading">
          <PanelTitle title="代理配置列表" subtitle="添加常用 host/port 后，从列表里直接启用系统代理。" />
          <button type="button" className="primary-button" onClick={onOpenCreateProfile}>
            <Plus size={16} />
            添加配置
          </button>
        </div>
        <SystemProxyStatusStrip status={status} busy={busy} onClear={onClear} />
        <div className="system-proxy-workspace">
          <SystemProxySourceList
            sources={sources}
            selectedSourceKey={resolvedSelectedKey}
            onSelect={onSelectedSourceChange}
          />
          <SystemProxySourceDetail
            source={selectedSource}
            busy={busy}
            onUse={onUseSource}
            onProfileAction={onProfileAction}
          />
        </div>
        <DialogShell
          open={profileDialogOpen}
          title={editingProfileId ? "编辑系统代理配置" : "添加系统代理配置"}
          description="保存常用系统代理目标后，会出现在系统代理列表里。"
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
    </div>
  );
}

function SystemProxyStatusStrip({
  status,
  busy,
  onClear,
}: {
  status: SystemProxyStatus;
  busy: string | null;
  onClear: () => void;
}) {
  const target = status.enabled && status.proxyHost && status.proxyPort !== null
    ? `${status.proxyHost}:${status.proxyPort}`
    : "未设置";
  const bypass = status.enabled && status.bypass ? status.bypass : "无绕过地址";

  return (
    <div className={cx("system-proxy-status-strip", status.enabled && "active")}>
      <div className="system-proxy-status-main">
        <span className="system-proxy-status-dot" aria-hidden="true" />
        <div>
          <span className="system-proxy-status-kicker">系统代理</span>
          <strong>{status.enabled ? "已开启" : "未开启"}</strong>
          <small>{status.message}</small>
        </div>
      </div>
      <dl className="system-proxy-status-details">
        <div><dt>目标</dt><dd>{target}</dd></div>
        <div><dt>绕过</dt><dd>{bypass}</dd></div>
      </dl>
      <button type="button" className="ghost-button system-proxy-clear-button" disabled={busy === "system-proxy-clear"} onClick={onClear}>
        {busy === "system-proxy-clear" ? <Loader2 size={16} className="spin" /> : <XCircle size={16} />}
        清理
      </button>
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
    detail: `配置 · ${profile.bypass || "无绕过地址"}`,
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
          {editing ? <Save size={16} /> : <Plus size={16} />}
          {editing ? "保存配置" : "添加配置"}
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
    return <EmptyState title="暂无代理配置" detail="点击添加配置后，就可以从列表里启用系统代理。" />;
  }
  return (
    <div className="compact-list source-menu-list">
      {sources.map((source) => {
        const isService = source.type === "service";
        return (
          <div className={cx("compact-row source-row", selectedSourceKey === source.key && "selected")} key={source.key}>
            <button type="button" className="source-row-main" onClick={() => onSelect(source.key)}>
              <strong>{source.name}</strong>
              <small>{source.target} · {source.detail}</small>
            </button>
            <div className="source-row-meta">
              {source.enabled && <span className="status-pill running">当前</span>}
              <span className="source-kind">{isService ? "服务" : "配置"}</span>
              {isService && <StatusPill status={source.service.runtimeStatus} />}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function SystemProxySourceDetail({
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
    return <EmptyState title="暂无可用代理" detail="添加配置或启动 HTTP 正向代理后，就可以在这里启用系统代理。" />;
  }

  const isService = source.type === "service";
  const isRunningService = isService && source.service.runtimeStatus.type === "running";
  const useBusy = busy === `use-system-proxy-source:${source.key}`;
  const bypass = source.type === "profile" ? source.profile.bypass || "-" : "跟随系统默认";
  const statusLabel = source.enabled
    ? "当前启用"
    : isService
      ? runtimeStatusLabels[source.service.runtimeStatus.type]
      : "未启用";

  return (
    <aside className="system-proxy-detail" aria-label="系统代理配置详情">
      <div className="system-proxy-detail-heading">
        <div className="system-proxy-detail-title">
          <span className="system-proxy-detail-icon">
            {isService ? <Power size={18} /> : <FileText size={18} />}
          </span>
          <div>
            <span className="source-kind">{isService ? "运行服务" : "保存配置"}</span>
            <strong>{source.name}</strong>
            <small>{source.enabled ? "当前系统代理目标" : "可切换为系统代理目标"}</small>
          </div>
        </div>
        {source.enabled && <span className="status-pill running">当前</span>}
      </div>
      <dl className="system-proxy-detail-grid">
        <div>
          <dt>目标</dt>
          <dd>{source.target}</dd>
        </div>
        <div>
          <dt>状态</dt>
          <dd>{statusLabel}</dd>
        </div>
        <div>
          <dt>绕过</dt>
          <dd>{bypass}</dd>
        </div>
      </dl>
      <div className="system-proxy-detail-actions">
        <button type="button" className="primary-button" disabled={useBusy} onClick={() => onUse(source.key)}>
          {useBusy ? <Loader2 size={16} className="spin" /> : <ShieldCheck size={16} />}
          {isService && !isRunningService ? "启动后启用代理" : source.enabled ? "重新应用" : "启用代理"}
        </button>
        {source.type === "profile" && (
          <>
            <IconButton title="编辑配置" busy={busy === `edit-proxy-profile:${source.id}`} onClick={() => onProfileAction(source.id, "edit")}><Pencil size={15} /></IconButton>
            <IconButton title="删除配置" danger busy={busy === `delete-proxy-profile:${source.id}`} onClick={() => onProfileAction(source.id, "delete")}><Trash2 size={15} /></IconButton>
          </>
        )}
      </div>
    </aside>
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

export function ServiceLogsDialog({ service, logs, services, busy, filter, onFilter, onClear, onClose }: ServiceLogsDialogProps) {
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
  themeMode: ThemeMode;
  busy: string | null;
  updateProgress: AppUpdateProgress | null;
  onUpdate: (key: string, valueJson: string) => void;
  onAutostartChange: (enabled: boolean) => void;
  onThemeModeChange: (mode: ThemeMode) => void;
  onCheckUpdate: () => void;
}

export function SettingsPage({
  settings,
  autostart,
  themeMode,
  busy,
  updateProgress,
  onUpdate,
  onAutostartChange,
  onThemeModeChange,
  onCheckUpdate,
}: SettingsPageProps) {
  const autoStartEnabled = settings.find((setting) => setting.key === "services.auto_start_enabled")?.valueJson === "true";
  const themeModeDetail: Record<ThemeMode, string> = {
    light: "固定使用浅色界面。",
    dark: "固定使用深色界面。",
    system: "跟随操作系统外观。",
  };
  return (
    <div className="settings-layout">
      <section className="panel settings-panel settings-panel-refined">
        <PanelTitle title="应用设置" subtitle="管理启动策略、桌面端更新和应用信息。" />
        <div className="settings-content-grid">
          <div className="settings-category-stack">
            <SettingsCategory id="settings-appearance" title="外观" icon={<Palette size={16} aria-hidden="true" />}>
              <SettingRow title="主题模式" detail={themeModeDetail[themeMode]}>
                <SelectControl
                  aria-label="主题模式"
                  className="setting-select"
                  disabled={busy === `setting:${themeModeSettingKey}`}
                  value={themeMode}
                  onChange={(event) => onThemeModeChange(event.currentTarget.value as ThemeMode)}
                  options={themeModeOptions}
                />
              </SettingRow>
            </SettingsCategory>

            <SettingsCategory id="settings-startup" title="启动" icon={<Power size={16} aria-hidden="true" />}>
              <SettingRow
                title="开机启动应用"
                detail={autostart.enabled ? "登录系统后自动启动 net-power。" : "登录系统后不自动启动 net-power。"}
                extraDetail={autostart.message}
              >
                <button type="button" className="ghost-button" disabled={busy === "autostart" || !autostart.supported} onClick={() => onAutostartChange(!autostart.enabled)}>
                  {autostart.enabled ? "关闭" : "开启"}
                </button>
              </SettingRow>
              <SettingRow title="服务自动启动" detail="启动应用后自动启动已启用且开启自动启动的服务。">
                <button type="button" className="ghost-button" disabled={busy === "setting:services.auto_start_enabled"} onClick={() => onUpdate("services.auto_start_enabled", autoStartEnabled ? "false" : "true")}>
                  {autoStartEnabled ? "关闭" : "开启"}
                </button>
              </SettingRow>
            </SettingsCategory>

            <SettingsCategory id="settings-update" title="更新" icon={<RefreshCw size={16} aria-hidden="true" />}>
              <SettingRow title="应用更新" detail={updateProgress?.message ?? "从 GitHub Releases 检查、下载并安装桌面端更新。"}>
                <button type="button" className="ghost-button" disabled={busy === "update"} onClick={onCheckUpdate}>
                  {busy === "update" ? <Loader2 size={15} className="spin" /> : <Download size={15} />}
                  检查更新
                </button>
              </SettingRow>
            </SettingsCategory>

            <SettingsCategory id="settings-about" title="关于" icon={<Info size={16} aria-hidden="true" />}>
              <dl className="about-list">
                <div>
                  <dt>版本号</dt>
                  <dd>{appVersion}</dd>
                </div>
                <div>
                  <dt>GitHub</dt>
                  <dd>
                    <a className="settings-link" href={githubRepositoryUrl} target="_blank" rel="noreferrer">
                      <ExternalLink size={15} />
                      {githubRepositoryLabel}
                    </a>
                  </dd>
                </div>
              </dl>
            </SettingsCategory>
          </div>
        </div>
      </section>
    </div>
  );
}

function SettingsCategory({ id, title, icon, children }: { id: string; title: string; icon: ReactNode; children: ReactNode }) {
  return (
    <section id={id} className="settings-category">
      <div className="settings-category-heading">
        <span className="settings-category-icon">{icon}</span>
        <h3>{title}</h3>
      </div>
      <div className="settings-category-content">{children}</div>
    </section>
  );
}

function SettingRow({ title, detail, extraDetail, children }: { title: string; detail: string; extraDetail?: string; children: ReactNode }) {
  return (
    <div className="setting-row">
      <div className="setting-row-main">
        <strong>{title}</strong>
        <small>{detail}</small>
        {extraDetail ? <small>{extraDetail}</small> : null}
      </div>
      <div className="setting-row-action">{children}</div>
    </div>
  );
}

export function MetricCard({ label, value, tone }: { label: string; value: string | number; tone: "good" | "bad" | "muted" }) {
  return <article className={`metric-card ${tone}`}><span>{label}</span><strong>{value}</strong></article>;
}

export function PanelTitle({ title, subtitle }: { title: string; subtitle: string }) {
  return <div className="panel-title"><h2>{title}</h2><p>{subtitle}</p></div>;
}

type FormInputProps = ComponentPropsWithoutRef<"input"> & {
  label: string;
  fieldClassName?: string;
};

export function FormInput({ label, fieldClassName, className, ...props }: FormInputProps) {
  return (
    <label className={cx("form-field", fieldClassName)}>
      <span>{label}</span>
      <input className={cx("form-control", className)} {...props} />
    </label>
  );
}

type SelectChangeEvent = {
  currentTarget: {
    value: string;
  };
};

type SelectControlProps = {
  options: SelectOption[];
  value: string;
  onChange: (event: SelectChangeEvent) => void;
  className?: string;
  disabled?: boolean;
  label?: string;
  id?: string;
  "aria-label"?: string;
};

export function SelectControl({
  options,
  value,
  onChange,
  className,
  disabled,
  label,
  id,
  "aria-label": ariaLabel,
}: SelectControlProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLSpanElement>(null);
  const selectedOption = options.find((option) => option.value === value) ?? options[0];
  const menuId = id ? `${id}-menu` : undefined;
  const controlLabel = ariaLabel ?? label;

  useEffect(() => {
    if (!open) {
      return;
    }
    const handlePointerDown = (event: PointerEvent) => {
      if (rootRef.current && !rootRef.current.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
      }
    };
    window.addEventListener("pointerdown", handlePointerDown, true);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown, true);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  const selectOption = (option: SelectOption) => {
    if (option.disabled) {
      return;
    }
    if (option.value !== value) {
      onChange({ currentTarget: { value: option.value } });
    }
    setOpen(false);
  };

  return (
    <span className="select-shell" ref={rootRef}>
      <button
        type="button"
        id={id}
        className={cx("select-trigger", "form-control", open && "open", className)}
        role="combobox"
        aria-label={controlLabel}
        aria-expanded={open}
        aria-haspopup="listbox"
        aria-controls={menuId}
        data-value={selectedOption?.value ?? ""}
        disabled={disabled || options.length === 0}
        onClick={() => setOpen((current) => !current)}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown" || event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            setOpen(true);
          }
        }}
      >
        <span className="select-current">{selectedOption?.label ?? ""}</span>
        <ChevronDown className="select-icon" size={16} aria-hidden="true" />
      </button>
      {open && (
        <div className="select-menu" id={menuId} role="listbox" aria-label={controlLabel}>
          {options.map((option) => (
            <button
              key={option.value}
              type="button"
              className="select-option"
              role="option"
              aria-selected={option.value === selectedOption?.value}
              disabled={option.disabled}
              onClick={() => selectOption(option)}
            >
              {option.label}
            </button>
          ))}
        </div>
      )}
    </span>
  );
}

type FormSelectProps = SelectControlProps & {
  label: string;
  fieldClassName?: string;
};

export function FormSelect({ label, fieldClassName, ...props }: FormSelectProps) {
  return (
    <div className={cx("form-field", fieldClassName)}>
      <span>{label}</span>
      <SelectControl label={label} {...props} />
    </div>
  );
}

type FormCheckboxProps = Omit<ComponentPropsWithoutRef<"input">, "type"> & {
  label: string;
  fieldClassName?: string;
};

export function FormCheckbox({ label, fieldClassName, className, ...props }: FormCheckboxProps) {
  return (
    <label className={cx("check-row", fieldClassName)}>
      <input type="checkbox" className={cx("form-checkbox", className)} {...props} />
      <span>{label}</span>
    </label>
  );
}

export function DialogShell({
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
  const titleId = useId();

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
      <section className={cx("dialog-panel", size === "wide" && "dialog-panel-wide")} role="dialog" aria-modal="true" aria-labelledby={titleId}>
        <div className="dialog-heading">
          <div>
            <h2 id={titleId}>{title}</h2>
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

export function StatusPill({ status }: { status: RuntimeStatus }) {
  return <span className={`status-pill ${status.type}`}>{status.type === "failed" ? `失败: ${status.message}` : runtimeStatusLabels[status.type]}</span>;
}

export function StatusIcon({ ok }: { ok: boolean }) {
  return <div className={ok ? "status-icon ok" : "status-icon"}>{ok ? <CheckCircle2 size={26} /> : <XCircle size={26} />}</div>;
}

export function IconButton({ title, busy, danger, onClick, children }: { title: string; busy: boolean; danger?: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button type="button" className={danger ? "icon-button danger" : "icon-button"} title={title} disabled={busy} onClick={onClick}>
      {busy ? <Loader2 size={15} className="spin" /> : children}
    </button>
  );
}

export function EmptyState({ title, detail }: { title: string; detail: string }) {
  return <div className="empty-state"><FileText size={24} /><strong>{title}</strong><p>{detail}</p></div>;
}

export function LoadingRows() {
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

export function parseSystemProxyProfileDraft(draft: SystemProxyProfileDraft): SystemProxyProfileInput | string {
  const proxyPort = Number(draft.proxyPort);
  if (!draft.name.trim()) {
    return "配置名称不能为空。";
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

export function systemProxyProfileToDraft(profile: SystemProxyProfile): SystemProxyProfileDraft {
  return {
    name: profile.name,
    proxyHost: profile.proxyHost,
    proxyPort: String(profile.proxyPort),
    bypass: profile.bypass,
  };
}

export function ToastStack({ toasts }: { toasts: Toast[] }) {
  return <div className="toast-stack">{toasts.map((toast) => <div key={toast.id} className={`toast ${toast.kind}`}><strong>{toast.title}</strong>{toast.detail && <span>{toast.detail}</span>}</div>)}</div>;
}
