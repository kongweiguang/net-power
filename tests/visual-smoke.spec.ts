/**
 * @author kongweiguang
 * 工作台视觉冒烟测试。用真实浏览器复核中文页面、桌面/窄屏响应式和横向溢出。
 */

import { expect, test, type Page } from "@playwright/test";

const viewports = [
  { name: "desktop", width: 1440, height: 900 },
  { name: "tablet", width: 900, height: 720 },
  { name: "mobile", width: 390, height: 844 },
];

const pages = [
  { nav: "仪表盘", heading: "仪表盘" },
  { nav: "本地服务", heading: "本地服务" },
  { nav: "网络转发", heading: "网络转发" },
  { nav: "SSH", heading: "SSH" },
  { nav: "系统代理", heading: "系统代理" },
  { nav: "设置", heading: "设置" },
];

test.describe("工作台响应式视觉冒烟", () => {
  for (const viewport of viewports) {
    test(`${viewport.name} viewport renders Chinese workbench without page overflow`, async ({ page }) => {
      await page.setViewportSize({ width: viewport.width, height: viewport.height });
      await page.goto("/?preview=visual-smoke");
      await expect(page.getByRole("heading", { name: "仪表盘", level: 1 })).toBeVisible();
      await expect(page.locator("strong", { hasText: "办公 HTTP 正向代理" }).first()).toBeVisible();

      for (const item of pages) {
        await page.getByRole("button", { name: item.nav, exact: true }).click();
        await expect(page.getByRole("heading", { name: item.heading, level: 1 })).toBeVisible();
        if (item.context) {
          await expect(page.getByText(item.context)).toBeVisible();
        }
        await assertNoPageOverflow(page, `${viewport.name}:${item.heading}`);
      }

      if (viewport.name === "desktop") {
        await page.getByRole("button", { name: "系统代理", exact: true }).click();
        const systemProxyLayout = await page.evaluate(() => {
          const workspace = document.querySelector<HTMLElement>(".system-proxy-workspace")?.getBoundingClientRect();
          const list = document.querySelector<HTMLElement>(".source-menu-list")?.getBoundingClientRect();
          const detail = document.querySelector<HTMLElement>(".system-proxy-detail")?.getBoundingClientRect();
          const statusStrip = document.querySelector<HTMLElement>(".system-proxy-status-strip")?.getBoundingClientRect();
          const statusDot = document.querySelector<HTMLElement>(".system-proxy-status-dot")?.getBoundingClientRect();
          return {
            workspaceWidth: workspace?.width ?? 0,
            listWidth: list?.width ?? 0,
            detailWidth: detail?.width ?? 0,
            detailLeft: detail?.left ?? 0,
            listRight: list?.right ?? 0,
            statusStripWidth: statusStrip?.width ?? 0,
            statusDotSize: statusDot?.width ?? 0,
            statusDetailCount: document.querySelectorAll(".system-proxy-status-details > div").length,
          };
        });
        expect(systemProxyLayout.listWidth, "System proxy source menu should render as a compact menu").toBeGreaterThan(240);
        expect(systemProxyLayout.detailWidth, "System proxy detail should receive the main action space").toBeGreaterThan(
          systemProxyLayout.listWidth,
        );
        expect(systemProxyLayout.detailLeft, "System proxy detail should sit beside the menu on desktop").toBeGreaterThan(
          systemProxyLayout.listRight,
        );
        expect(systemProxyLayout.statusDotSize, "System proxy status should use a compact status dot").toBeGreaterThan(8);
        expect(systemProxyLayout.statusDetailCount, "System proxy status should show target and bypass only").toBe(2);

        await page.getByRole("button", { name: "SSH", exact: true }).click();
        const sshLayout = await page.evaluate(() => {
          const profile = document.querySelector<HTMLElement>(".ssh-profile-panel")?.getBoundingClientRect();
          const tunnel = document.querySelector<HTMLElement>(".ssh-tunnel-panel")?.getBoundingClientRect();
          const tunnelTable = document.querySelector<HTMLElement>(".ssh-tunnel-panel .table-wrap");
          return {
            profileWidth: profile?.width ?? 0,
            tunnelWidth: tunnel?.width ?? 0,
            tunnelTableClientWidth: tunnelTable?.clientWidth ?? 0,
            tunnelTableScrollWidth: tunnelTable?.scrollWidth ?? 0,
          };
        });
        expect(sshLayout.profileWidth, "SSH profile panel should read as a compact side panel").toBeLessThanOrEqual(
          380,
        );
        expect(sshLayout.profileWidth, "SSH tunnel panel should receive the main workspace width").toBeLessThan(
          sshLayout.tunnelWidth,
        );
        expect(
          sshLayout.tunnelTableScrollWidth,
          "SSH tunnel table should fit without an internal horizontal scrollbar",
        ).toBeLessThanOrEqual(sshLayout.tunnelTableClientWidth + 2);

        await page.getByRole("button", { name: "添加 SSH 配置", exact: true }).click();
        const profileDialog = await dialogMetrics(page);
        await page.getByTitle("关闭弹框").click();
        await page.getByRole("button", { name: "添加 SSH 隧道", exact: true }).click();
        const tunnelDialog = await dialogMetrics(page);
        expect(
          Math.abs(profileDialog.centerX - tunnelDialog.centerX),
          "SSH dialogs should share the same workspace center",
        ).toBeLessThanOrEqual(2);
        await page.getByTitle("关闭弹框").click();
        await assertNoPageOverflow(page, `${viewport.name}:SSH dialog alignment`);
      }

      await page.getByRole("button", { name: "网络转发", exact: true }).click();
      await page.getByRole("button", { name: "更多操作" }).first().click();
      await page.getByRole("menuitem", { name: "日志" }).click();
      await page.getByRole("button", { name: "详情" }).first().click();
      await expect(page.getByText("完整 meta JSON")).toBeVisible();
      await assertNoPageOverflow(page, `${viewport.name}:配置日志详情`);
    });
  }
});

async function assertNoPageOverflow(page: Page, label: string) {
  const metrics = await page.evaluate(() => {
    const viewportWidth = window.innerWidth;
    const documentWidth = Math.ceil(document.documentElement.scrollWidth);
    const bodyWidth = Math.ceil(document.body.scrollWidth);
    const inspectedSelectors = [
      ".app-frame",
      ".app-shell",
      ".system-titlebar",
      ".sidebar",
      ".workspace",
      ".topbar",
      ".panel",
      ".two-column",
      ".ssh-workbench",
      ".system-proxy-layout",
      ".dialog-panel",
      ".form-grid",
      ".filter-bar",
      ".compact-row",
      ".log-workbench",
      ".traffic-detail",
    ];
    const offenders = inspectedSelectors.flatMap((selector) =>
      Array.from(document.querySelectorAll<HTMLElement>(selector))
        .map((element) => {
          const rect = element.getBoundingClientRect();
          return {
            selector,
            right: Math.ceil(rect.right),
            left: Math.floor(rect.left),
            width: Math.ceil(rect.width),
            text: element.textContent?.replace(/\s+/g, " ").trim().slice(0, 80) ?? "",
          };
        })
        .filter((item) => item.right > viewportWidth + 2 || item.left < -2),
    );

    return { viewportWidth, documentWidth, bodyWidth, offenders };
  });

  expect(metrics.documentWidth, `${label} document should not overflow horizontally`).toBeLessThanOrEqual(
    metrics.viewportWidth + 2,
  );
  expect(metrics.bodyWidth, `${label} body should not overflow horizontally`).toBeLessThanOrEqual(
    metrics.viewportWidth + 2,
  );
  expect(metrics.offenders, `${label} visible elements overflow viewport`).toEqual([]);
}

async function dialogMetrics(page: Page) {
  return page.getByRole("dialog").evaluate((element) => {
    const rect = element.getBoundingClientRect();
    return {
      centerX: rect.left + rect.width / 2,
      centerY: rect.top + rect.height / 2,
      width: rect.width,
      height: rect.height,
    };
  });
}
