<!-- @author kongweiguang -->

# 进行中事项

- 发布前可继续做 Tauri dev 模式人工拖拽窗口复核；当前自动化测试、Playwright 桌面/平板/390px 窄屏视觉冒烟和真实 release WebView resize 截图 smoke 已通过。
- 在 Windows 干净环境验收 MSI/NSIS 安装器完整交互流程；release exe 启动和 SQLite 初始化已由 `npm run verify:release` 覆盖，NSIS 临时静默安装、安装后 WebView 响应式截图、托盘关闭隐藏和卸载已由 `npm run verify:installer` 覆盖，系统代理 HKCU 注册表写入/读取/清理已由 gated 集成测试覆盖。
- GitHub release workflow 已配置多平台桌面端构建和 updater `latest.json` 上传；仍需要在 GitHub 仓库配置 `TAURI_SIGNING_PRIVATE_KEY` 和 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secrets，并通过一次 `v*.*.*` tag 构建验证 Windows/macOS/Linux 产物和客户端更新链路。
- 流量详情已支持连接元数据查看；完整 payload 级抓包仍建议作为显式高级开关另行设计，默认继续不保存 body 和敏感 header。
- macOS/Linux 系统代理已具备平台后端，仍建议在真实 macOS、GNOME Linux 和非 GNOME Linux 上做人工验收。
- 系统托盘关闭按钮隐藏窗口已由 `npm run verify:tray` 覆盖，仍建议在真实 Windows 托盘区域人工验收：左键恢复窗口、右键中文菜单可显示/隐藏/退出。
- 开机启动已具备运行能力，仍建议在真实 Windows 登录项中人工验收：开启后注销/重启登录会自动启动 net-power，关闭后启动项被清理。
