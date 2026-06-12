<!-- @author kongweiguang -->

# 文档入口

本目录保存 net-power 的产品规范、能力状态、业务说明、架构、数据、安全和验收文档。当前仓库已经从 Tauri 模板推进到可运行的代理管理 MVP，前端/Rust 自动化测试、Windows 打包、release smoke、NSIS installer smoke、真实 release WebView smoke 和托盘关闭隐藏 smoke 已通过；GitHub Actions 已接入桌面端 CI、多平台 release 打包和 Tauri updater `latest.json` 发布链路；完整发布前主要剩余 GitHub Secrets 配置后的首次远端 release 验证、托盘区域恢复/菜单交互、开机启动、跨平台系统代理和干净 Windows 安装交互验收。

## 阅读顺序

1. [../README.md](../README.md)：开发、运行、测试、打包命令和当前能力。
2. [PRODUCT.md](PRODUCT.md)：产品目标、范围、非目标、UX 原则和交付标准。
3. [completed.md](completed.md)：当前已经落地的能力。
4. [COMPLETION_AUDIT.md](COMPLETION_AUDIT.md)：逐项审计完成证据、部分证明项和剩余验收。
5. [in-progress.md](in-progress.md)：剩余风险、未完成项和下一步。
6. [biz/README.md](biz/README.md)：按业务域说明代理能力。
7. [ARCHITECTURE.md](ARCHITECTURE.md)：Rust/Tauri/React 模块分工与服务生命周期。
8. [DATABASE.md](DATABASE.md)：SQLite 表职责、迁移策略和数据访问边界。
9. [SECURITY.md](SECURITY.md)：Tauri capabilities、CSP、secret、日志脱敏策略。
10. [TESTING.md](TESTING.md)：自动化测试、人工验收和发布前检查。
11. [../plan.md](../plan.md)：历史计划迁移索引，不再作为主事实来源。

## 文档职责

| 文档 | 职责 |
| --- | --- |
| `PRODUCT.md` | 固化产品目标、范围、非目标、信息架构和交付标准。 |
| `completed.md` | 列出当前代码已经实现并经过基础验证的能力。 |
| `COMPLETION_AUDIT.md` | 按产品要求列出当前证据等级、部分证明项和发布前剩余验收。 |
| `in-progress.md` | 记录剩余缺口、风险和推荐实现顺序。 |
| `biz/README.md` | 从用户视角说明 HTTP、Forwarding、SSH、System Proxy 和配置日志。 |
| `ARCHITECTURE.md` | 说明前端、Tauri command、数据库、服务管理器、代理实现和事件流的边界。 |
| `DATABASE.md` | 说明 SQLite 作为唯一配置来源的原则、核心表和 migration 口径。 |
| `SECURITY.md` | 说明最小权限、敏感数据加密、日志隐私和系统代理风险。 |
| `TESTING.md` | 说明当前可运行检查、必补测试和人工验收材料。 |
| `../plan.md` | 记录旧计划内容已迁移到哪些稳定文档。 |
