<!-- @author kongweiguang -->

# net-power

net-power 是一个跨平台桌面代理管理工具，基于 Rust、Tauri 和 React 构建。它把本地代理服务、系统代理、SSH 转发和运行日志集中到一个桌面工作台中管理。

## 主要能力

- 管理本地代理服务：创建、编辑、复制、启动、停止、重启和软删除。
- HTTP 代理：支持 reverse proxy、forward proxy、HTTPS CONNECT、Header 规则和 Body rewrite。
- TCP/UDP 转发：支持常见端口转发场景。
- SSH 转发：支持本地转发、远程转发、SOCKS5 动态代理、跳板机、多级跳板和自动重连。
- 系统代理：支持 Windows、macOS、Linux 的代理设置、清理和状态读取。
- 配置持久化：使用 SQLite 保存服务配置、设置和运行数据。
- 运行日志：按服务查看连接、状态、耗时、字节和结构化 meta 信息。
- 桌面体验：系统托盘、关闭隐藏到托盘、开机启动和响应式工作台。
- 自动更新：通过 Tauri updater 从 GitHub Releases 检查、下载和安装更新。

## 使用方法

安装依赖：

```bash
npm install
```

启动桌面开发模式：

```bash
npm run tauri dev
```

仅启动前端调试：

```bash
npm run dev
```

构建桌面安装包：

```bash
npm run tauri -- build
```

运行常用检查：

```bash
npm run build
npm test
cd src-tauri
cargo fmt --check
cargo test
cargo clippy -- -D warnings
```

## 发布与更新

- GitHub Actions 已配置桌面端 CI 和 Release workflow。
- 推送 `v*.*.*` tag 后会构建 Windows NSIS、macOS Intel/Apple Silicon DMG、Linux AppImage/DEB。
- Release 会上传 Tauri updater 使用的 `latest.json`。
- 当前 updater endpoint：`https://github.com/kongweiguang/net-power/releases/latest/download/latest.json`。

## 文档

- [docs/README.md](docs/README.md)：文档索引。
- [docs/completed.md](docs/completed.md)：已完成能力。
- [docs/biz/README.md](docs/biz/README.md)：业务能力说明。
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)：架构说明。
- [docs/TESTING.md](docs/TESTING.md)：测试和验收说明。
