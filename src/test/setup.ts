/**
 * @author kongweiguang
 * 前端测试全局 setup。这里只注册断言扩展，业务 mock 放在具体测试文件中。
 */

import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(() => {
  cleanup();
});
