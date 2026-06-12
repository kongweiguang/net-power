<!-- @author kongweiguang -->

# 进行中事项

当前代码路径、自动化测试和本机 smoke 已覆盖主要功能。剩余事项集中在真实外部环境和发布链路，不再把已由自动化覆盖的测试细节重复列为待办。

- GitHub release workflow 已配置多平台桌面端构建和 updater `latest.json` 上传；仍需要在 GitHub 仓库配置 `TAURI_SIGNING_PRIVATE_KEY` 和 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secrets，并通过一次 `v*.*.*` tag 构建验证 Windows/macOS/Linux 产物和客户端更新链路。
- 在干净 Windows 环境验收 MSI/NSIS 安装器完整交互流程；当前 release exe 启动、SQLite 初始化、NSIS 临时静默安装、安装后 WebView 响应式截图、关闭隐藏到托盘和卸载已由 smoke 脚本覆盖。
- 在真实 Windows 托盘区域人工验收左键恢复窗口、右键中文菜单显示/隐藏/退出；菜单结构、动作映射和关闭隐藏到托盘已有自动化或 smoke 覆盖。
- 在真实 Windows 登录项人工验收开机启动：开启后注销/重启登录会自动启动 net-power，关闭后启动项被清理。
- 在真实 macOS、GNOME Linux 和非 GNOME Linux 环境验收系统代理写入、清理和异常提示。
- 流量详情已支持连接元数据查看；完整 payload 级抓包仍建议作为显式高级开关另行设计，默认继续不保存 body 和敏感 header。
