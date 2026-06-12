/**
 * @author kongweiguang
 * Playwright 视觉冒烟配置。用于复核浏览器预览模式下的中文工作台和响应式布局。
 */

import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  timeout: 30_000,
  expect: {
    timeout: 5_000,
  },
  reporter: "list",
  webServer: {
    command: "npm run dev -- --host 127.0.0.1",
    url: "http://127.0.0.1:1422",
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
  use: {
    baseURL: "http://127.0.0.1:1422",
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
});
