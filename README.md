# Portico Desktop

Portico 是一个 **local-first（本地优先）的个人桌面 Agent 工作台**。它把 AI 对话、工作区上下文、文件工具、终端、Git、记忆、插件与多 Agent 编排整合到一个 Tauri 桌面应用中，并以明确的权限边界、审批流程和审计记录保护本地开发环境。

> 当前版本：`0.1.0`。项目处于持续开发阶段，主要开发与原生端到端验证环境为 macOS；正式发布仍需真实 Provider 凭据验收、平台 CI 以及代码签名/公证。

## 核心能力

- **工作区驱动的 Agent 会话**：围绕本地项目创建线程，持久化消息、运行状态和模型快照。
- **多模型 Provider**：支持 OpenAI 兼容接口、DeepSeek、Moonshot、Ollama 等 Provider 配置与健康检查。
- **本地上下文与记忆**：索引工作区文档，提供 RAG 检索、记忆管理和可复用工作流模式。
- **安全工具执行**：提供受控的文件读写、搜索、终端和 Git 工具，并在执行前进行权限策略判断。
- **审批与审计**：高风险操作进入审批流程；关键操作、权限结果和运行事件可追踪。
- **Agent 编排**：默认使用单 Agent，在任务需要时支持受权限范围约束的多 Agent 协作。
- **插件、Skills 与 MCP**：发现和管理本地插件、工作流技能及 MCP 工具能力。
- **Markdown 工作台**：支持 GFM、代码高亮、KaTeX、Mermaid、预览以及 Markdown/DOCX 导出。
- **桌面可观测性**：提供运行时间线、上下文、文件、终端、Artifacts、通知和诊断视图。

## 技术栈

| 层级         | 主要技术                                  |
| ------------ | ----------------------------------------- |
| 桌面容器     | Tauri 2                                   |
| 后端与运行时 | Rust 2024、Tokio、SQLx、SQLite            |
| Agent 适配   | AutoAgents（锁定到已审查的 Git revision） |
| 前端         | React 19、TypeScript、Vite 6              |
| 状态与路由   | TanStack Query、TanStack Router           |
| 样式         | Tailwind CSS 4                            |
| 测试         | Vitest、Cargo Test、WebdriverIO           |
| 工程管理     | pnpm、Turborepo、Cargo Workspace          |

## 架构概览

```text
React / TypeScript UI
        │ Tauri IPC + events
        ▼
src-tauri (desktop composition root)
        │
        ├── app-runtime       会话、运行、存储、Provider、审批与工具执行
        ├── app-security      权限、命令与网络策略、密钥、脱敏、审计
        ├── app-tools         文件系统、终端与 Git 工具
        ├── app-memory        指令、记忆、Embedding 与 RAG
        ├── app-workflows     Agent 注册、调度与多 Agent 编排
        ├── app-plugins       插件、Skills 与 MCP 注册
        ├── app-models        领域模型与 TypeScript bindings
        └── autoagents-adapter
                              Portico 运行时与 AutoAgents 的适配层
```

设计原则：

1. **Local-first**：工作区、消息、运行记录与索引数据优先保存在本机。
2. **Fail closed**：不可信工作区或越界路径默认拒绝访问。
3. **Least privilege**：子 Agent 和工具不能扩大父任务授予的权限范围。
4. **Durable execution**：消息、审批、工具调用与恢复状态持久化，避免重启后盲目重放副作用。
5. **Secret isolation**：Provider 密钥通过系统 Keychain 或本地加密存储管理，不写入代码和普通配置。

更详细的产品路径见 [`docs/AGENT-PRODUCT-PATH.md`](docs/AGENT-PRODUCT-PATH.md)。

## 环境要求

通用开发环境：

- Node.js 20+
- pnpm `10.6.0`（仓库通过 `packageManager` 固定版本）
- Rust `1.88`+
- Git

macOS 原生 E2E 还需要系统可用的 `sqlite3`、`strings` 和 `uuidgen`。

Tauri 还需要对应操作系统的原生依赖。请参考 [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) 完成安装。

macOS 建议安装 Xcode Command Line Tools：

```bash
xcode-select --install
```

Linux 构建需要 WebKitGTK 等桌面依赖；Ubuntu/Debian 可参考 CI：

```bash
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
  librsvg2-dev libxdo-dev libssl-dev
```

## 快速开始

克隆仓库并安装依赖：

```bash
git clone git@github.com:xiaofuzi914/portico-desktop.git
cd portico-desktop
corepack enable
pnpm install --frozen-lockfile
```

启动完整桌面应用：

```bash
pnpm tauri:dev
```

仅启动前端开发服务器：

```bash
pnpm dev
```

默认前端开发地址为 `http://localhost:1420`。

## 模型与密钥配置

Provider 与模型通过应用内的 **Models（模型）** 页面配置。需要 API Key 的 Provider 应在界面中录入密钥；Portico 会将密钥交给安全存储，而不是保存到仓库。

- 不要把 API Key、Token 或密码写入源码、README、`.env` 示例或 Git 历史。
- Ollama 等本地 Provider 可以配置本地服务地址，不需要云端 API Key。
- 自定义 OpenAI 兼容 Provider 必须提供有效的 Base URL，并通过应用内健康检查后使用。

## 常用命令

| 命令                                                    | 用途                                 |
| ------------------------------------------------------- | ------------------------------------ |
| `pnpm dev`                                              | 启动前端开发服务器                   |
| `pnpm tauri:dev`                                        | 启动 Tauri 桌面开发环境              |
| `pnpm build`                                            | 构建前端工作区                       |
| `pnpm typecheck`                                        | 执行 TypeScript 类型检查             |
| `pnpm lint`                                             | 执行 ESLint                          |
| `pnpm format:check`                                     | 检查 Prettier 格式                   |
| `pnpm test`                                             | 执行前端测试                         |
| `pnpm test:coverage`                                    | 执行前端覆盖率测试                   |
| `cargo test --workspace`                                | 执行 Rust 工作区测试                 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 执行 Rust 静态检查                   |
| `cargo fmt --all -- --check`                            | 检查 Rust 格式                       |
| `pnpm tauri:e2e:desktop`                                | 执行 macOS 原生 WebView E2E 黄金路径 |

## 构建桌面安装包

构建 Tauri 配置中声明的平台安装包：

```bash
pnpm tauri build
```

在 macOS 本地构建未签名 DMG：

```bash
pnpm tauri:build:macos:local
```

验证本地 DMG：

```bash
pnpm tauri:verify:macos:local
```

构建产物默认位于 `target/release/bundle/`。正式发布仍需配置对应平台的代码签名、公证或安装包签名。

## 测试与质量门禁

提交前建议执行：

```bash
pnpm format:check
pnpm lint
pnpm typecheck
pnpm test:coverage
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI 还会执行：

- Rust 与前端依赖安全审计；
- 跟踪文件中的嵌入式凭据扫描；
- Rust 核心范围行覆盖率不低于 80%；
- 前端逻辑覆盖率校验；
- macOS 原生 WebView 重启与恢复路径 E2E。

## 项目结构

```text
.
├── apps/desktop-ui/         React 桌面前端
├── crates/                  Rust 领域模块与运行时
├── src-tauri/               Tauri 应用入口、IPC 命令与平台配置
├── e2e/                     桌面端 E2E 驱动
├── crates/*/tests/          Rust 模块集成测试
├── src-tauri/tests/         Tauri IPC 集成测试
├── examples/plugins/        插件示例与本地插件源
├── docs/                    产品路径、架构和验收文档
├── scripts/                 构建、E2E 与安装包验证脚本
├── .github/workflows/       CI 与发布工作流
├── Cargo.toml               Rust Workspace 配置
└── package.json             JavaScript Workspace 命令入口
```

## 安全模型

Portico 可以接触代码、文件、终端和网络，因此安全边界是产品能力的一部分：

- 工作区在显式信任前按不可信处理；
- 文件路径会规范化并验证是否位于授权根目录内，同时防止符号链接逃逸；
- Shell、网络和写操作经过权限策略检查；
- 危险命令、强制推送和越权访问默认拒绝；
- 有副作用的工具调用使用持久化状态和幂等控制；
- 日志与诊断导出执行敏感数据脱敏；
- Rust Workspace 禁止 `unsafe` 代码。

发现安全问题时，请不要创建包含利用细节或真实凭据的公开 Issue。请通过 [GitHub Private Vulnerability Reporting](https://github.com/xiaofuzi914/portico-desktop/security/advisories/new) 私密报告，并立即轮换任何可能暴露的凭据。

## 开发约定

- 新功能和缺陷修复遵循测试驱动开发，目标覆盖率为 80% 以上。
- 在系统边界验证所有外部输入，并返回明确、可操作的错误。
- 数据访问通过领域接口隔离，业务逻辑不直接依赖具体存储实现。
- 优先保持对象不可变、函数短小、模块高内聚。
- Commit 使用 Conventional Commits：`<type>: <description>`。
- 新工作流优先贡献到 `skills/`；`commands/` 仅保留兼容入口。

## 参与贡献

1. 从 `main` 创建功能分支。
2. 先补充或更新测试，再实现变更。
3. 运行本地质量门禁并确认没有凭据或敏感数据进入暂存区。
4. 使用 Conventional Commit 提交。
5. 创建 Pull Request，说明变更范围、风险、测试证据和回滚方式。

## 文档

- [`docs/AGENT-PRODUCT-PATH.md`](docs/AGENT-PRODUCT-PATH.md)：Agent 产品路径与运行模型。
- [`docs/reviews/2026-07-10-portico-desktop/README.md`](docs/reviews/2026-07-10-portico-desktop/README.md)：产品与架构审查入口。
- [`docs/reviews/2026-07-10-portico-desktop/15-user-acceptance.md`](docs/reviews/2026-07-10-portico-desktop/15-user-acceptance.md)：最终用户验收清单。

## License

本项目采用双许可证：

- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

你可以根据需要选择其中任一许可证使用本项目。详见根目录的 [`LICENSE`](LICENSE)。
