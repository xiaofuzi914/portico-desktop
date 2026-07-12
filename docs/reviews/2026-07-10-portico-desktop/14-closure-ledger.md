# 14. 修复闭环台账

更新时间：2026-07-10  
工程判定：**Go for macOS user acceptance**  
对外发布判定：**No-Go，等待真实 Developer ID 公证签名、CI 三平台运行结果和用户黄金路径验收**

维护规则：功能必须同时具备明确能力边界、实现证据和自动验证；需要真实凭据、OS 交互或发布账号的事项不能用 mock 冒充完成。

## 本次可验收产品范围

- Workspace 创建、trust 与 canonical read/write allowlist。
- Provider/Model 配置、Keychain secret、连接测试、健康状态、Global/Workspace/Thread active selection 和 run snapshot。
- Durable conversation：消息、单活动 turn、取消、错误可见、五轮历史、重启恢复。
- 安全工具黄金路径：`fs_read`、审批后的 atomic `fs_write`、`git status/diff`。
- Durable approval：等待、拒绝、并发双批准 exactly-once、取消、进程重启与 effect reconciliation。
- Settings diagnostics：只导出静态日志省略说明和 aggregate audit count。

Terminal、Git mutation、MCP、Browser/Desktop、Artifact 写操作、Automation 与多 Agent 执行继续关闭。Usage/Budget 展示与 IPC 已关闭，因为真实 token/cost 尚未接入；不能把零值当成计费证据。

## 已闭环

### Conversation、Provider 与模型

- `send_message` 单事务持久化 User Message + Queued Run，并通过 `client_request_id` 幂等。
- Assistant/System failure message 幂等；同一 thread 只允许一个活动 turn；取消不会被迟到响应覆盖。
- PromptAssembler 按 run 边界组装历史并施加消息数/字符预算。
- Provider 不再回退 mock；每次 run 动态解析 active selection，要求 24 小时内 `Ready` 健康结果。
- Provider URL/API key 变化会使健康结果失效；run 开始前保存不可变 provider/model/config snapshot。
- OpenAI 使用 Responses API；OpenAI-compatible Provider 使用 Chat Completions；401/429/timeout 分类且错误脱敏。

### Workspace、安全与 durable tool

- Workspace root 必须绝对、存在、可读并 canonicalize；拒绝 `/`、home、临时根、系统根、重复 root 与 symlink escape。
- 后端从数据库解析 workspace/thread/trust/revision，调用方不能伪造 trusted context。
- `ExecutionContext`、`PolicyGate`、请求 SHA-256 指纹、durable `ToolInvocation`、approval 和 checkpoint 已进入 SQLite。
- invocation + approval 同事务创建；批准/拒绝原子决策；lease CAS 保证并发批准只有一个执行者。
- `WaitingApproval` 是正常 run 状态；批准后 tool result 回灌同一 checkpoint 并继续同一 run。
- `Executing` 重启后进入 `NeedsReconciliation`；文件写使用 pre/post hash、同目录临时文件、sync、atomic rename 与 recovery receipt，禁止盲重放。
- legacy AutoAgents tool wrapper、Browser/Desktop 后端模块和未注册高风险 Tauri command 模块不再进入生产编译；模型只能看到三项已审查 schema。

### IPC、diagnostics 与凭据

- 当前前端使用的 Tauri 参数统一为 camelCase，统一解包 `ApiResponse<T>`。
- 真实 `tauri::test` MockRuntime IPC 覆盖 workspace/thread/provider/secret validation/pending approval/migration/diagnostics，不只是前端 invoke mock。
- 原始 `evaluate_permission` 和 scaffold `greet` IPC 已移除；授权只能从 run-scoped durable path 进入。
- API key reference/value 在 Tauri 边界校验；空值、空白、控制字符和超限 secret 被拒绝。
- diagnostics 不复制 app log，不序列化 audit resource/prompt，只写 allowlisted metadata；arbitrary canary 不进入导出文件。
- provider、embedding、MCP HTTP 错误不再把远端响应正文原样写入错误；默认文件日志级别由 TRACE 降到 INFO。
- 虚假 diagnostics upload 和占位 updater 公钥/endpoint 已删除。

### 可复现 Build 与 Release

- AutoAgents 从 `../AutoAgents` sibling path 改为公开 Git revision `7d4f71b2…` 精确锁定；干净 checkout 不再依赖本机目录结构。
- pnpm 固定为 `10.6.0`，CI 使用 frozen lockfile；Rust MSRV 提升到 1.88。
- CI/release 增加 Linux Tauri 系统依赖、Rust fmt/clippy/test/audit/coverage、前端 format/lint/typecheck/test/coverage/audit、tracked-secret scan 和独立 macOS native WebView E2E job。
- release artifact glob 修正到 workspace 根 `target/release/bundle/**`。
- macOS 资源显式包含 PNG/ICNS/ICO；本地脚本使用 ad-hoc identity 构建验收 DMG。
- release CI 要求非空 Developer ID，并对 DMG 内 `.app` 执行 icon、codesign、Gatekeeper 和 stapler 硬校验。

## 自动验证证据

| Gate | 结果 |
|---|---|
| `pnpm typecheck` | 通过 |
| `pnpm lint` | 通过 |
| `pnpm format:check` | 通过 |
| `pnpm test:coverage` | 21 files / 161 tests；受测前端逻辑 statements 96.06%、branches 88.78%、functions 86.29%、lines 96.06% |
| `cargo test --workspace` | 全 workspace 通过；包含 provider HTTP fixture、durable approval/tool/restart、workspace sandbox、真实 Tauri IPC 与 diagnostics canary |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 |
| Rust coverage | release-critical `app-runtime + app-security + autoagents-adapter` lines 80.03%；raw whole-workspace lines 74.63%（声明式模型、平台 bootstrap 与 thin Tauri shell 未纳入 80% release gate） |
| `pnpm audit --prod --audit-level high` | No known vulnerabilities |
| `cargo audit --ignore RUSTSEC-2023-0071` | 0 个可达 vulnerability；唯一 ignore 属于 SQLx 未启用 MySQL feature 的 optional RSA，`cargo tree --target all -i rsa@0.9.10` 必须为空 |
| Secret scan | production source 无疑似长 token/private key；测试 canary 通过拆分构造，不形成静态假阳性 |
| HTTP Provider fixture | 真实本地 TCP/HTTP：五轮历史、401、429、timeout、取消不重试 |
| Clean checkout | `/private/tmp/portico-clean-*` 无 sibling AutoAgents：`cargo metadata --locked`、`cargo test --workspace --locked`、frozen pnpm install、直接 tsc/ESLint/Prettier/Vitest 全部通过 |
| Native WebView E2E | `pnpm tauri:e2e:desktop` 通过；两次独立 macOS 原生进程经随机端口 W3C WebDriver 操作可见 UI：动态核对全部 migration、生成 diagnostics、创建带 nonce 的 Provider；重启后 Provider 可见且 SQLite 精确唯一落盘；真实 IPC 还逐项拒绝 Git mutation/Terminal/MCP/Browser/Desktop/Automation/Subagent 命令 |
| E2E/生产隔离 | WebDriver server 只在 `desktop-e2e` feature、专用 bundle identifier、内存 secret store、安全 marker 数据目录与 `target/desktop-e2e` 独立产物目录下编译；测试进程只继承最小环境变量。普通 feature graph 无该插件，production capability 未增加测试权限；release 对 Windows PE、Linux ELF、macOS 全部 Mach-O 执行 WebDriver 标识负向扫描 |
| macOS DMG | `pnpm tauri:verify:macos:local` 完成 checksum、只读挂载、ICNS、strict codesign 与全部 Mach-O 的 WebDriver 缺失扫描；最终 DMG SHA-256 `0e0d6c01b3cbc7cf6b9cafeae575244dcbd602e1f9fb49b4018141dec2bf8157`（8,941,993 bytes） |
| Kimi Code 2.7 独立复核 | 识别 diagnostics、updater、artifact path、签名和 E2E 风险；主流程逐项复核并落实 |

说明：RustSec 仍报告 Tauri 的 Linux GTK3 依赖及 `urlpattern` build-time 依赖的 unmaintained/unsound warning；它们不是当前 macOS binary 的已知可达漏洞，但 Windows/Linux 对外发布前必须由真实平台 CI 结果和上游升级策略重新评估。

## 仍需外部完成的发布门禁

1. 用户按 [15-user-acceptance.md](15-user-acceptance.md) 在 production DMG、真实 Keychain 和真实 Provider 凭据下完成人工 macOS 黄金路径；隔离 E2E 构建已有 WebView/IPC/Provider 持久化证据，但不能外推成 production 数据目录验收。
2. 发布账号提供 Developer ID 与 notarization 凭据；通过 `codesign`、`spctl`、`stapler`。本地 ad-hoc DMG 不能对外分发。
3. GitHub CI/release matrix 在 Linux、Windows、macOS 三平台实际跑绿并产出对应安装包；本机只证明 Apple Silicon macOS。

以上三项需要用户凭据或远端平台，当前代码不能安全代替。它们完成前保持 distribution No-Go。

## 2026-07-12 核心闭环补齐（agent mode）

在既有 macOS 安全黄金路径之上，完成第一性原理缺口的垂直贯通：

| 闭环 | 实现要点 |
|---|---|
| 知识 → 模型 | `assemble_prompt_context` 注入 instructions / 非敏感 memory / RAG；`build_execution_prompt` 接入 |
| 索引 | `index_workspace_from_disk` + `rebuild_rag_index` 服务端解析 workspace root 后扫盘 |
| 工具面 | `fs_search`、`fs_edit` 进入 durable SafeTool + PolicyGate + recovery（`file_edit_v1`） |
| 契约 | 注册 `inspect_context` / `search_rag` / `rebuild_rag_index` / `load_instructions` / `get_feature_capabilities` |
| 多 Agent | `orchestrations` 表 upsert/list/get；进程内 SessionIndex 移除 |
| 止血 | Automation ticker/worker 停跑；feature readiness 与能力矩阵对齐 |

## 后续能力重新开放条件

- Usage/Budget：接入 provider 实际 token/price 响应、幂等 usage record 与对账后再展示。
- Terminal/Git mutation：命令 AST、PTY 生命周期、效果 receipt、取消与 crash reconciliation 全部通过。
- MCP/Plugin：安装包签名、权限 manifest、env secret injection、SSRF/RCE sandbox 与 server lifecycle 全部通过。
- Browser/Desktop：平台权限、窗口/坐标模型、确定性 element handle、敏感输入保护和 E2E 全部通过。
- Automation：IPC 注册 + 产品化前不得重启 ticker；child scope 不得超过 parent，durable outbox/cursor、预算、取消传播和合并冲突全部通过。
