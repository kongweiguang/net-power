/**
 * @author kongweiguang
 * Workbench 服务、工具服务和 SSH 页面展示组件。
 */

import { useEffect, useRef, useState, type CSSProperties, type FormEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import {
  CheckCircle2,
  ClipboardList,
  Copy,
  Ellipsis,
  FileText,
  FolderOpen,
  Link2,
  Loader2,
  Maximize2,
  Pause,
  Pencil,
  Play,
  Plus,
  RotateCcw,
  Save,
  TerminalSquare,
  Trash2,
} from "lucide-react";
import type { ServiceKind, ServiceSummary, SshAuthType, SshProfile, SystemProxyStatus, ToolServiceSummary } from "../../types";
import {
  bindModeFromHost,
  bindModeOptionsForHost,
  defaultBodyRewriteRuleDraft,
  defaultHeaderRuleDraft,
  hostForBindMode,
  kindLabels,
  pageTitle,
  toolServiceContentSourceLabels,
  toolServiceMethodLabels,
  toolServiceStaticModeLabels,
  type BindMode,
  type PageKey,
  type ServiceDraft,
  type SshDraft,
  type ToolServiceDraft,
  type ToolServiceRouteDraft,
} from "./workbenchModel";
import {
  DialogShell,
  EmptyState,
  FormCheckbox,
  FormInput,
  FormSelect,
  IconButton,
  LoadingRows,
  MetricCard,
  PanelTitle,
  StatusPill,
} from "./WorkbenchPanels";
import {
  knownHostModeLabels,
  cx,
  newToolServiceRoute,
  normalizeDraftForPage,
  serviceKindsForPage,
  sshAuthLabels,
} from "./workbenchShared";

interface DashboardProps {
  loading: boolean;
  services: ServiceSummary[];
  runningCount: number;
  stoppedCount: number;
  failedCount: number;
  proxyStatus: SystemProxyStatus;
  busy: string | null;
  onAction: (id: string, action: "start" | "stop" | "restart" | "delete" | "duplicate" | "test") => void;
  onCopyAddress: (service: ServiceSummary) => void;
  onEdit: (id: string) => void;
  onLogs: (service: ServiceSummary) => void;
}

export function Dashboard({ loading, services, runningCount, stoppedCount, failedCount, proxyStatus, busy, onAction, onCopyAddress, onEdit, onLogs }: DashboardProps) {
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
        {loading ? <LoadingRows /> : <ServiceTable services={services} busy={busy} onAction={onAction} onCopyAddress={onCopyAddress} onEdit={onEdit} onLogs={onLogs} />}
      </section>
    </div>
  );
}

interface ToolServicesPageProps {
  services: ToolServiceSummary[];
  draft: ToolServiceDraft;
  runningCount: number;
  busy: string | null;
  dialogOpen: boolean;
  editingServiceId: string | null;
  onDraftChange: (draft: ToolServiceDraft) => void;
  onOpenCreate: () => void;
  onCloseDialog: () => void;
  onCreate: (event: FormEvent<HTMLFormElement>) => void;
  onToggle: (service: ToolServiceSummary) => void;
  onEdit: (id: string) => void;
  onDelete: (id: string) => void;
  onCopyAddress: (service: ToolServiceSummary) => void;
  onChooseDirectory: () => void;
  onChooseFile: (routeId: string) => void;
}

export function ToolServicesPage({
  services,
  draft,
  runningCount,
  busy,
  dialogOpen,
  editingServiceId,
  onDraftChange,
  onOpenCreate,
  onCloseDialog,
  onCreate,
  onToggle,
  onEdit,
  onDelete,
  onCopyAddress,
  onChooseDirectory,
  onChooseFile,
}: ToolServicesPageProps) {
  return (
    <div className="view-stack">
      <section className="panel service-list-panel">
        <div className="panel-heading">
          <PanelTitle title="服务列表" subtitle={`已保存 ${services.length} 个服务，当前运行 ${runningCount} 个。`} />
          <button type="button" className="primary-button" onClick={onOpenCreate}>
            <Plus size={16} />
            添加服务
          </button>
        </div>
        <ToolServiceList services={services} busy={busy} onToggle={onToggle} onEdit={onEdit} onDelete={onDelete} onCopyAddress={onCopyAddress} />
      </section>
      <DialogShell
        open={dialogOpen}
        title={editingServiceId ? "编辑服务配置" : "添加服务"}
        description="选择服务类型后配置基础信息；当前支持可挂载静态目录和接口响应的 HTTP。"
        onClose={onCloseDialog}
        size="wide"
      >
        <ToolServiceForm
          draft={draft}
          busy={busy}
          editing={Boolean(editingServiceId)}
          onDraftChange={onDraftChange}
          onSubmit={onCreate}
          onChooseDirectory={onChooseDirectory}
          onChooseFile={onChooseFile}
        />
      </DialogShell>
    </div>
  );
}

interface ToolServiceFormProps {
  draft: ToolServiceDraft;
  busy: string | null;
  editing: boolean;
  onDraftChange: (draft: ToolServiceDraft) => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onChooseDirectory: () => void;
  onChooseFile: (routeId: string) => void;
}

function ToolServiceForm({ draft, busy, editing, onDraftChange, onSubmit, onChooseDirectory, onChooseFile }: ToolServiceFormProps) {
  const [expandedBodyRouteId, setExpandedBodyRouteId] = useState<string | null>(null);

  const updateRoute = (routeId: string, patch: Partial<ToolServiceRouteDraft>) => {
    onDraftChange({
      ...draft,
      routes: draft.routes.map((route) => (route.id === routeId ? { ...route, ...patch } : route)),
    });
  };
  const addRoute = () => {
    const nextIndex = draft.routes.length + 1;
    onDraftChange({
      ...draft,
      routes: [...draft.routes, newToolServiceRoute(nextIndex)],
    });
  };
  const removeRoute = (routeId: string) => {
    if (expandedBodyRouteId === routeId) {
      setExpandedBodyRouteId(null);
    }
    onDraftChange({ ...draft, routes: draft.routes.filter((route) => route.id !== routeId) });
  };
  const expandedBodyRoute = draft.routes.find((route) => route.id === expandedBodyRouteId && route.contentSource !== "file") ?? null;

  return (
    <form className="form-grid" onSubmit={onSubmit}>
      <FormSelect
        label="服务类型"
        value="tool_http"
        disabled
        onChange={() => undefined}
        options={[{ value: "tool_http", label: "HTTP" }]}
      />
      <FormInput label="服务名称" value={draft.name} onChange={(event) => onDraftChange({ ...draft, name: event.currentTarget.value })} />
      <FormSelect
        label="启动范围"
        value={bindModeFromHost(draft.host)}
        onChange={(event) =>
          onDraftChange({ ...draft, host: hostForBindMode(event.currentTarget.value as BindMode, draft.host) })
        }
        options={bindModeOptionsForHost(draft.host)}
      />
      <FormInput label="端口" inputMode="numeric" value={draft.port} onChange={(event) => onDraftChange({ ...draft, port: event.currentTarget.value })} />
      <FormInput
        label="静态路径前缀"
        value={draft.staticPathPrefix}
        onChange={(event) => onDraftChange({ ...draft, staticPathPrefix: event.currentTarget.value })}
      />
      <FormSelect
        label="静态访问模式"
        value={draft.staticMode}
        onChange={(event) => onDraftChange({ ...draft, staticMode: event.currentTarget.value as ToolServiceDraft["staticMode"] })}
        options={Object.entries(toolServiceStaticModeLabels).map(([value, label]) => ({ value, label }))}
      />

      <div className="form-field span-2">
        <span>静态目录</span>
        <div className="folder-picker">
          <input
            className="form-control"
            aria-label="静态目录"
            value={draft.staticRootDir}
            readOnly
            placeholder="未挂载静态目录"
          />
          <button type="button" className="ghost-button" disabled={busy === "choose-tool-root"} onClick={onChooseDirectory}>
            {busy === "choose-tool-root" ? <Loader2 size={15} className="spin" /> : <FolderOpen size={15} />}
            选择
          </button>
          {draft.staticRootDir && (
            <button type="button" className="ghost-button" onClick={() => onDraftChange({ ...draft, staticRootDir: "" })}>
              清空
            </button>
          )}
        </div>
      </div>

      <section className="rule-editor span-2">
        <div className="rule-editor-heading">
          <PanelTitle title="接口路由" subtitle="按顺序精确匹配请求路径，未命中时继续尝试静态目录。" />
          <button type="button" className="ghost-button" onClick={addRoute}>
            <Plus size={15} />
            添加接口
          </button>
        </div>
        {draft.routes.length === 0 ? (
          <p className="rule-empty">暂无接口路由，仅挂载静态目录。</p>
        ) : (
          draft.routes.map((route, index) => (
            <div className="rule-row tool-route-row" key={route.id}>
              <FormSelect
                label={`方法 #${index + 1}`}
                value={route.method}
                onChange={(event) => updateRoute(route.id, { method: event.currentTarget.value as ToolServiceRouteDraft["method"] })}
                options={Object.entries(toolServiceMethodLabels).map(([value, label]) => ({ value, label }))}
              />
              <FormInput label={`路径 #${index + 1}`} value={route.path} onChange={(event) => updateRoute(route.id, { path: event.currentTarget.value })} />
              <FormInput
                label="状态码"
                inputMode="numeric"
                value={route.responseStatus}
                onChange={(event) => updateRoute(route.id, { responseStatus: event.currentTarget.value })}
              />
              <FormSelect
                label="响应来源"
                value={route.contentSource}
                onChange={(event) => {
                  const contentSource = event.currentTarget.value as ToolServiceRouteDraft["contentSource"];
                  if (contentSource === "file" && expandedBodyRouteId === route.id) {
                    setExpandedBodyRouteId(null);
                  }
                  updateRoute(route.id, { contentSource });
                }}
                options={Object.entries(toolServiceContentSourceLabels).map(([value, label]) => ({ value, label }))}
              />
              <FormInput
                label="Content-Type"
                value={route.contentType}
                onChange={(event) => updateRoute(route.id, { contentType: event.currentTarget.value })}
              />
              {route.contentSource === "file" ? (
                <div className="form-field">
                  <span>响应文件</span>
                  <div className="folder-picker">
                    <input
                      className="form-control"
                      aria-label={`接口 ${index + 1} 响应文件`}
                      value={route.filePath}
                      readOnly
                      placeholder="请选择本地文件"
                    />
                    <button type="button" className="ghost-button" disabled={busy === `choose-tool-file:${route.id}`} onClick={() => onChooseFile(route.id)}>
                      {busy === `choose-tool-file:${route.id}` ? <Loader2 size={15} className="spin" /> : <FileText size={15} />}
                      选择
                    </button>
                  </div>
                </div>
              ) : (
                <ToolRouteBodyField
                  index={index}
                  route={route}
                  onBodyChange={(body) => updateRoute(route.id, { body })}
                  onExpand={() => setExpandedBodyRouteId(route.id)}
                />
              )}
              <button type="button" className="icon-button danger" title="删除接口" onClick={() => removeRoute(route.id)}>
                <Trash2 size={15} />
              </button>
            </div>
          ))
        )}
      </section>
      <ToolRouteBodyDialog
        route={expandedBodyRoute}
        onBodyChange={(body) => {
          if (expandedBodyRoute) {
            updateRoute(expandedBodyRoute.id, { body });
          }
        }}
        onClose={() => setExpandedBodyRouteId(null)}
      />

      <div className="button-row span-2">
        <button type="submit" className="primary-button" disabled={busy === "save-tool-service"}>
          {busy === "save-tool-service" ? <Loader2 className="spin" size={16} /> : editing ? <Save size={16} /> : <Plus size={16} />}
          {editing ? "保存配置" : "创建并启动"}
        </button>
      </div>
    </form>
  );
}

function ToolRouteBodyField({
  route,
  index,
  onBodyChange,
  onExpand,
}: {
  route: ToolServiceRouteDraft;
  index: number;
  onBodyChange: (body: string) => void;
  onExpand: () => void;
}) {
  const label = `接口 ${index + 1} 响应内容`;
  return (
    <div className="form-field response-body-field">
      <span>响应内容</span>
      <div className="response-body-control">
        <textarea
          className="response-body-textarea"
          aria-label={label}
          value={route.body}
          onChange={(event) => onBodyChange(event.currentTarget.value)}
        />
        <button type="button" className="icon-button response-body-expand" title={`放大编辑${label}`} aria-label={`放大编辑${label}`} onClick={onExpand}>
          <Maximize2 size={15} />
        </button>
      </div>
    </div>
  );
}

function ToolRouteBodyDialog({
  route,
  onBodyChange,
  onClose,
}: {
  route: ToolServiceRouteDraft | null;
  onBodyChange: (body: string) => void;
  onClose: () => void;
}) {
  return (
    <DialogShell
      open={Boolean(route)}
      title="编辑响应内容"
      description={route ? `${route.method} ${route.path} · ${route.contentType}` : "编辑接口响应内容。"}
      onClose={onClose}
    >
      {route && (
        <div className="response-body-dialog">
          <textarea
            className="response-body-large"
            aria-label="放大响应内容"
            autoFocus
            value={route.body}
            onChange={(event) => onBodyChange(event.currentTarget.value)}
          />
          <div className="button-row">
            <button type="button" className="primary-button" onClick={onClose}>
              完成
            </button>
          </div>
        </div>
      )}
    </DialogShell>
  );
}

interface MoreActionItem {
  title: string;
  icon: ReactNode;
  onClick: () => void;
  busy?: boolean;
  danger?: boolean;
}

const moreActionMenuWidth = 184;
const moreActionMenuPadding = 12;
const moreActionMenuItemHeight = 36;

function MoreActionMenu({ actions }: { actions: MoreActionItem[] }) {
  const [open, setOpen] = useState(false);
  const [menuStyle, setMenuStyle] = useState<CSSProperties>({ left: 0, top: 0 });
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  const placeMenu = () => {
    const trigger = triggerRef.current;
    if (!trigger) {
      return;
    }
    const rect = trigger.getBoundingClientRect();
    const estimatedHeight = moreActionMenuPadding + actions.length * moreActionMenuItemHeight;
    const topBelow = rect.bottom + 6;
    const top = topBelow + estimatedHeight > window.innerHeight
      ? Math.max(8, rect.top - estimatedHeight - 6)
      : topBelow;
    const left = Math.max(8, Math.min(rect.right - moreActionMenuWidth, window.innerWidth - moreActionMenuWidth - 8));
    setMenuStyle({ left, top, width: moreActionMenuWidth });
  };

  useEffect(() => {
    if (!open) {
      return;
    }
    placeMenu();
    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (triggerRef.current?.contains(target) || menuRef.current?.contains(target)) {
        return;
      }
      setOpen(false);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
      }
    };
    window.addEventListener("pointerdown", handlePointerDown, true);
    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("resize", placeMenu);
    window.addEventListener("scroll", placeMenu, true);
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown, true);
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("resize", placeMenu);
      window.removeEventListener("scroll", placeMenu, true);
    };
  }, [actions.length, open]);

  const menu = open
    ? createPortal(
        <div className="action-menu-popover" ref={menuRef} role="menu" aria-label="更多操作" style={menuStyle}>
          {actions.map((action) => (
            <button
              key={action.title}
              type="button"
              className={cx("action-menu-item", action.danger && "danger")}
              role="menuitem"
              title={action.title}
              disabled={action.busy}
              onClick={() => {
                setOpen(false);
                action.onClick();
              }}
            >
              {action.busy ? <Loader2 size={15} className="spin" /> : action.icon}
              <span>{action.title}</span>
            </button>
          ))}
        </div>,
        document.body,
      )
    : null;

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        className="icon-button action-menu-trigger"
        title="更多操作"
        aria-label="更多操作"
        aria-expanded={open}
        aria-haspopup="menu"
        onClick={() => {
          if (!open) {
            placeMenu();
          }
          setOpen((current) => !current);
        }}
      >
        <Ellipsis size={16} />
      </button>
      {menu}
    </>
  );
}

function ToolServiceList({
  services,
  busy,
  onToggle,
  onEdit,
  onDelete,
  onCopyAddress,
}: {
  services: ToolServiceSummary[];
  busy: string | null;
  onToggle: (service: ToolServiceSummary) => void;
  onEdit: (id: string) => void;
  onDelete: (id: string) => void;
  onCopyAddress: (service: ToolServiceSummary) => void;
}) {
  if (services.length === 0) {
    return <EmptyState title="暂无服务" detail="创建后会显示类型、监听地址、资源和启停入口。" />;
  }
  return (
    <div className="compact-list">
      {services.map((service) => {
        const running = service.runtimeStatus.type === "running" || service.runtimeStatus.type === "starting";
        const toggleBusy = busy === `${running ? "stop" : "start"}-tool:${service.id}`;
        const staticLabel = service.staticRootDir ?? "无静态目录";
        const staticDetail = service.staticRootDir
          ? `${toolServiceStaticModeLabels[service.staticMode]} · 挂载 ${service.staticPathPrefix} · ${service.routeCount} 接口 · ${service.totalRequests} 请求`
          : `${service.routeCount} 接口 · ${service.totalRequests} 请求`;
        return (
          <div className="compact-row tool-service-row" key={service.id}>
            <div className="tool-service-summary">
              <div className="tool-service-cell tool-service-name">
                <span className="tool-service-label">名称</span>
                <strong>{service.name}</strong>
              </div>
              <div className="tool-service-cell tool-service-identity">
                <span className="tool-service-label">服务</span>
                <strong>HTTP</strong>
              </div>
              <div className="tool-service-cell">
                <span className="tool-service-label">监听</span>
                <span className="tool-service-value">{service.host}:{service.port}</span>
              </div>
              <div className="tool-service-cell">
                <span className="tool-service-label">资源</span>
                <span className="tool-service-value" title={staticLabel}>{staticLabel}</span>
                <small>{staticDetail}</small>
              </div>
            </div>
            <div className="icon-row tool-service-actions action-row">
              <StatusPill status={service.runtimeStatus} />
              <IconButton title="复制地址" busy={false} onClick={() => onCopyAddress(service)}>
                <Link2 size={15} />
              </IconButton>
              <IconButton title={running ? "暂停服务" : "启动服务"} busy={toggleBusy} onClick={() => onToggle(service)}>
                {running ? <Pause size={15} /> : <Play size={15} />}
              </IconButton>
              <IconButton title="编辑配置" busy={busy === `edit-tool:${service.id}`} onClick={() => onEdit(service.id)}>
                <Pencil size={15} />
              </IconButton>
              <MoreActionMenu
                actions={[
                  {
                    title: "删除服务",
                    icon: <Trash2 size={15} />,
                    danger: true,
                    busy: busy === `delete-tool:${service.id}`,
                    onClick: () => onDelete(service.id),
                  },
                ]}
              />
            </div>
          </div>
        );
      })}
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
  onCopyAddress: (service: ServiceSummary) => void;
  onEdit: (id: string) => void;
  onLogs: (service: ServiceSummary) => void;
}

export function ServicePage({ page, services, draft, busy, profiles, editingServiceId, serviceDialogOpen, onDraftChange, onSubmit, onOpenCreate, onCancelEdit, onAction, onCopyAddress, onEdit, onLogs }: ServicePageProps) {
  const allowedKinds = serviceKindsForPage(page) ?? ["http_reverse"];
  const currentDraft = normalizeDraftForPage(page, draft);
  return (
    <div className="view-stack">
      <section className="panel service-list-panel">
        <div className="panel-heading">
          <PanelTitle title={`${pageTitle(page)}配置列表`} subtitle="统一管理 HTTP 反向代理、HTTP 正向代理、TCP 转发和 UDP 转发。" />
          <button type="button" className="primary-button" onClick={onOpenCreate}>
            <Plus size={16} />
            添加配置
          </button>
        </div>
        <ServiceTable services={services} busy={busy} onAction={onAction} onCopyAddress={onCopyAddress} onEdit={onEdit} onLogs={onLogs} />
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
  onServiceCopyAddress: (service: ServiceSummary) => void;
  onServiceEdit: (id: string) => void;
  onServiceLogs: (service: ServiceSummary) => void;
  onSshAction: (id: string, action: "test" | "delete" | "edit" | "terminal") => void;
  onChoosePrivateKey: () => void;
  onChooseKnownHosts: () => void;
}

export function SshPage({ services, profiles, serviceDraft, sshDraft, busy, sshProfileDialogOpen, serviceDialogOpen, editingServiceId, editingSshProfileId, onServiceDraftChange, onSshDraftChange, onOpenCreateService, onOpenCreateProfile, onCreateService, onCreateProfile, onCancelServiceEdit, onCancelProfileEdit, onServiceAction, onServiceCopyAddress, onServiceEdit, onServiceLogs, onSshAction, onChoosePrivateKey, onChooseKnownHosts }: SshPageProps) {
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
        <SshProfileForm profiles={profiles} editingProfileId={editingSshProfileId} draft={sshDraft} busy={busy} editing={Boolean(editingSshProfileId)} onDraftChange={onSshDraftChange} onSubmit={onCreateProfile} onCancelEdit={onCancelProfileEdit} onChoosePrivateKey={onChoosePrivateKey} onChooseKnownHosts={onChooseKnownHosts} />
      </DialogShell>
      <section className="panel ssh-tunnel-panel">
        <div className="panel-heading">
          <PanelTitle title="SSH 隧道配置列表" subtitle="支持本地端口转发、远程端口转发和 SOCKS5 动态代理。" />
          <button type="button" className="primary-button" onClick={onOpenCreateService}>
            <Plus size={16} />
            添加 SSH 隧道
          </button>
        </div>
        <ServiceTable services={services} busy={busy} onAction={onServiceAction} onCopyAddress={onServiceCopyAddress} onEdit={onServiceEdit} onLogs={onServiceLogs} />
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
      {isSshRemote ? (
        <FormInput label="远程绑定主机" value={draft.listenHost} onChange={(event) => onDraftChange({ ...draft, listenHost: event.currentTarget.value })} />
      ) : (
        <FormSelect
          label="启动范围"
          value={bindModeFromHost(draft.listenHost)}
          onChange={(event) =>
            onDraftChange({ ...draft, listenHost: hostForBindMode(event.currentTarget.value as BindMode, draft.listenHost) })
          }
          options={bindModeOptionsForHost(draft.listenHost)}
        />
      )}
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
          {busy === "save-service" ? <Loader2 className="spin" size={16} /> : editing ? <Save size={16} /> : <Plus size={16} />}
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
  onCopyAddress: (service: ServiceSummary) => void;
  onEdit?: (id: string) => void;
  onLogs: (service: ServiceSummary) => void;
}

function ServiceTable({ services, busy, onAction, onCopyAddress, onEdit, onLogs }: ServiceTableProps) {
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
          {services.map((service) => {
            const running = service.runtimeStatus.type === "running" || service.runtimeStatus.type === "starting";
            const toggleAction = running ? "stop" : "start";
            return (
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
                  <div className="icon-row action-row">
                    <IconButton title={running ? "暂停" : "启动"} busy={busy === `${toggleAction}:${service.id}`} onClick={() => onAction(service.id, toggleAction)}>
                      {running ? <Pause size={15} /> : <Play size={15} />}
                    </IconButton>
                    <IconButton title="复制地址" busy={false} onClick={() => onCopyAddress(service)}><Link2 size={15} /></IconButton>
                    {onEdit && <IconButton title="编辑" busy={busy === `edit:${service.id}`} onClick={() => onEdit(service.id)}><Pencil size={15} /></IconButton>}
                    <MoreActionMenu
                      actions={[
                        {
                          title: "重启",
                          icon: <RotateCcw size={15} />,
                          busy: busy === `restart:${service.id}`,
                          onClick: () => onAction(service.id, "restart"),
                        },
                        {
                          title: "测试",
                          icon: <CheckCircle2 size={15} />,
                          busy: busy === `test:${service.id}`,
                          onClick: () => onAction(service.id, "test"),
                        },
                        {
                          title: "日志",
                          icon: <ClipboardList size={15} />,
                          busy: busy === `logs:${service.id}`,
                          onClick: () => onLogs(service),
                        },
                        {
                          title: "复制配置",
                          icon: <Copy size={15} />,
                          busy: busy === `duplicate:${service.id}`,
                          onClick: () => onAction(service.id, "duplicate"),
                        },
                        {
                          title: "删除",
                          icon: <Trash2 size={15} />,
                          danger: true,
                          busy: busy === `delete:${service.id}`,
                          onClick: () => onAction(service.id, "delete"),
                        },
                      ]}
                    />
                  </div>
                </td>
              </tr>
            );
          })}
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
  onChoosePrivateKey: () => void;
  onChooseKnownHosts: () => void;
}

function SshProfileForm({ profiles, editingProfileId, draft, busy, editing, onDraftChange, onSubmit, onCancelEdit, onChoosePrivateKey, onChooseKnownHosts }: SshProfileFormProps) {
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
          <div className="form-field span-2">
            <span>私钥路径</span>
            <div className="folder-picker">
              <input
                className="form-control"
                aria-label="私钥路径"
                value={draft.privateKeyPath}
                readOnly
                placeholder="请选择私钥文件"
              />
              <button type="button" className="ghost-button" disabled={busy === "choose-ssh-private-key"} onClick={onChoosePrivateKey}>
                {busy === "choose-ssh-private-key" ? <Loader2 size={15} className="spin" /> : <FileText size={15} />}
                选择
              </button>
              {draft.privateKeyPath && (
                <button type="button" className="ghost-button" onClick={() => onDraftChange({ ...draft, privateKeyPath: "" })}>
                  清空
                </button>
              )}
            </div>
          </div>
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
      <div className="form-field span-2">
        <span>known_hosts 路径</span>
        <div className="folder-picker">
          <input
            className="form-control"
            aria-label="known_hosts 路径"
            value={draft.knownHostsPath}
            readOnly
            placeholder="默认使用用户 .ssh/known_hosts"
          />
          <button type="button" className="ghost-button" disabled={busy === "choose-ssh-known-hosts"} onClick={onChooseKnownHosts}>
            {busy === "choose-ssh-known-hosts" ? <Loader2 size={15} className="spin" /> : <FileText size={15} />}
            选择
          </button>
          {draft.knownHostsPath && (
            <button type="button" className="ghost-button" onClick={() => onDraftChange({ ...draft, knownHostsPath: "" })}>
              清空
            </button>
          )}
        </div>
      </div>
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
          {busy === "save-ssh" ? <Loader2 className="spin" size={16} /> : editing ? <Save size={16} /> : <Plus size={16} />}
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
  onAction: (id: string, action: "test" | "delete" | "edit" | "terminal") => void;
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
          <div className="icon-row ssh-profile-actions action-row">
            <IconButton title="测试 SSH" busy={busy === `test-ssh:${profile.id}`} onClick={() => onAction(profile.id, "test")}><CheckCircle2 size={15} /></IconButton>
            <IconButton title="打开终端" busy={busy === `terminal-ssh:${profile.id}`} onClick={() => onAction(profile.id, "terminal")}><TerminalSquare size={15} /></IconButton>
            <IconButton title="编辑 SSH" busy={busy === `edit-ssh:${profile.id}`} onClick={() => onAction(profile.id, "edit")}><Pencil size={15} /></IconButton>
            <MoreActionMenu
              actions={[
                {
                  title: "删除 SSH",
                  icon: <Trash2 size={15} />,
                  danger: true,
                  busy: busy === `delete-ssh:${profile.id}`,
                  onClick: () => onAction(profile.id, "delete"),
                },
              ]}
            />
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
