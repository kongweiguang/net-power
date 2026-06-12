/**
 * @author kongweiguang
 * Workbench 组件级测试。通过用户可见行为保护服务编辑、SSH secret、日志筛选和系统代理入口。
 */

import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import packageJson from "../../../package.json";
import type {
  AppSetting,
  AutostartStatus,
  LogRow,
  ServiceDetail,
  ServiceRuntimeSummary,
  ServiceSummary,
  SshProfile,
  SystemProxyProfile,
  SystemProxyStatus,
  ToolServiceConfig,
  ToolServiceSummary,
} from "../../types";
import { Workbench } from "./Workbench";

const appVersion = packageJson.version;

const apiMocks = vi.hoisted(() => ({
  settingsList: vi.fn(),
  settingsUpdate: vi.fn(),
  autostartGet: vi.fn(),
  autostartSet: vi.fn(),
  servicesList: vi.fn(),
  servicesGet: vi.fn(),
  servicesCreate: vi.fn(),
  servicesUpdate: vi.fn(),
  servicesDelete: vi.fn(),
  servicesDuplicate: vi.fn(),
  servicesStart: vi.fn(),
  servicesStop: vi.fn(),
  servicesRestart: vi.fn(),
  servicesRuntime: vi.fn(),
  servicesTest: vi.fn(),
  toolServicesList: vi.fn(),
  toolServicesGet: vi.fn(),
  toolServicesCreate: vi.fn(),
  toolServicesUpdate: vi.fn(),
  toolServicesStart: vi.fn(),
  toolServicesStop: vi.fn(),
  toolServicesDelete: vi.fn(),
  fileDialogChooseDirectory: vi.fn(),
  fileDialogChooseFile: vi.fn(),
  sshList: vi.fn(),
  sshGet: vi.fn(),
  sshCreate: vi.fn(),
  sshUpdate: vi.fn(),
  sshDelete: vi.fn(),
  sshTest: vi.fn(),
  logsList: vi.fn(),
  logsClear: vi.fn(),
  networkGetLanIp: vi.fn(),
  proxySet: vi.fn(),
  proxySetTarget: vi.fn(),
  proxyProfilesList: vi.fn(),
  proxyProfileCreate: vi.fn(),
  proxyProfileUpdate: vi.fn(),
  proxyProfileDelete: vi.fn(),
  proxyClear: vi.fn(),
  proxyStatus: vi.fn(),
  updaterCheckDownloadInstall: vi.fn(),
}));

vi.mock("../../api", () => ({
  isTauriRuntime: () => false,
  settingsApi: {
    list: apiMocks.settingsList,
    update: apiMocks.settingsUpdate,
  },
  autostartApi: {
    getStatus: apiMocks.autostartGet,
    setEnabled: apiMocks.autostartSet,
  },
  servicesApi: {
    list: apiMocks.servicesList,
    get: apiMocks.servicesGet,
    create: apiMocks.servicesCreate,
    update: apiMocks.servicesUpdate,
    delete: apiMocks.servicesDelete,
    duplicate: apiMocks.servicesDuplicate,
    start: apiMocks.servicesStart,
    stop: apiMocks.servicesStop,
    restart: apiMocks.servicesRestart,
    listRuntimeStatus: apiMocks.servicesRuntime,
    test: apiMocks.servicesTest,
  },
  toolServicesApi: {
    list: apiMocks.toolServicesList,
    get: apiMocks.toolServicesGet,
    create: apiMocks.toolServicesCreate,
    update: apiMocks.toolServicesUpdate,
    start: apiMocks.toolServicesStart,
    stop: apiMocks.toolServicesStop,
    delete: apiMocks.toolServicesDelete,
    chooseDirectory: apiMocks.fileDialogChooseDirectory,
    chooseFile: apiMocks.fileDialogChooseFile,
  },
  fileDialogApi: {
    chooseDirectory: apiMocks.fileDialogChooseDirectory,
    chooseFile: apiMocks.fileDialogChooseFile,
  },
  sshProfilesApi: {
    list: apiMocks.sshList,
    get: apiMocks.sshGet,
    create: apiMocks.sshCreate,
    update: apiMocks.sshUpdate,
    delete: apiMocks.sshDelete,
    test: apiMocks.sshTest,
  },
  logsApi: {
    list: apiMocks.logsList,
    clear: apiMocks.logsClear,
  },
  networkApi: {
    getLanIp: apiMocks.networkGetLanIp,
  },
  systemProxyApi: {
    listProfiles: apiMocks.proxyProfilesList,
    createProfile: apiMocks.proxyProfileCreate,
    updateProfile: apiMocks.proxyProfileUpdate,
    deleteProfile: apiMocks.proxyProfileDelete,
    set: apiMocks.proxySet,
    setTarget: apiMocks.proxySetTarget,
    clear: apiMocks.proxyClear,
    getStatus: apiMocks.proxyStatus,
  },
  updaterApi: {
    checkDownloadInstall: apiMocks.updaterCheckDownloadInstall,
  },
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

describe("Workbench", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    document.documentElement.removeAttribute("data-theme");
    document.documentElement.removeAttribute("data-theme-mode");
    document.documentElement.style.colorScheme = "";
    mockApis();
  });

  it("submits a selected TCP service kind from the unified forwarding page", async () => {
    const user = userEvent.setup();
    render(<Workbench />);

    await screen.findByRole("heading", { name: "仪表盘" });
    expect(screen.queryByRole("button", { name: "HTTP" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "网络转发" }));
    await user.click(screen.getByRole("button", { name: "添加配置" }));

    const panel = dialogByHeading("添加 网络转发 配置");
    expect(within(panel).getByRole("combobox", { name: "服务类型" })).toHaveAttribute("data-value", "http_reverse");
    await chooseComboboxOption(user, panel, "服务类型", "TCP 转发");
    await chooseComboboxOption(user, panel, "启动范围", "局域网 (0.0.0.0)");
    await user.type(within(panel).getByLabelText("服务名称"), "Database TCP");
    await user.click(within(panel).getByRole("button", { name: /创建服务/ }));

    await waitFor(() => expect(apiMocks.servicesCreate).toHaveBeenCalledTimes(1));
    expect(apiMocks.servicesCreate).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "Database TCP",
        kind: "tcp_forward",
        listenHost: "0.0.0.0",
        tcpForward: expect.objectContaining({
          targetHost: "127.0.0.1",
          targetPort: 8080,
        }),
      }),
    );
  });

  it("loads a service into edit mode and saves changes through the update API", async () => {
    const user = userEvent.setup();
    const service = serviceSummary({ id: "svc-http", name: "Reverse" });
    mockApis({ services: [service] });
    apiMocks.servicesGet.mockResolvedValue(serviceDetail({ id: "svc-http", name: "Reverse" }));

    render(<Workbench />);

    await screen.findByText("Reverse");
    await user.click(screen.getByTitle("编辑"));

    const panel = dialogByHeading("编辑服务配置");
    expect(within(panel).getByLabelText("服务名称")).toHaveValue("Reverse");
    await user.clear(within(panel).getByLabelText("服务名称"));
    await user.type(within(panel).getByLabelText("服务名称"), "Reverse API");
    await user.click(within(panel).getByRole("button", { name: /保存服务/ }));

    await waitFor(() => expect(apiMocks.servicesUpdate).toHaveBeenCalledTimes(1));
    expect(apiMocks.servicesUpdate).toHaveBeenCalledWith(
      "svc-http",
      expect.objectContaining({
        name: "Reverse API",
        kind: "http_reverse",
      }),
    );
  });

  it("edits an SSH profile without leaking the existing password back into the form", async () => {
    const user = userEvent.setup();
    mockApis({ profiles: [sshProfile()] });

    render(<Workbench />);

    await screen.findByRole("heading", { name: "仪表盘" });
    await user.click(screen.getByRole("button", { name: "SSH" }));
    await user.click(await screen.findByTitle("编辑 SSH"));

    await waitFor(() => expect(apiMocks.sshGet).toHaveBeenCalledWith("ssh-1"));
    const panel = dialogByHeading("编辑 SSH 配置");
    expect(within(panel).getByLabelText("密码")).toHaveValue("");
    await user.click(within(panel).getByRole("button", { name: /保存 SSH 配置/ }));

    await waitFor(() => expect(apiMocks.sshUpdate).toHaveBeenCalledTimes(1));
    expect(apiMocks.sshUpdate).toHaveBeenCalledWith(
      "ssh-1",
      expect.objectContaining({
        name: "Prod SSH",
        password: null,
      }),
    );
  });

  it("creates an SSH profile that uses another profile as jump host", async () => {
    const user = userEvent.setup();
    mockApis({
      profiles: [
        sshProfile({
          id: "jump-1",
          name: "堡垒机",
          host: "bastion.example.com",
        }),
      ],
    });

    render(<Workbench />);

    await screen.findByRole("heading", { name: "仪表盘" });
    await user.click(screen.getByRole("button", { name: "SSH" }));
    await user.click(screen.getByRole("button", { name: /添加 SSH 配置/ }));

    const panel = dialogByHeading("添加 SSH 配置");
    await user.type(within(panel).getByLabelText("名称"), "内网主机");
    await user.type(within(panel).getByLabelText("主机"), "app.internal");
    await user.type(within(panel).getByLabelText("用户名"), "deploy");
    await user.type(within(panel).getByLabelText("密码"), "secret");
    await chooseComboboxOption(user, panel, "跳板配置", "堡垒机 (deploy@bastion.example.com:22)");
    await user.click(within(panel).getByRole("button", { name: /添加 SSH 配置/ }));

    await waitFor(() => expect(apiMocks.sshCreate).toHaveBeenCalledTimes(1));
    expect(apiMocks.sshCreate).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "内网主机",
        host: "app.internal",
        username: "deploy",
        password: "secret",
        jumpProfileId: "jump-1",
      }),
    );
  });

  it("uses file pickers for SSH private key and known_hosts paths", async () => {
    const user = userEvent.setup();
    render(<Workbench />);

    await screen.findByRole("heading", { name: "仪表盘" });
    await user.click(screen.getByRole("button", { name: "SSH" }));
    await user.click(screen.getByRole("button", { name: /添加 SSH 配置/ }));

    const panel = dialogByHeading("添加 SSH 配置");
    await chooseComboboxOption(user, panel, "认证方式", "私钥");

    const privateKeyInput = within(panel).getByLabelText("私钥路径");
    expect(privateKeyInput).toHaveAttribute("readonly");
    const privateKeyPicker = privateKeyInput.closest(".folder-picker");
    if (!privateKeyPicker) {
      throw new Error("找不到私钥选择器");
    }
    await user.click(within(privateKeyPicker as HTMLElement).getByRole("button", { name: "选择" }));
    await waitFor(() => expect(privateKeyInput).toHaveValue("C:/fixtures/response.json"));

    const knownHostsInput = within(panel).getByLabelText("known_hosts 路径");
    expect(knownHostsInput).toHaveAttribute("readonly");
    const knownHostsPicker = knownHostsInput.closest(".folder-picker");
    if (!knownHostsPicker) {
      throw new Error("找不到 known_hosts 选择器");
    }
    await user.click(within(knownHostsPicker as HTMLElement).getByRole("button", { name: "选择" }));
    await waitFor(() => expect(apiMocks.fileDialogChooseFile).toHaveBeenCalledTimes(2));
  });

  it("creates an SSH SOCKS5 service without target host and port", async () => {
    const user = userEvent.setup();
    mockApis({ profiles: [sshProfile()] });

    render(<Workbench />);

    await screen.findByRole("heading", { name: "仪表盘" });
    await user.click(screen.getByRole("button", { name: "SSH" }));
    await user.click(screen.getByRole("button", { name: "添加 SSH 隧道" }));

    const panel = dialogByHeading("添加 SSH 隧道配置");
    await chooseComboboxOption(user, panel, "服务类型", "SSH SOCKS5 动态代理");
    expect(within(panel).queryByLabelText("目标主机")).not.toBeInTheDocument();
    expect(within(panel).queryByLabelText("目标端口")).not.toBeInTheDocument();
    await user.type(within(panel).getByLabelText("服务名称"), "Dev SOCKS");
    await chooseComboboxOption(user, panel, "SSH 配置", "Prod SSH");
    await user.click(within(panel).getByRole("button", { name: /创建服务/ }));

    await waitFor(() => expect(apiMocks.servicesCreate).toHaveBeenCalledTimes(1));
    expect(apiMocks.servicesCreate).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "Dev SOCKS",
        kind: "ssh_socks",
        sshTunnel: {
          sshProfileId: "ssh-1",
          tunnelType: "socks",
          targetHost: null,
          targetPort: null,
          remoteBindHost: null,
          remoteBindPort: null,
        },
      }),
    );
  });

  it("creates an SSH remote service with remote bind and local target", async () => {
    const user = userEvent.setup();
    mockApis({ profiles: [sshProfile()] });

    render(<Workbench />);

    await screen.findByRole("heading", { name: "仪表盘" });
    await user.click(screen.getByRole("button", { name: "SSH" }));
    await user.click(screen.getByRole("button", { name: "添加 SSH 隧道" }));

    const panel = dialogByHeading("添加 SSH 隧道配置");
    await chooseComboboxOption(user, panel, "服务类型", "SSH 远程隧道");
    expect(within(panel).getByLabelText("远程绑定主机")).toBeInTheDocument();
    expect(within(panel).getByLabelText("远程绑定端口")).toBeInTheDocument();
    expect(within(panel).getByLabelText("本地目标主机")).toBeInTheDocument();
    expect(within(panel).getByLabelText("本地目标端口")).toBeInTheDocument();

    await user.type(within(panel).getByLabelText("服务名称"), "Remote Web");
    await chooseComboboxOption(user, panel, "SSH 配置", "Prod SSH");
    await user.clear(within(panel).getByLabelText("远程绑定主机"));
    await user.type(within(panel).getByLabelText("远程绑定主机"), "0.0.0.0");
    await user.clear(within(panel).getByLabelText("远程绑定端口"));
    await user.type(within(panel).getByLabelText("远程绑定端口"), "18080");
    await user.clear(within(panel).getByLabelText("本地目标主机"));
    await user.type(within(panel).getByLabelText("本地目标主机"), "127.0.0.1");
    await user.clear(within(panel).getByLabelText("本地目标端口"));
    await user.type(within(panel).getByLabelText("本地目标端口"), "8080");
    await user.click(within(panel).getByRole("button", { name: /创建服务/ }));

    await waitFor(() => expect(apiMocks.servicesCreate).toHaveBeenCalledTimes(1));
    expect(apiMocks.servicesCreate).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "Remote Web",
        kind: "ssh_remote",
        listenHost: "0.0.0.0",
        listenPort: 18080,
        sshTunnel: {
          sshProfileId: "ssh-1",
          tunnelType: "remote",
          targetHost: "127.0.0.1",
          targetPort: 8080,
          remoteBindHost: "0.0.0.0",
          remoteBindPort: 18080,
        },
      }),
    );
  });

  it("creates and starts a persisted local HTTP service from the service list dialog", async () => {
    const user = userEvent.setup();
    render(<Workbench />);

    await screen.findByRole("heading", { name: "仪表盘" });
    await user.click(screen.getByRole("button", { name: "本地服务" }));
    await user.click(screen.getByRole("button", { name: "添加服务" }));
    const panel = dialogByHeading("添加服务");
    expect(within(panel).getByRole("combobox", { name: "服务类型" })).toHaveAttribute("data-value", "tool_http");
    await user.clear(within(panel).getByLabelText("服务名称"));
    await user.type(within(panel).getByLabelText("服务名称"), "Local API");
    await user.clear(within(panel).getByLabelText("端口"));
    await user.type(within(panel).getByLabelText("端口"), "18081");
    await user.clear(within(panel).getByLabelText("路径 #1"));
    await user.type(within(panel).getByLabelText("路径 #1"), "/api/ping");
    await user.click(within(panel).getByRole("button", { name: "创建并启动" }));

    await waitFor(() => expect(apiMocks.toolServicesCreate).toHaveBeenCalledTimes(1));
    expect(apiMocks.toolServicesCreate).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "Local API",
        host: "127.0.0.1",
        port: 18081,
        staticRootDir: null,
        staticMode: "directory",
        staticPathPrefix: "/",
        routes: [
          expect.objectContaining({
            method: "GET",
            path: "/api/ping",
            responseStatus: 200,
            contentSource: "inline",
          }),
        ],
      }),
    );
  });

  it("loads a persisted HTTP tool service into edit mode and saves config changes", async () => {
    const user = userEvent.setup();
    mockApis({
      toolServices: [
        toolServiceSummary({
          id: "tool-edit",
          name: "Local API",
          staticRootDir: "C:\\dev\\rust\\net-power\\net-power\\src-tauri",
          staticPathPrefix: "/",
          runtimeStatus: { type: "stopped" },
        }),
      ],
    });
    apiMocks.toolServicesGet.mockResolvedValue(toolServiceConfig({ id: "tool-edit", name: "Local API" }));

    render(<Workbench />);

    await screen.findByRole("heading", { name: "仪表盘" });
    await user.click(screen.getByRole("button", { name: "本地服务" }));
    const list = panelByHeading("服务列表");
    expect(within(list).getByText("Local API")).toBeInTheDocument();
    expect(within(list).getByText("HTTP")).toBeInTheDocument();
    expect(within(list).getByText("127.0.0.1:18081")).toBeInTheDocument();
    expect(within(list).queryByText("访问地址")).not.toBeInTheDocument();
    expect(within(list).getByText("C:\\dev\\rust\\net-power\\net-power\\src-tauri")).toBeInTheDocument();
    expect(within(list).getByText("目录浏览 · 挂载 / · 1 接口 · 0 请求")).toBeInTheDocument();
    await user.click(within(list).getByTitle("编辑配置"));

    const panel = dialogByHeading("编辑服务配置");
    expect(within(panel).getByLabelText("服务名称")).toHaveValue("Local API");
    await user.clear(within(panel).getByLabelText("服务名称"));
    await user.type(within(panel).getByLabelText("服务名称"), "Local API v2");
    await user.clear(within(panel).getByLabelText("路径 #1"));
    await user.type(within(panel).getByLabelText("路径 #1"), "/api/v2");
    await user.click(within(panel).getByRole("button", { name: "保存配置" }));

    await waitFor(() => expect(apiMocks.toolServicesUpdate).toHaveBeenCalledTimes(1));
    expect(apiMocks.toolServicesUpdate).toHaveBeenCalledWith(
      "tool-edit",
      expect.objectContaining({
        name: "Local API v2",
        routes: [expect.objectContaining({ path: "/api/v2" })],
      }),
    );
  });

  it("edits inline HTTP tool service response content from the enlarged editor", async () => {
    const user = userEvent.setup();
    const body = "{\n  \"ok\": false,\n  \"name\": \"local\"\n}";
    render(<Workbench />);

    await screen.findByRole("heading", { name: "仪表盘" });
    await user.click(screen.getByRole("button", { name: "本地服务" }));
    await user.click(screen.getByRole("button", { name: "添加服务" }));
    const panel = dialogByHeading("添加服务");
    const inlineBody = within(panel).getByLabelText("接口 1 响应内容");
    expect(inlineBody).toHaveClass("response-body-textarea");

    await user.click(within(panel).getByRole("button", { name: "放大编辑接口 1 响应内容" }));
    const bodyDialog = screen.getByRole("dialog", { name: "编辑响应内容" });
    fireEvent.change(within(bodyDialog).getByLabelText("放大响应内容"), { target: { value: body } });
    await user.click(within(bodyDialog).getByRole("button", { name: "完成" }));

    expect(screen.queryByRole("dialog", { name: "编辑响应内容" })).not.toBeInTheDocument();
    expect(inlineBody).toHaveValue(body);
    await user.click(within(panel).getByRole("button", { name: "创建并启动" }));

    await waitFor(() => expect(apiMocks.toolServicesCreate).toHaveBeenCalledTimes(1));
    expect(apiMocks.toolServicesCreate).toHaveBeenCalledWith(
      expect.objectContaining({
        routes: [
          expect.objectContaining({
            body,
            contentSource: "inline",
          }),
        ],
      }),
    );
  });

  it("copies the LAN address and confirms success for wildcard HTTP tool services", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn();
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    mockApis({
      toolServices: [
        toolServiceSummary({
          host: "0.0.0.0",
          url: "http://0.0.0.0:18081/api/ping",
        }),
      ],
    });

    render(<Workbench />);

    await screen.findByRole("heading", { name: "仪表盘" });
    await user.click(screen.getByRole("button", { name: "本地服务" }));
    await user.click(await screen.findByTitle("复制地址"));

    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith("http://192.168.1.23:18081/api/ping"),
    );
    expect(await screen.findByText("地址已复制")).toBeInTheDocument();
    expect(screen.getByText("已复制局域网地址；本机也可用 http://127.0.0.1:18081/api/ping")).toBeInTheDocument();
    expect(apiMocks.networkGetLanIp).toHaveBeenCalledTimes(1);
  });

  it("uses the link icon for copy address actions across service menus", async () => {
    const user = userEvent.setup();
    mockApis({
      services: [serviceSummary({ id: "svc-http", name: "Reverse Proxy" })],
      toolServices: [toolServiceSummary({ id: "tool-http", name: "Local API" })],
    });

    render(<Workbench />);

    await screen.findByRole("heading", { name: "仪表盘" });
    await user.click(screen.getByRole("button", { name: "本地服务" }));
    expect((await screen.findByTitle("复制地址")).querySelector(".lucide-link-2")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "网络转发" }));
    expect((await screen.findByTitle("复制地址")).querySelector(".lucide-link-2")).toBeInTheDocument();
    await user.click(await screen.findByTitle("更多操作"));
    expect((await screen.findByTitle("复制配置")).querySelector(".lucide-copy")).toBeInTheDocument();
  });

  it("toggles persisted local HTTP services without deleting the saved config", async () => {
    const user = userEvent.setup();
    mockApis({
      toolServices: [
        toolServiceSummary({ id: "tool-running", name: "Running Tool", runtimeStatus: { type: "running" } }),
        toolServiceSummary({ id: "tool-stopped", name: "Stopped Tool", runtimeStatus: { type: "stopped" }, startedAt: null }),
      ],
    });

    render(<Workbench />);

    await screen.findByRole("heading", { name: "仪表盘" });
    await user.click(screen.getByRole("button", { name: "本地服务" }));

    const list = panelByHeading("服务列表");
    const [runningRow, stoppedRow] = Array.from(list.querySelectorAll<HTMLElement>(".tool-service-row"));
    if (!runningRow || !stoppedRow) {
      throw new Error("找不到工具服务行");
    }

    await user.click(within(runningRow).getByTitle("暂停服务"));
    await waitFor(() => expect(apiMocks.toolServicesStop).toHaveBeenCalledWith("tool-running"));
    expect(apiMocks.toolServicesDelete).not.toHaveBeenCalled();

    await user.click(within(stoppedRow).getByTitle("启动服务"));
    await waitFor(() => expect(apiMocks.toolServicesStart).toHaveBeenCalledWith("tool-stopped"));
  });

  it("passes log service, level, protocol and keyword filters to the API", async () => {
    const user = userEvent.setup();
    mockApis({
      services: [serviceSummary({ id: "svc-http", name: "Reverse" })],
      logs: [
        logRow({
          serviceId: "svc-http",
          message: "started",
          metaJson: "{\"protocol\":\"http\",\"status\":200}",
        }),
      ],
    });

    render(<Workbench />);

    await screen.findByRole("heading", { name: "仪表盘" });
    await user.click(await screen.findByTitle("更多操作"));
    await user.click(await screen.findByTitle("日志"));
    await waitFor(() =>
      expect(apiMocks.logsList).toHaveBeenCalledWith(expect.objectContaining({ serviceId: "svc-http", limit: 200 })),
    );

    await chooseComboboxOption(user, document.body, "级别筛选", "错误");
    await waitFor(() =>
      expect(apiMocks.logsList).toHaveBeenCalledWith(
        expect.objectContaining({ serviceId: "svc-http", level: "error", limit: 200 }),
      ),
    );
    await chooseComboboxOption(user, document.body, "协议筛选", "HTTP");
    await waitFor(() =>
      expect(apiMocks.logsList).toHaveBeenCalledWith(
        expect.objectContaining({ serviceId: "svc-http", protocol: "http", limit: 200 }),
      ),
    );
    expect(screen.getByText("{\"protocol\":\"http\",\"status\":200}")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("开始时间"), { target: { value: "2026-01-01T10:00" } });
    await waitFor(() =>
      expect(apiMocks.logsList).toHaveBeenCalledWith(
        expect.objectContaining({ createdAfter: "2026-01-01T10:00", limit: 200 }),
      ),
    );
    fireEvent.change(screen.getByLabelText("结束时间"), { target: { value: "2026-01-01T11:00" } });
    await waitFor(() =>
      expect(apiMocks.logsList).toHaveBeenCalledWith(
        expect.objectContaining({ createdBefore: "2026-01-01T11:00", limit: 200 }),
      ),
    );
    await user.type(screen.getByPlaceholderText("筛选消息或元数据"), "boom");
    await waitFor(() =>
      expect(apiMocks.logsList).toHaveBeenCalledWith(expect.objectContaining({ keyword: "boom", limit: 200 })),
    );
  });

  it("shows privacy-safe traffic details for a selected log row", async () => {
    const user = userEvent.setup();
    mockApis({
      services: [serviceSummary({ id: "svc-http", name: "Reverse" })],
      logs: [
        logRow({
          serviceId: "svc-http",
          message: "GET example.test /api",
          metaJson: JSON.stringify({
            source: "connection_events",
            protocol: "http",
            method: "GET",
            host: "example.test",
            path: "/api",
            statusCode: 200,
            bytesIn: 1536,
            bytesOut: 4096,
            durationMs: 25,
          }),
        }),
      ],
    });

    render(<Workbench />);

    await screen.findByRole("heading", { name: "仪表盘" });
    await user.click(await screen.findByTitle("更多操作"));
    await user.click(screen.getByRole("menuitem", { name: "日志" }));
    await user.click(screen.getByRole("button", { name: "详情" }));

    const detail = screen.getByRole("heading", { name: "流量详情" }).closest("aside");
    if (!detail) {
      throw new Error("找不到流量详情面板");
    }
    expect(within(detail).getByText("Reverse")).toBeInTheDocument();
    expect(within(detail).getByText("example.test")).toBeInTheDocument();
    expect(within(detail).getByText("/api")).toBeInTheDocument();
    expect(within(detail).getByText("200")).toBeInTheDocument();
    expect(within(detail).getByText("1.5 KB")).toBeInTheDocument();
    expect(within(detail).getByText("25 ms")).toBeInTheDocument();
  });

  it("starts a selected HTTP forward service before setting it as the system proxy", async () => {
    const user = userEvent.setup();
    mockApis({
      services: [
        serviceSummary({
          id: "svc-forward",
          name: "Forward",
          kind: "http_forward",
          runtimeStatus: { type: "stopped" },
        }),
      ],
    });
    render(<Workbench />);

    await screen.findByRole("heading", { name: "仪表盘" });
    await user.click(screen.getByRole("button", { name: "系统代理" }));
    const sourcePanel = panelByHeading("代理配置列表");
    const sourceMenu = sourcePanel.querySelector(".source-menu-list");
    if (!(sourceMenu instanceof HTMLElement)) {
      throw new Error("找不到系统代理配置菜单");
    }
    const sourceButton = within(sourceMenu).getByRole("button", { name: /Forward/ });
    expect(sourceButton).toBeInTheDocument();
    const row = sourceButton.closest(".source-row");
    if (!row) {
      throw new Error("找不到 Forward 行");
    }
    await user.click(within(sourcePanel).getByRole("button", { name: "启动后启用代理" }));

    await waitFor(() => expect(apiMocks.servicesStart).toHaveBeenCalledWith("svc-forward"));
    await waitFor(() => expect(apiMocks.proxySet).toHaveBeenCalledWith("svc-forward"));
  });

  it("selects a system proxy source from the unified list and enables it inline", async () => {
    const user = userEvent.setup();
    mockApis({
      services: [
        serviceSummary({
          id: "svc-forward",
          name: "Forward",
          kind: "http_forward",
          runtimeStatus: { type: "running" },
        }),
        serviceSummary({
          id: "svc-alt",
          name: "Forward Alt",
          kind: "http_forward",
          listenPort: 18081,
          runtimeStatus: { type: "running" },
        }),
      ],
    });
    render(<Workbench />);

    await screen.findByRole("heading", { name: "仪表盘" });
    await user.click(screen.getByRole("button", { name: "系统代理" }));
    const panel = panelByHeading("代理配置列表");
    const row = within(panel).getByText("Forward Alt").closest(".source-row");
    if (!row) {
      throw new Error("找不到 Forward Alt 行");
    }
    await user.click(within(row as HTMLElement).getByRole("button", { name: /Forward Alt/ }));
    expect(row).toHaveClass("selected");
    await user.click(within(panel).getByRole("button", { name: "启用代理" }));

    await waitFor(() => expect(apiMocks.proxySet).toHaveBeenCalledWith("svc-alt"));
  });

  it("keeps bypass empty after clearing and refreshing a disabled system proxy", async () => {
    const user = userEvent.setup();
    const activeStatus: SystemProxyStatus = {
      enabled: true,
      proxyHost: "127.0.0.1",
      proxyPort: 7890,
      bypass: "localhost;127.*",
      message: "系统代理已开启",
    };
    const clearedStatus: SystemProxyStatus = {
      enabled: false,
      proxyHost: "",
      proxyPort: null,
      bypass: "",
      message: "系统代理已清理",
    };
    const refreshedDisabledStatus: SystemProxyStatus = {
      enabled: false,
      proxyHost: "",
      proxyPort: null,
      bypass: "localhost;127.*",
      message: "系统代理未开启",
    };
    mockApis({ proxyStatus: activeStatus });
    apiMocks.proxyStatus.mockResolvedValueOnce(activeStatus).mockResolvedValue(refreshedDisabledStatus);
    apiMocks.proxyClear.mockResolvedValue(clearedStatus);

    render(<Workbench />);

    await screen.findByRole("heading", { name: "仪表盘" });
    await user.click(screen.getByRole("button", { name: "系统代理" }));
    const panel = panelByHeading("代理配置列表");
    expect(within(panel).getByText("localhost;127.*")).toBeInTheDocument();

    await user.click(within(panel).getByRole("button", { name: "清理" }));
    await waitFor(() => expect(apiMocks.proxyClear).toHaveBeenCalledTimes(1));
    expect(within(panel).getByText("无绕过地址")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "刷新" }));
    await waitFor(() => expect(apiMocks.proxyStatus).toHaveBeenCalledTimes(2));
    expect(within(panel).getByText("无绕过地址")).toBeInTheDocument();
    expect(within(panel).queryByText("localhost;127.*")).not.toBeInTheDocument();
  });

  it("creates, edits, uses and deletes system proxy profiles", async () => {
    const user = userEvent.setup();
    mockApis({ proxyProfiles: [systemProxyProfile()] });

    render(<Workbench />);

    await screen.findByRole("heading", { name: "仪表盘" });
    await user.click(screen.getByRole("button", { name: "系统代理" }));

    await user.click(screen.getByRole("button", { name: "添加配置" }));
    let panel = dialogByHeading("添加系统代理配置");
    await user.type(within(panel).getByLabelText("名称"), "办公网");
    await user.clear(within(panel).getByLabelText("主机"));
    await user.type(within(panel).getByLabelText("主机"), "127.0.0.1");
    await user.clear(within(panel).getByLabelText("端口"));
    await user.type(within(panel).getByLabelText("端口"), "7890");
    await user.click(within(panel).getByRole("button", { name: /添加配置/ }));
    await waitFor(() =>
      expect(apiMocks.proxyProfileCreate).toHaveBeenCalledWith(
        expect.objectContaining({ name: "办公网", proxyHost: "127.0.0.1", proxyPort: 7890 }),
      ),
    );

    await user.click(await screen.findByTitle("编辑配置"));
    panel = dialogByHeading("编辑系统代理配置");
    expect(within(panel).getByLabelText("名称")).toHaveValue("办公网代理");
    await user.clear(within(panel).getByLabelText("名称"));
    await user.type(within(panel).getByLabelText("名称"), "家庭代理");
    await user.click(within(panel).getByRole("button", { name: /保存配置/ }));
    await waitFor(() =>
      expect(apiMocks.proxyProfileUpdate).toHaveBeenCalledWith(
        "proxy-profile-1",
        expect.objectContaining({ name: "家庭代理" }),
      ),
    );

    const profileMainButton = within(panelByHeading("代理配置列表")).getByRole("button", { name: /办公网代理/ });
    const profileRow = profileMainButton.closest(".source-row");
    if (!profileRow) {
      throw new Error("找不到系统代理配置行");
    }
    await user.click(profileMainButton);
    await user.click(within(panelByHeading("代理配置列表")).getByRole("button", { name: "启用代理" }));
    await waitFor(() => expect(apiMocks.proxySet).toHaveBeenCalledWith("proxy-profile-1"));

    await user.click(await screen.findByTitle("删除配置"));
    await waitFor(() => expect(apiMocks.proxyProfileDelete).toHaveBeenCalledWith("proxy-profile-1"));
  });

  it("toggles OS autostart from the settings page", async () => {
    const user = userEvent.setup();
    mockApis({ autostartStatus: { enabled: false, supported: true, message: "未启用系统登录启动。" } });
    apiMocks.autostartSet.mockResolvedValue({
      enabled: true,
      supported: true,
      message: "已注册系统登录启动项。",
    } satisfies AutostartStatus);

    render(<Workbench />);
    await screen.findByRole("heading", { name: "仪表盘" });
    await user.click(screen.getByRole("button", { name: "设置" }));

    const panel = panelByHeading("应用设置");
    expect(within(panel).queryByRole("heading", { name: "运行快照" })).not.toBeInTheDocument();
    expect(within(panel).queryByRole("navigation", { name: "设置分类导航" })).not.toBeInTheDocument();
    expect(within(panel).getByRole("heading", { name: "关于" })).toBeInTheDocument();
    expect(within(panel).getByText(appVersion)).toBeInTheDocument();
    expect(within(panel).getByRole("link", { name: "github.com/kongweiguang/net-power" })).toHaveAttribute(
      "href",
      "https://github.com/kongweiguang/net-power",
    );
    const row = within(panel).getByText("开机启动应用").closest(".setting-row");
    if (!row) {
      throw new Error("找不到开机启动设置行");
    }
    await user.click(within(row as HTMLElement).getByRole("button", { name: "开启" }));

    await waitFor(() => expect(apiMocks.autostartSet).toHaveBeenCalledWith(true));
    await waitFor(() =>
      expect(within(row as HTMLElement).getByText("已注册系统登录启动项。")).toBeInTheDocument(),
    );
  });

  it("switches the persisted theme mode from the settings page", async () => {
    const user = userEvent.setup();

    render(<Workbench />);
    await screen.findByRole("heading", { name: "仪表盘" });
    await user.click(screen.getByRole("button", { name: "设置" }));

    const panel = panelByHeading("应用设置");
    const themeRow = within(panel).getByText("主题模式").closest(".setting-row");
    if (!themeRow) {
      throw new Error("找不到主题模式设置行");
    }
    expect(within(themeRow as HTMLElement).getByRole("combobox", { name: "主题模式" })).toHaveAttribute(
      "data-value",
      "system",
    );

    await chooseComboboxOption(user, panel, "主题模式", "深色");

    await waitFor(() => expect(apiMocks.settingsUpdate).toHaveBeenCalledWith("ui.theme_mode", "\"dark\""));
    await waitFor(() => expect(document.documentElement).toHaveAttribute("data-theme", "dark"));
    expect(document.documentElement).toHaveAttribute("data-theme-mode", "dark");
    expect(within(themeRow as HTMLElement).getByRole("combobox", { name: "主题模式" })).toHaveAttribute(
      "data-value",
      "dark",
    );
  });

  it("applies a saved dark theme before the settings page is opened", async () => {
    mockApis({
      settings: [
        ...defaultSettings(),
        { key: "ui.theme_mode", valueJson: "\"dark\"" },
      ],
    });

    render(<Workbench />);
    await screen.findByRole("heading", { name: "仪表盘" });

    await waitFor(() => expect(document.documentElement).toHaveAttribute("data-theme", "dark"));
    expect(document.documentElement).toHaveAttribute("data-theme-mode", "dark");
  });

  it("checks and installs updates from the settings page", async () => {
    const user = userEvent.setup();
    apiMocks.updaterCheckDownloadInstall.mockImplementation(async (onProgress: (progress: { message: string }) => void) => {
      onProgress({ message: "已下载 2.0 MB / 4.0 MB。" });
      return {
        updated: true,
        version: "0.2.0",
        relaunchTriggered: false,
        relaunchError: "测试环境不重启。",
      };
    });

    render(<Workbench />);
    await screen.findByRole("heading", { name: "仪表盘" });
    await user.click(screen.getByRole("button", { name: "设置" }));
    await user.click(screen.getByRole("button", { name: "检查更新" }));

    await waitFor(() => expect(apiMocks.updaterCheckDownloadInstall).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("已下载 2.0 MB / 4.0 MB。")).toBeInTheDocument();
    expect(await screen.findByText("已安装 0.2.0")).toBeInTheDocument();
  });
});

function mockApis({
  services = [],
  runtime = [],
  toolServices = [],
  profiles = [],
  logs = [],
  settings = defaultSettings(),
  autostartStatus = disabledAutostartStatus(),
  proxyStatus = disabledProxyStatus(),
  proxyProfiles = [],
}: {
  services?: ServiceSummary[];
  runtime?: ServiceRuntimeSummary[];
  toolServices?: ToolServiceSummary[];
  profiles?: SshProfile[];
  logs?: LogRow[];
  settings?: AppSetting[];
  autostartStatus?: AutostartStatus;
  proxyStatus?: SystemProxyStatus;
  proxyProfiles?: SystemProxyProfile[];
} = {}) {
  apiMocks.settingsList.mockResolvedValue(settings);
  apiMocks.settingsUpdate.mockResolvedValue(undefined);
  apiMocks.autostartGet.mockResolvedValue(autostartStatus);
  apiMocks.autostartSet.mockResolvedValue(autostartStatus);
  apiMocks.servicesList.mockResolvedValue(services);
  apiMocks.servicesGet.mockResolvedValue(serviceDetail());
  apiMocks.servicesCreate.mockResolvedValue(serviceDetail());
  apiMocks.servicesUpdate.mockResolvedValue(serviceDetail());
  apiMocks.servicesDelete.mockResolvedValue(undefined);
  apiMocks.servicesDuplicate.mockResolvedValue(serviceDetail());
  apiMocks.servicesStart.mockResolvedValue({ type: "running" });
  apiMocks.servicesStop.mockResolvedValue({ type: "stopped" });
  apiMocks.servicesRestart.mockResolvedValue({ type: "running" });
  apiMocks.servicesRuntime.mockResolvedValue(runtime);
  apiMocks.servicesTest.mockResolvedValue({ ok: true, message: "ok", durationMs: 1 });
  apiMocks.toolServicesList.mockResolvedValue(toolServices);
  apiMocks.toolServicesGet.mockResolvedValue(toolServiceConfig());
  apiMocks.toolServicesCreate.mockResolvedValue(toolServiceSummary());
  apiMocks.toolServicesUpdate.mockResolvedValue(toolServiceSummary());
  apiMocks.toolServicesStart.mockResolvedValue(toolServiceSummary());
  apiMocks.toolServicesStop.mockResolvedValue({ type: "stopped" });
  apiMocks.toolServicesDelete.mockResolvedValue(undefined);
  apiMocks.fileDialogChooseDirectory.mockResolvedValue("C:/fixtures/static");
  apiMocks.fileDialogChooseFile.mockResolvedValue("C:/fixtures/response.json");
  apiMocks.sshList.mockResolvedValue(profiles);
  apiMocks.sshGet.mockResolvedValue(profiles[0] ?? sshProfile());
  apiMocks.sshCreate.mockResolvedValue(sshProfile());
  apiMocks.sshUpdate.mockResolvedValue(sshProfile());
  apiMocks.sshDelete.mockResolvedValue(undefined);
  apiMocks.sshTest.mockResolvedValue({ ok: true, message: "ok", durationMs: 1 });
  apiMocks.logsList.mockResolvedValue(logs);
  apiMocks.logsClear.mockResolvedValue(undefined);
  apiMocks.networkGetLanIp.mockResolvedValue("192.168.1.23");
  apiMocks.proxyProfilesList.mockResolvedValue(proxyProfiles);
  apiMocks.proxyProfileCreate.mockResolvedValue(systemProxyProfile());
  apiMocks.proxyProfileUpdate.mockResolvedValue(systemProxyProfile());
  apiMocks.proxyProfileDelete.mockResolvedValue(undefined);
  apiMocks.proxySet.mockResolvedValue({ ...proxyStatus, enabled: true, message: "系统代理已设置" });
  apiMocks.proxySetTarget.mockResolvedValue({ ...proxyStatus, enabled: true, message: "系统代理已设置" });
  apiMocks.proxyClear.mockResolvedValue(disabledProxyStatus());
  apiMocks.proxyStatus.mockResolvedValue(proxyStatus);
  apiMocks.updaterCheckDownloadInstall.mockResolvedValue({ updated: false });
}

function panelByHeading(name: string): HTMLElement {
  const panel = screen.getByRole("heading", { name }).closest("section");
  if (!panel) {
    throw new Error(`找不到面板: ${name}`);
  }
  return panel;
}

function dialogByHeading(name: string): HTMLElement {
  return screen.getByRole("dialog", { name });
}

async function chooseComboboxOption(
  user: ReturnType<typeof userEvent.setup>,
  container: HTMLElement,
  name: string,
  optionName: string,
): Promise<void> {
  const scope = within(container);
  await user.click(scope.getByRole("combobox", { name }));
  await user.click(scope.getByRole("option", { name: optionName }));
}

function defaultSettings(): AppSetting[] {
  return [
    { key: "app.initialized", valueJson: "true" },
    { key: "services.auto_start_enabled", valueJson: "false" },
  ];
}

function disabledAutostartStatus(): AutostartStatus {
  return {
    enabled: false,
    supported: true,
    message: "未启用系统登录启动。",
  };
}

function disabledProxyStatus(): SystemProxyStatus {
  return {
    enabled: false,
    proxyHost: "",
    proxyPort: null,
    bypass: "",
    message: "系统代理未启用",
  };
}

function systemProxyProfile(overrides: Partial<SystemProxyProfile> = {}): SystemProxyProfile {
  return {
    id: "proxy-profile-1",
    name: "办公网代理",
    proxyHost: "127.0.0.1",
    proxyPort: 7890,
    bypass: "localhost;127.*",
    active: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

function serviceSummary(overrides: Partial<ServiceSummary> = {}): ServiceSummary {
  return {
    id: "svc-1",
    name: "Reverse",
    kind: "http_reverse",
    enabled: true,
    autoStart: false,
    listenHost: "127.0.0.1",
    listenPort: 18080,
    targetLabel: "http://127.0.0.1:19090",
    runtimeStatus: { type: "stopped" },
    activeConnections: 0,
    totalConnections: 0,
    ...overrides,
  };
}

function serviceDetail(overrides: Partial<ServiceDetail> = {}): ServiceDetail {
  return {
    id: "svc-1",
    name: "Reverse",
    kind: "http_reverse",
    enabled: true,
    autoStart: false,
    listenHost: "127.0.0.1",
    listenPort: 18080,
    notes: "",
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    httpReverse: {
      targetUrl: "http://127.0.0.1:19090",
      preserveHost: false,
      requestTimeoutMs: 30_000,
      maxRewriteBodyBytes: 10_485_760,
      skipCompressedBody: true,
    },
    httpForward: null,
    tcpForward: null,
    udpForward: null,
    sshTunnel: null,
    headerRules: [],
    bodyRewriteRules: [],
    ...overrides,
  };
}

function toolServiceSummary(overrides: Partial<ToolServiceSummary> = {}): ToolServiceSummary {
  return {
    id: "tool-1",
    name: "Local API",
    host: "127.0.0.1",
    port: 18081,
    url: "http://127.0.0.1:18081/api/ping",
    staticRootDir: null,
    staticMode: "directory",
    staticPathPrefix: "/",
    routeCount: 1,
    startedAt: "2026-01-01T00:00:00Z",
    totalRequests: 0,
    runtimeStatus: { type: "running" },
    ...overrides,
  };
}

function toolServiceConfig(overrides: Partial<ToolServiceConfig> = {}): ToolServiceConfig {
  return {
    id: "tool-1",
    name: "Local API",
    host: "127.0.0.1",
    port: 18081,
    staticRootDir: null,
    staticMode: "directory",
    staticPathPrefix: "/",
    routes: [
      {
        method: "GET",
        path: "/api/ping",
        responseStatus: 200,
        contentType: "application/json; charset=utf-8",
        contentSource: "inline",
        body: "{\n  \"ok\": true\n}",
        filePath: null,
      },
    ],
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

function sshProfile(overrides: Partial<SshProfile> = {}): SshProfile {
  return {
    id: "ssh-1",
    name: "Prod SSH",
    host: "prod.example.com",
    port: 22,
    username: "deploy",
    authType: "password",
    hasPassword: true,
    privateKeyPath: null,
    hasPassphrase: false,
    knownHostsMode: "accept_new",
    knownHostsPath: null,
    connectTimeoutMs: 10_000,
    keepaliveIntervalMs: 30_000,
    jumpProfileId: null,
    ...overrides,
  };
}

function logRow(overrides: Partial<LogRow> = {}): LogRow {
  return {
    id: 1,
    serviceId: null,
    level: "info",
    message: "started",
    metaJson: "{}",
    createdAt: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}
