/**
 * @author kongweiguang
 * Workbench 纯模型测试，覆盖表单校验、协议转换和页面筛选边界。
 */

import { describe, expect, it } from "vitest";
import type { CreateServiceInput, ServiceDetail, ServiceSummary, SshProfile } from "../../types";
import {
  defaultServiceDraft,
  defaultSshDraft,
  filterServicesByPage,
  formatBytes,
  pageForServiceKind,
  parseServiceDraft,
  parseSshDraft,
  serviceDetailToDraft,
  sshProfileToDraft,
} from "./workbenchModel";

describe("workbenchModel", () => {
  it("maps http reverse draft into create input with structured rewrite rules", () => {
    const input = expectCreateServiceInput(
      parseServiceDraft({
        ...defaultServiceDraft,
        name: "  Reverse  ",
        listenPort: "9001",
        targetUrl: "https://api.example.com/base",
        preserveHost: true,
        requestTimeoutMs: "12000",
        maxRewriteBodyBytes: "2048",
        skipCompressedBody: false,
        headerRules: [
          {
            phase: "request",
            action: "set",
            name: " x-api-key ",
            value: " secret ",
            enabled: true,
            sortOrder: 2,
          },
          {
            phase: "response",
            action: "remove",
            name: "set-cookie",
            value: "ignored",
            enabled: true,
            sortOrder: 3,
          },
        ],
        bodyRewriteRules: [
          {
            bodyType: "json",
            path: "user.name",
            valueJson: "\"kong\"",
            enabled: true,
            sortOrder: 1,
          },
        ],
      }),
    );

    expect(input.name).toBe("Reverse");
    expect(input.listenPort).toBe(9001);
    expect(input.httpReverse?.targetUrl).toBe("https://api.example.com/base");
    expect(input.httpReverse?.preserveHost).toBe(true);
    expect(input.httpReverse?.requestTimeoutMs).toBe(12_000);
    expect(input.httpReverse?.maxRewriteBodyBytes).toBe(2_048);
    expect(input.httpReverse?.skipCompressedBody).toBe(false);
    expect(input.headerRules).toEqual([
      {
        phase: "request",
        action: "set",
        name: "x-api-key",
        value: "secret",
        enabled: true,
        sortOrder: 2,
      },
      {
        phase: "response",
        action: "remove",
        name: "set-cookie",
        value: null,
        enabled: true,
        sortOrder: 3,
      },
    ]);
    expect(input.bodyRewriteRules[0]?.valueJson).toBe("\"kong\"");
  });

  it("rejects invalid ports, target URLs and body rewrite JSON", () => {
    expect(parseServiceDraft({ ...defaultServiceDraft, name: "bad", listenPort: "70000" })).toContain("监听端口");
    expect(parseServiceDraft({ ...defaultServiceDraft, name: "bad", targetUrl: "ftp://example.com" })).toContain(
      "目标 URL",
    );
    expect(
      parseServiceDraft({
        ...defaultServiceDraft,
        name: "bad",
        bodyRewriteRules: [
          {
            bodyType: "json",
            path: "payload",
            valueJson: "{broken",
            enabled: true,
            sortOrder: 0,
          },
        ],
      }),
    ).toContain("不是合法 JSON");
  });

  it("requires ssh local tunnel profile and target", () => {
    expect(parseServiceDraft({ ...defaultServiceDraft, name: "ssh", kind: "ssh_local" })).toContain("SSH 隧道");
    const input = expectCreateServiceInput(
      parseServiceDraft({
        ...defaultServiceDraft,
        name: "ssh",
        kind: "ssh_local",
        sshProfileId: "profile-1",
        targetHost: "10.0.0.9",
        targetPort: "5432",
      }),
    );
    expect(input.sshTunnel).toEqual({
      sshProfileId: "profile-1",
      tunnelType: "local",
      targetHost: "10.0.0.9",
      targetPort: 5432,
      remoteBindHost: null,
      remoteBindPort: null,
    });
  });

  it("maps ssh socks draft without requiring a fixed target", () => {
    expect(parseServiceDraft({ ...defaultServiceDraft, name: "socks", kind: "ssh_socks" })).toContain(
      "SSH SOCKS5",
    );
    const input = expectCreateServiceInput(
      parseServiceDraft({
        ...defaultServiceDraft,
        name: "SSH SOCKS",
        kind: "ssh_socks",
        sshProfileId: "profile-1",
        targetHost: "",
        targetPort: "",
      }),
    );
    expect(input.sshTunnel).toEqual({
      sshProfileId: "profile-1",
      tunnelType: "socks",
      targetHost: null,
      targetPort: null,
      remoteBindHost: null,
      remoteBindPort: null,
    });
  });

  it("maps ssh remote draft with remote bind and local target", () => {
    expect(parseServiceDraft({ ...defaultServiceDraft, name: "remote", kind: "ssh_remote" })).toContain("SSH 远程");
    const input = expectCreateServiceInput(
      parseServiceDraft({
        ...defaultServiceDraft,
        name: "SSH Remote",
        kind: "ssh_remote",
        sshProfileId: "profile-1",
        listenHost: "0.0.0.0",
        listenPort: "18080",
        targetHost: "127.0.0.1",
        targetPort: "8080",
      }),
    );
    expect(input.sshTunnel).toEqual({
      sshProfileId: "profile-1",
      tunnelType: "remote",
      targetHost: "127.0.0.1",
      targetPort: 8080,
      remoteBindHost: "0.0.0.0",
      remoteBindPort: 18080,
    });
  });

  it("maps forward service advanced timeout fields", () => {
    const httpForward = expectCreateServiceInput(
      parseServiceDraft({
        ...defaultServiceDraft,
        name: "Forward",
        kind: "http_forward",
        allowHttp: false,
        allowConnect: true,
        connectTimeoutMs: "3500",
        idleTimeoutMs: "0",
      }),
    );
    expect(httpForward.httpForward).toEqual({
      allowHttp: false,
      allowConnect: true,
      connectTimeoutMs: 3_500,
      idleTimeoutMs: 0,
    });

    const udpForward = expectCreateServiceInput(
      parseServiceDraft({
        ...defaultServiceDraft,
        name: "UDP",
        kind: "udp_forward",
        targetHost: "10.0.0.8",
        targetPort: "5353",
        idleTimeoutMs: "15000",
      }),
    );
    expect(udpForward.udpForward).toEqual({
      targetHost: "10.0.0.8",
      targetPort: 5353,
      idleTimeoutMs: 15_000,
    });
  });

  it("maps ssh profile drafts without leaking existing secrets", () => {
    expect(parseSshDraft({ ...defaultSshDraft, name: "key", host: "dev", username: "root", authType: "private_key" }))
      .toContain("私钥认证");
    expect(parseSshDraft({ ...defaultSshDraft, name: "dev", host: "10.0.0.2", username: "root" })).toContain(
      "密码认证",
    );

    const parsed = parseSshDraft({
      ...defaultSshDraft,
      name: " dev ",
      host: " 10.0.0.2 ",
      username: " root ",
      password: "new-password",
      knownHostsMode: "strict",
      knownHostsPath: " C:/Users/test/.ssh/known_hosts ",
      connectTimeoutMs: "4500",
      keepaliveIntervalMs: "12000",
      jumpProfileId: "jump-1",
    });
    expect(typeof parsed).not.toBe("string");
    if (typeof parsed !== "string") {
      expect(parsed).toMatchObject({
        name: "dev",
        host: "10.0.0.2",
        username: "root",
        password: "new-password",
        knownHostsMode: "strict",
        knownHostsPath: "C:/Users/test/.ssh/known_hosts",
        connectTimeoutMs: 4_500,
        keepaliveIntervalMs: 12_000,
        jumpProfileId: "jump-1",
      });
    }

    const draft = sshProfileToDraft({
      id: "profile-1",
      name: "Prod",
      host: "prod.example.com",
      port: 22,
      username: "deploy",
      authType: "password",
      hasPassword: true,
      privateKeyPath: null,
      hasPassphrase: false,
      knownHostsMode: "accept_new",
      knownHostsPath: "C:/Users/test/.ssh/known_hosts",
      connectTimeoutMs: 10_500,
      keepaliveIntervalMs: 31_000,
      jumpProfileId: "jump-1",
    } satisfies SshProfile);
    expect(draft.password).toBe("");
    expect(draft.privateKeyPassphrase).toBe("");
    expect(draft.knownHostsPath).toBe("C:/Users/test/.ssh/known_hosts");
    expect(draft.connectTimeoutMs).toBe("10500");
    expect(draft.keepaliveIntervalMs).toBe("31000");
    expect(draft.jumpProfileId).toBe("jump-1");
  });

  it("roundtrips service detail into an editable draft", () => {
    const draft = serviceDetailToDraft({
      id: "svc-1",
      name: "TCP",
      kind: "tcp_forward",
      enabled: true,
      autoStart: true,
      listenHost: "127.0.0.1",
      listenPort: 15432,
      notes: "database",
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
      httpReverse: null,
      httpForward: null,
      tcpForward: {
        targetHost: "10.0.0.5",
        targetPort: 5432,
        connectTimeoutMs: 10_000,
        idleTimeoutMs: 0,
      },
      udpForward: null,
      sshTunnel: null,
      headerRules: [],
      bodyRewriteRules: [],
    } satisfies ServiceDetail);

    expect(draft.kind).toBe("tcp_forward");
    expect(draft.listenPort).toBe("15432");
    expect(draft.targetHost).toBe("10.0.0.5");
    expect(draft.targetPort).toBe("5432");
    expect(draft.autoStart).toBe(true);
    expect(draft.connectTimeoutMs).toBe("10000");
    expect(draft.idleTimeoutMs).toBe("0");
  });

  it("roundtrips ssh socks service detail into an editable draft", () => {
    const draft = serviceDetailToDraft({
      id: "svc-socks",
      name: "SOCKS",
      kind: "ssh_socks",
      enabled: true,
      autoStart: false,
      listenHost: "127.0.0.1",
      listenPort: 1080,
      notes: "",
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
      httpReverse: null,
      httpForward: null,
      tcpForward: null,
      udpForward: null,
      sshTunnel: {
        sshProfileId: "profile-1",
        tunnelType: "socks",
        targetHost: null,
        targetPort: null,
        remoteBindHost: null,
        remoteBindPort: null,
      },
      headerRules: [],
      bodyRewriteRules: [],
    } satisfies ServiceDetail);

    expect(draft.kind).toBe("ssh_socks");
    expect(draft.sshProfileId).toBe("profile-1");
    expect(draft.listenPort).toBe("1080");
    expect(draft.targetHost).toBe(defaultServiceDraft.targetHost);
    expect(draft.targetPort).toBe(defaultServiceDraft.targetPort);
  });

  it("roundtrips ssh remote service detail into an editable draft", () => {
    const draft = serviceDetailToDraft({
      id: "svc-remote",
      name: "Remote",
      kind: "ssh_remote",
      enabled: true,
      autoStart: false,
      listenHost: "127.0.0.1",
      listenPort: 18080,
      notes: "",
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
      httpReverse: null,
      httpForward: null,
      tcpForward: null,
      udpForward: null,
      sshTunnel: {
        sshProfileId: "profile-1",
        tunnelType: "remote",
        targetHost: "127.0.0.1",
        targetPort: 8080,
        remoteBindHost: "0.0.0.0",
        remoteBindPort: 18080,
      },
      headerRules: [],
      bodyRewriteRules: [],
    } satisfies ServiceDetail);

    expect(draft.kind).toBe("ssh_remote");
    expect(draft.sshProfileId).toBe("profile-1");
    expect(draft.listenHost).toBe("0.0.0.0");
    expect(draft.listenPort).toBe("18080");
    expect(draft.targetHost).toBe("127.0.0.1");
    expect(draft.targetPort).toBe("8080");
  });

  it("filters services by current page and formats runtime values", () => {
    const services: ServiceSummary[] = [
      service("http_reverse", "http"),
      service("tcp_forward", "tcp"),
      service("ssh_local", "ssh"),
      service("ssh_remote", "remote"),
      service("ssh_socks", "socks"),
    ];

    expect(filterServicesByPage(services, "http").map((item) => item.id)).toEqual(["http"]);
    expect(filterServicesByPage(services, "forwarding").map((item) => item.id)).toEqual(["tcp"]);
    expect(filterServicesByPage(services, "ssh").map((item) => item.id)).toEqual(["ssh", "remote", "socks"]);
    expect(pageForServiceKind("udp_forward")).toBe("forwarding");
    expect(pageForServiceKind("ssh_remote")).toBe("ssh");
    expect(pageForServiceKind("ssh_socks")).toBe("ssh");
    expect(formatBytes(1024)).toBe("1.0 KB");
  });
});

function expectCreateServiceInput(value: ReturnType<typeof parseServiceDraft>): CreateServiceInput {
  if (typeof value === "string") {
    throw new Error(value);
  }
  return value;
}

function service(kind: ServiceSummary["kind"], id: string): ServiceSummary {
  return {
    id,
    name: id,
    kind,
    enabled: true,
    autoStart: false,
    listenHost: "127.0.0.1",
    listenPort: 8000,
    targetLabel: "",
    runtimeStatus: { type: "stopped" },
    activeConnections: 0,
    totalConnections: 0,
  };
}
