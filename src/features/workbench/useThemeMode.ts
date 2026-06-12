/**
 * @author kongweiguang
 * Workbench 主题模式解析和 DOM 主题属性同步。
 */

import { useEffect, useMemo, useState } from "react";
import type { AppSetting } from "../../types";
import type { SelectOption } from "./workbenchShared";

/** 应用设置中保存主题模式的 key。 */
export const themeModeSettingKey = "ui.theme_mode";

/** 用户可选的主题模式。 */
export type ThemeMode = "light" | "dark" | "system";

/** 实际渲染使用的主题。 */
export type ResolvedTheme = "light" | "dark";

/** 主题模式选择器选项。 */
export const themeModeOptions: SelectOption[] = [
  { value: "light", label: "浅色" },
  { value: "dark", label: "深色" },
  { value: "system", label: "跟随系统" },
];

const defaultThemeMode: ThemeMode = "system";
const systemDarkQuery = "(prefers-color-scheme: dark)";

/** 将主题模式编码为 app_settings 可保存的 JSON 字符串。 */
export function themeModeValueJson(mode: ThemeMode): string {
  return JSON.stringify(mode);
}

/** 从设置行解析主题模式，异常值统一回退到跟随系统。 */
export function themeModeFromSettings(settings: AppSetting[]): ThemeMode {
  const valueJson = settings.find((setting) => setting.key === themeModeSettingKey)?.valueJson;
  if (!valueJson) {
    return defaultThemeMode;
  }
  try {
    const parsed = JSON.parse(valueJson) as unknown;
    return isThemeMode(parsed) ? parsed : defaultThemeMode;
  } catch {
    return defaultThemeMode;
  }
}

/** 绑定当前主题到 documentElement，供 CSS 变量按主题切换。 */
export function useThemeMode(settings: AppSetting[]) {
  const themeMode = useMemo(() => themeModeFromSettings(settings), [settings]);
  const [systemPrefersDark, setSystemPrefersDark] = useState(prefersDarkTheme);
  const resolvedTheme: ResolvedTheme =
    themeMode === "system" ? (systemPrefersDark ? "dark" : "light") : themeMode;

  useEffect(() => {
    if (typeof window === "undefined" || !window.matchMedia) {
      return;
    }
    const media = window.matchMedia(systemDarkQuery);
    const handleChange = () => setSystemPrefersDark(media.matches);
    handleChange();
    media.addEventListener("change", handleChange);
    return () => media.removeEventListener("change", handleChange);
  }, []);

  useEffect(() => {
    if (typeof document === "undefined") {
      return;
    }
    const root = document.documentElement;
    root.dataset.theme = resolvedTheme;
    root.dataset.themeMode = themeMode;
    root.style.colorScheme = resolvedTheme;
  }, [resolvedTheme, themeMode]);

  return { themeMode, resolvedTheme };
}

function prefersDarkTheme(): boolean {
  return typeof window !== "undefined" &&
    Boolean(window.matchMedia?.(systemDarkQuery).matches);
}

function isThemeMode(value: unknown): value is ThemeMode {
  return value === "light" || value === "dark" || value === "system";
}
