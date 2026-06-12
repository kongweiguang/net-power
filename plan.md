<!-- @author kongweiguang -->

# 历史计划迁移索引

`plan.md` 的产品目标、功能范围、架构、数据、安全、测试和交付标准已经沉淀到 `docs/` 下的稳定文档。本文件只保留迁移索引，避免继续把历史计划当成唯一事实来源。

## 文档映射

| 原计划内容 | 当前维护位置 |
| --- | --- |
| 产品目标、第一版范围、非目标、UI/UX 原则、最终交付标准 | [docs/PRODUCT.md](docs/PRODUCT.md) |
| 当前已实现能力 | [docs/completed.md](docs/completed.md) |
| 剩余风险、发布前人工验收和后续强化项 | [docs/in-progress.md](docs/in-progress.md) |
| HTTP、Forwarding、SSH、System Proxy、App Shell、Logs 业务说明 | [docs/biz/README.md](docs/biz/README.md) |
| Tauri/React/Rust 总体架构、模块边界、服务生命周期和响应式窗口策略 | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| SQLite 文件位置、核心表、迁移、数据校验和日志保留 | [docs/DATABASE.md](docs/DATABASE.md) |
| Tauri capabilities、CSP、secret 加密、SSH 安全、日志隐私和系统代理风险 | [docs/SECURITY.md](docs/SECURITY.md) |
| 自动化测试、外部 SSH 集成测试、smoke 脚本和人工验收清单 | [docs/TESTING.md](docs/TESTING.md) |

## 当前事实来源

- 功能是否已经落地，以 [docs/completed.md](docs/completed.md) 和对应测试结果为准。
- 仍需发布前复核的事项，以 [docs/in-progress.md](docs/in-progress.md) 为准。
- 开发、运行、构建和验证命令，以 [README.md](README.md) 和 [docs/TESTING.md](docs/TESTING.md) 为准。
- 业务行为和安全边界，以 [docs/biz/README.md](docs/biz/README.md)、[docs/DATABASE.md](docs/DATABASE.md) 和 [docs/SECURITY.md](docs/SECURITY.md) 为准。

## 维护约定

- 新功能或用户可见行为变化先更新对应稳定文档，再按需更新完成态或进行中事项。
- 不再把新增需求写回长篇计划清单；若需要规划新阶段，新增独立规范文档并在 [docs/README.md](docs/README.md) 挂入口。
- 外部 SSH 或 Docker 验收继续使用 WSL Docker，命令写成 `wsl.exe docker ...`。
