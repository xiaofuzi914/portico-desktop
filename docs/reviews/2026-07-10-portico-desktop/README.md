# Portico Desktop 产品与架构审查

审查日期：2026-07-10  
审查范围：`portico-desktop` 全仓、运行时数据库与 `/private/tmp` 中 Portico 现场  
审查视角：高级产品、系统架构、安全与交付质量  
发布结论：**No-Go；当前版本不可作为可用产品分发。**

> 2026-07-10 修复进展：收窄后的 macOS 0.1 安全黄金路径已完成工程闭环，
> 包含 Approval continuation、受控文件/Git 工具、Provider health/active selection、
> 真 IPC、原生 WebView E2E、重启持久化与 DMG smoke。实时状态与证据见
> [14-closure-ledger.md](14-closure-ledger.md)。对外发布仍因真实凭据 UAT、Developer ID
> 公证签名和三平台远端 CI 结果保持 No-Go。

## 1. 执行摘要

Portico 已经具备较完整的界面骨架、SQLite 数据模型和若干基础服务，代码也能编译、启动并通过现有单元测试；但真实用户路径同时被以下系统问题阻断：

1. 大量 Tauri 参数名与返回 envelope 不一致，界面与后端无法可靠通信。
2. `Thread / Message / Turn / Run` 领域模型未闭环，只能尝试单轮执行，刷新后不能恢复。
3. Provider 只在启动时装配，配置保存不生效；未配置时又静默使用 Mock。
4. Permission、Approval 与工具调用彼此分离，正常写操作无法审批后继续。
5. Terminal、MCP 和 workspace root 存在高风险边界绕过，不应在当前状态开放。
6. Browser、Desktop、Artifact、Plugin、Skill、Worktree、Automation 多数只是 UI 或数据骨架，却被呈现为可用能力。
7. 当前测试全部是模块级证据，没有真实 Tauri IPC、桌面 E2E、安装包 smoke test 或覆盖率门禁。

这不是“修几个按钮”可以解决的问题。建议把 0.1 产品范围收窄为：

> 本地开发 Agent：选择代码目录 → 配置真实模型 → 多轮对话 → 安全读取文件 → 写入审批 → 查看 diff/制品 → 重启恢复。

Browser、Desktop RPA、可执行插件平台、通用自动化和多 Agent 编排应在上述黄金路径稳定后再逐项恢复。

## 2. 现场证据

### 2.1 可验证的工程状态

| 检查 | 结果 | 能证明什么 |
|---|---:|---|
| `pnpm test` | 19 个文件、138 个测试通过 | 前端局部逻辑可运行 |
| `pnpm typecheck` / `pnpm lint` | 通过 | 静态类型与现有 lint 规则通过 |
| `cargo test --workspace` | 240 个测试通过 | Rust 模块单测通过 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 | 当前 Clippy 规则通过 |
| `cargo fmt --all -- --check` | 失败 | Rust 格式未形成门禁 |
| Prettier check | 失败，51 个文件 | 前端格式未形成门禁 |
| `pnpm audit --audit-level moderate` | 失败 | `happy-dom@17.6.3` 有 1 critical、2 high |
| E2E / 真 IPC / coverage | 不存在 | 不能证明桌面产品可用 |

### 2.2 本机运行现场

- `/private/tmp/portico-desktop-tauri.log` 证明 Vite、Tauri、SQLite 和 23 个 migration 可启动，但第 43 行出现 `web content process terminated`；其余主要是空的后台轮询。
- 实际 SQLite 中有 2 个 workspace、3 个 thread、1 个 provider，但 `agent_runs`、`run_events`、`approval_requests`、`audit_log`、`usage_records` 和 `artifacts` 均为 0。这与“能建项目/会话、不能进入执行闭环”完全一致。
- `/private/tmp/portico-audit` 只有审查 prompt；唯一输出是 `Cannot combine --prompt with --yolo.`，预期审查 JSON 不存在，之前的自动审查未完成。
- `/private/tmp/portico-wt-test` 与 `/private/tmp/portico-orchestrator-test` 中只有普通空目录，不是真 Git worktree。
- `/private/tmp/portico-symtest/root/link.txt` 是指向外部目标的 symlink 测试残留，但仓库中没有对应的 symlink 越界回归测试。

## 3. 模块健康矩阵

| 模块 | 当前判定 | 最高级别 | 修复文档 |
|---|---|---:|---|
| IPC / 类型 / 错误协议 | 系统性失配 | P0 | [01-ipc-contract.md](01-ipc-contract.md) |
| Shell / i18n / Accessibility / Workspace / Thread | 可展示，不能形成可靠工作流 | P0 | [02-shell-workspace-thread.md](02-shell-workspace-thread.md) |
| Agent Runtime / Conversation / Event | 核心闭环不可用 | P0 | [03-agent-runtime-conversation.md](03-agent-runtime-conversation.md) |
| Permission / Approval / Audit | 审批断链且边界不一致 | P0 | [04-security-approval-audit.md](04-security-approval-audit.md) |
| Files / Git / Terminal / Worktree | 工具有骨架，隔离与执行不安全 | P0 | [05-tools-terminal-git-worktree.md](05-tools-terminal-git-worktree.md) |
| Provider / Model / Usage | 配置与执行脱节 | P0 | [06-model-provider-usage.md](06-model-provider-usage.md) |
| Memory / Context / RAG | 数据存在，未进入 Agent | P1 | [07-memory-context-rag.md](07-memory-context-rag.md) |
| Plugin / Skill / MCP | 插件仅登记；MCP 有 RCE/SSRF 风险 | P0 | [08-plugins-skills-mcp.md](08-plugins-skills-mcp.md) |
| Inspector / Browser / Desktop / Artifact | 多数不可调用或返回错误 | P0 | [09-inspector-browser-desktop-artifact.md](09-inspector-browser-desktop-artifact.md) |
| Automation / Background / Subagent / Notification | 状态语义不完整 | P1 | [10-automation-subagent-notification.md](10-automation-subagent-notification.md) |
| Storage / Migration / Diagnostics | 可存储，恢复和脱敏不足 | P1 | [11-storage-diagnostics-migrations.md](11-storage-diagnostics-migrations.md) |
| Build / Release / Test / Observability | 无法证明可复现交付 | P0 | [12-build-release-test-observability.md](12-build-release-test-observability.md) |

上述表格保留初始审查判定作为基线。最新工程闭环状态见 [14-closure-ledger.md](14-closure-ledger.md)，最终人工黄金路径见 [15-user-acceptance.md](15-user-acceptance.md)。

## 4. 目标架构

```text
Generated TypeScript SDK
  └─ Typed Command Facade
      └─ Application Use Cases
          ├─ ConversationService
          ├─ ExecutionService
          ├─ CapabilityService
          ├─ AutomationService
          └─ WorkspaceService
               ├─ ExecutionContext + PolicyGate
               ├─ Durable Event Outbox
               ├─ ProviderResolver（per run）
               └─ Worker Supervisor
                    ├─ SQLite repositories
                    ├─ Keychain secret store
                    ├─ AutoAgents adapter
                    ├─ Git worktree / PTY adapters
                    └─ Browser / Desktop / MCP sandboxes
```

建议核心领域模型固定为：

```text
Workspace → Thread → Message → Turn/Run
                         ├─ ToolInvocation
                         ├─ ApprovalRequest
                         ├─ RuntimeEvent
                         └─ Artifact
```

`Thread` 是对话容器；每次用户发送消息原子创建 `Message + Turn/Run`。不再要求用户先手动 Start Run，也不复用已经 terminal 的 run。

## 5. 立即止血

在核心修复完成前：

- 把 Terminal、MCP stdio、Browser、Desktop、可写 Worktree/Orchestrator 放到关闭的 feature flag 后。
- 将 Plugin/Skill、Artifact、Automation、Migration rollback、Diagnostics upload 明确标为“未实现”，或从产品界面隐藏。
- 删除 `core ready`、`native tools ready` 这类静态乐观状态；由后端 capability probe 决定。
- 禁止生产环境静默使用 Mock executor。
- 停止复制未脱敏日志到 diagnostics bundle，并轮换任何可能已经出现在日志中的 secret。

## 6. 发布黄金路径

发布前必须由真实桌面 E2E 验证：

> Fresh install → 选择真实项目目录 → 配置并测试真实模型 → 创建 thread → 连续多轮对话 → 读取文件 → 写文件触发审批 → 批准后同一调用继续 → 查看 diff → 生成并预览 artifact → 关闭重开 App → 历史和状态完整恢复。

Terminal 不作为 0.1 beta 的必选能力；若 release build 启用 Terminal，则必须额外通过 [05-tools-terminal-git-worktree.md](05-tools-terminal-git-worktree.md) 中完整的进程隔离、审批、超时、输出限制与取消 Gate。

发布门禁：

- 无 broken CTA、placeholder success、静默 Mock 或需要手填内部 UUID 的主流程。
- 所有用户操作都有 loading / success / error / empty 状态。
- 所有 Tauri command 有真实参数反序列化和响应协议契约测试。
- 安全、IPC、runtime 分支覆盖率不低于 90%，整体不低于 80%。
- macOS 真实安装包通过黄金路径；Windows/Linux 未支持能力必须明确禁用。
- Critical/High dependency advisory、secret scan、未脱敏 diagnostics 命中均阻断发布。

## 7. 执行顺序

具体依赖、里程碑、退出条件见 [13-recovery-roadmap.md](13-recovery-roadmap.md)。不要并行“大修所有模块”；先统一契约，再重建对话，再接通安全工具，最后恢复扩展能力。

## 8. 修复闭环台账

实施状态、已通过门禁、残余风险和下一阶段入口统一记录在
[14-closure-ledger.md](14-closure-ledger.md)。模块原始审查文档保留问题基线，
不通过删除原始结论来制造“已修复”的假象。
