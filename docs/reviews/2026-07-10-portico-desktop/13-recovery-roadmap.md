# 13. 恢复路线图与发布 Gate

## 总原则

修复目标不是让当前所有页面“都能点”，而是先建立一条安全、可恢复、可测试的核心用户闭环。每个阶段只有满足退出条件后，下一层能力才允许对用户开放。

## Phase 0：止血与范围收敛

目标：避免用户被虚假 ready/placeholder success 误导，并关闭高风险执行面。

工作：

- feature-flag Terminal、MCP stdio、Browser、Desktop、写 Worktree/Orchestrator。
- 隐藏或重命名 Plugin/Skill install、Migration rollback、Diagnostics upload。
- 禁止 release 静默 Mock；未配置 provider 显示 Setup required。
- 所有 runtime readiness 改由 capability probe 返回。
- 停止未脱敏 diagnostics；迁移/轮换可能泄漏的 secrets。
- 冻结新功能，建立问题 owner 与 ADR 清单。

退出条件：

- 发布 UI 无已知必失败 CTA、假成功、静默 Mock。
- 高风险能力默认不可达。
- 当前用户数据有备份方案。

## Phase 1：统一 IPC 与事实来源

目标：让 UI/后端可以可靠沟通。

工作：

- camelCase command 参数、唯一 response/error envelope。
- Rust → TypeScript/Zod 生成 SDK，清理不存在/未绑定 API。
- 真实 Tauri command contract tests。
- correlation ID 与安全错误映射。

退出条件：

- 100% 注册 command 参数反序列化通过。
- 无 raw/envelope 混用、无手写 IPC key 漂移。
- workspace/thread/settings 的真实 IPC smoke 通过。

## Phase 2：重建 Conversation 核心

目标：真实模型多轮对话、持久化、恢复。

工作：

- 明确 Workspace → Thread → Message → Turn/Run。
- 实现幂等 `send_message` use case。
- ProviderResolver per-run、health/selection、禁止 silent fallback。
- 实现最小 PromptAssembler：system policy + 完整/裁剪后的 thread history + 当前 user message；确保多轮语义真实成立。
- durable event outbox、normalized reducer、cursor replay。
- cancel 与 startup reconciliation。

退出条件：

- 同 thread 5 轮真实/fixture provider 对话。
- 刷新/重启后 message/event/status 一致。
- provider 配置立即生效，实际 model 可审计。
- cancel 后没有晚到执行覆盖终态。

## Phase 3：重建安全工具闭环

目标：安全读取/写入本地代码并可审批继续。

工作：

- ExecutionContext、workspace-scoped RootResolver、中央 PolicyGate。
- durable ToolInvocation + ApprovalBroker。
- Files read/search/write/patch 与 Git status/diff/stage/commit。
- canonical path、symlink/TOCTOU、归属约束和安全 audit。
- artifact store 与受控 preview。

退出条件：

- A workspace 无法访问 B。
- 写文件触发审批，approve 恰好一次、deny 零副作用、重启可继续。
- Agent patch → diff → artifact 的桌面 E2E 通过。
- canary secret 不进入日志/DB/diagnostics。

## Phase 4：上下文与开发 Agent 产品化

目标：完成最小可发布的“本地开发 Agent”。

工作：

- 在 Phase 2 的最小 PromptAssembler 上接入 instructions/memory/RAG、citation 与 context budget。
- WorkspaceIndexer 首次/增量索引与 privacy。
- usage/budget、thread lifecycle、Inspector UX。
- 真 Git worktree；受控 Terminal/PTY 可作为可选子阶段。
- backup/recovery/diagnostics/export。

退出条件：

- Fresh install 黄金路径完整通过。
- 真实项目上能解释代码、生成批准后的 patch、查看引用与 artifact。
- 重启恢复、数据库 migration/backup、安装包 smoke 通过。
- 整体 coverage ≥80%，核心安全/IPC/runtime branch ≥90%。

此阶段才具备 0.1 beta 候选资格。

## Phase 5：逐项恢复扩展能力

每项独立 feature flag、独立威胁模型与验收：

1. HTTP MCP（先于 stdio MCP）
2. stdio MCP sandbox
3. Automations / durable jobs
4. Read-only subagents → isolated write subagents
5. Browser automation
6. Desktop automation
7. Signed Plugin/Skill platform

禁止因为模块有 UI/表/command 就标 ready。

## 必须先定稿的 ADR / 产品决策

1. Run 是否明确为一次 turn；retry/branch 如何表达？
2. active model 是 global/workspace/thread/run 哪些层级？
3. trust、allowed paths、once/run/workspace approval 如何组合？
4. Plugin 是 catalog、目录包还是签名远程执行包？
5. Sensitive memory 是否允许发送外部模型，默认策略是什么？
6. RAG 索引范围、ignore、更新和引用语义是什么？
7. Automation timezone、DST、misfire 和 App 关闭期间补跑策略是什么？
8. Local-first 下 update check、diagnostics upload、telemetry 默认是否关闭？
9. Browser/Desktop 是 Agent tool 还是独立测试台，是否需要保留重复 UI？
10. Windows/Linux 哪些能力是 release supported，哪些明确 unavailable？

## 推荐工作包

### WP-A：Contract Foundation

覆盖文档 01。由一名 owner 统一 command/schema/error；避免多人同时手改 wrappers。

### WP-B：Conversation Foundation

覆盖文档 03、06、11 的核心部分。先写领域状态机和 repository tests，再接 UI。

### WP-C：Execution Safety

覆盖文档 04、05。由安全 owner 审核；任何工具恢复都依赖本包。

### WP-D：Developer Agent UX

覆盖文档 02、07、09。只消费稳定的 A/B/C 接口，不在 UI 绕过 use case。

### WP-E：Delivery Gate

覆盖文档 12；从 Phase 1 开始同步建设，而不是最后补测试。

### WP-F：Extensions

覆盖文档 08、10；Phase 4 退出前只做威胁模型和接口，不开放执行。

## Definition of Done

每个工作包必须同时满足：

- 对应产品行为和非目标有书面 spec/ADR。
- 测试先失败（RED），实现后通过（GREEN），再完成重构。
- unit、integration、contract、关键 desktop E2E 与风险相匹配。
- 代码 review 无 Critical/High；安全面经过专门审查。
- 用户操作有 loading/success/error/empty/recovery。
- 文档、capability probe、UI ready 状态与真实实现一致。
- 没有新增 hardcoded secret、跨 workspace 访问、placeholder success 或静默降级。

## 最终 No-Go / Go Gate

以下任一成立即 No-Go：

- 真实 Tauri IPC 仍有失配。
- silent Mock/fallback 仍可能发生。
- approval 后不能 continuation 或可能重复副作用。
- Terminal/MCP/路径存在已知越权/RCE/SSRF。
- 重启丢失对话/审批/终态。
- clean checkout 无法构建或安装包未 smoke。
- Critical/High advisory、secret canary、未脱敏 diagnostics 未清零。

Go 候选必须现场演示 README 中完整黄金路径，并提供 CI、安装包、数据恢复和安全测试证据。
