<!-- @author kongweiguang -->

# 文档入口

本目录保存 net-power 的产品规范、能力状态、业务说明、架构、数据、安全和验收文档。当前应用已经形成可运行的桌面代理工作台：支持本地服务、网络转发、SSH 隧道、系统代理、日志、主题、托盘、开机启动和自动更新。最新工作区能力包含本地/局域网启动范围、复制访问地址、本地 HTTP 工具服务编辑、静态目录“目录浏览/静态网站”模式，以及设置页浅色/深色/跟随系统主题。

自动化验证的事实来源放在 [engineering/testing.md](engineering/testing.md) 和 [product/completion-audit.md](product/completion-audit.md)；本文档只保留入口和职责，不重复展开测试流水。发布前仍需真实环境验收的项目集中记录在 [product/in-progress.md](product/in-progress.md)。

## 阅读顺序

1. [../README.md](../README.md)：开发、运行、测试、打包命令和当前能力。
2. [user/README.md](user/README.md)：面向用户的功能选择、使用路线和各功能数据流转说明。
3. [product/README.md](product/README.md)：产品目标、范围、非目标、UX 原则和交付标准。
4. [business/README.md](business/README.md)：按业务域说明代理能力。
5. [engineering/architecture.md](engineering/architecture.md)：Rust/Tauri/React 模块分工与服务生命周期。
6. [engineering/database.md](engineering/database.md)：SQLite 表职责、迁移策略和数据访问边界。
7. [engineering/security.md](engineering/security.md)：Tauri capabilities、CSP、secret、日志脱敏策略。
8. [engineering/testing.md](engineering/testing.md)：自动化测试、人工验收和发布前检查。
9. [product/completed.md](product/completed.md)：当前已经落地的能力。
10. [product/completion-audit.md](product/completion-audit.md)：逐项审计完成证据、部分证明项和剩余验收。
11. [product/in-progress.md](product/in-progress.md)：剩余风险、未完成项和下一步。

## 分类结构

| 分类 | 文档 | 职责 |
| --- | --- |
| 用户使用 | `user/README.md` | 从用户视角解释该用哪个功能、基础使用路线、各功能网络流向和配置到运行日志的数据流转。 |
| 产品状态 | `product/README.md` | 固化产品目标、范围、非目标、信息架构和交付标准。 |
| 产品状态 | `product/completed.md` | 列出当前代码已经实现并经过基础验证的能力。 |
| 产品状态 | `product/completion-audit.md` | 按产品要求列出当前证据等级、部分证明项和发布前剩余验收。 |
| 产品状态 | `product/in-progress.md` | 记录剩余缺口、风险和推荐实现顺序。 |
| 业务能力 | `business/README.md` | 从用户视角说明转发、SSH、System Proxy、Tool Services 和配置日志。 |
| 工程实现 | `engineering/architecture.md` | 说明前端、Tauri command、数据库、服务管理器、代理实现和事件流的边界。 |
| 工程实现 | `engineering/database.md` | 说明 SQLite 作为唯一配置来源的原则、核心表和 migration 口径。 |
| 工程实现 | `engineering/security.md` | 说明最小权限、敏感数据加密、日志隐私和系统代理风险。 |
| 工程实现 | `engineering/testing.md` | 说明当前可运行检查、gated 集成测试、smoke 和人工验收材料。 |
