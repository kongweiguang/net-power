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
  { nav: "转发", heading: "转发" },
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

      await page.getByRole("button", { name: "转发", exact: true }).click();
      await page.getByRole("button", { name: "日志" }).first().click();
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
