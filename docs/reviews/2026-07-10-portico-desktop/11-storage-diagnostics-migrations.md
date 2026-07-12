# 11. Storage、Migrations、Diagnostics 与 Recovery

## 结论

SQLite 基础配置和 migration 框架能工作，但存储层已经成为大而宽的 God Repository；业务事件、归属约束、启动恢复、备份修复和诊断脱敏尚未达到桌面产品要求。

## 当前功能与证据

### Storage

- SQLite 启用了 foreign keys、WAL 与 busy timeout：`crates/app-runtime/src/storage.rs:332-366`，这是可靠的基础。
- `storage.rs` 超过 2000 行，承担几乎所有领域 CRUD 并暴露 pool，事务边界散落。
- `run_events` 缺少足够的 `UNIQUE(run_id, sequence)` 约束；真实 executor 又只写 sequence 0。
- thread `updated_at` 没有可靠地随 message/turn 更新。
- artifacts 等表有 migration，但缺少生产 CRUD/producer。
- 部分 memory/RAG/usage/audit 表的外键、唯一约束或生命周期约束不足。
- `start_run` 的 workspace/thread 归属验证不足，可能组合两个分别存在但不属于彼此的实体。

### Recovery

- 启动没有统一检查遗留 Running/WaitingApproval/leased jobs/worktrees。
- background `recover_stalled` 没有生产调用。
- 没有明确的数据库 backup、restore、integrity check、repair 与安全降级 screen。
- Tauri composition root 大量 `expect`/同步 bootstrap；历史三份 macOS crash report 均为 `SIGABRT`，栈包含启动阶段 `portico_tauri::run`。这不能单独定位当前 bug，但证明 panic 式启动曾造成真实崩溃。

### Migrations

- migrations 能在当前数据库执行到 23 个，运行日志有证据。
- 设置页公开 “Rollback last migration”，后端却明确是 placeholder：`src-tauri/src/commands/migration.rs:58-67`。
- 没有面向用户数据的升级前备份、失败恢复和 repair 流程。

### Diagnostics

- diagnostics upload 是模拟成功/placeholder：`src-tauri/src/commands/diagnostics.rs:92-133`。
- bundle 复制原始 app log，但只对 audit summary 做 redaction：`diagnostics.rs:34-76`。
- 日志路径发现可能与实际 macOS 日志位置不一致，bundle 可能漏收真实日志或收占位内容。
- 设置 `redacted=true` 不能证明每个文件安全。

## P1 修复文档

### 1. 拆分 Repository 与事务 use case

按领域定义窄接口：

- WorkspaceRepository
- ConversationRepository
- RunEventRepository / OutboxRepository
- ApprovalRepository
- ArtifactRepository
- Automation/JobRepository
- Memory/IndexRepository
- Audit/UsageRepository

Application service 拥有事务边界；adapter 不能随意拿 pool 写跨领域状态。先围绕 send message、approval、artifact、job completion 四个关键事务拆分。

### 2. 补完整性约束

- `UNIQUE(run_id, sequence)`、event ID 唯一。
- message/run/tool/approval/artifact 通过 FK 绑定 thread/workspace，必要时用 service + trigger/复合 key 强制归属。
- idempotency key 对 send message、automation tick、job、usage 唯一。
- 关键 update 使用 version/expected status，避免竞态覆盖终态。
- 明确 cascade/restrict；删除 workspace 默认归档，不盲目级联用户可见历史。

### 3. Staged Bootstrap + RecoveryService

启动阶段：

1. 打开日志与 crash marker。
2. 找到数据库并备份。
3. integrity check 与 migration。
4. 初始化 repositories/secret store/provider capability。
5. reconcile runs、approvals、jobs、worktrees/outbox。
6. 启动 event bridge/workers。
7. 返回 Ready/Degraded/Repair Required。

非必要模块失败进入 degraded；数据库/migration 失败显示可操作 repair screen，而不是 `expect` abort。

重启策略：

- 无法续跑的 Running → Interrupted。
- WaitingApproval 保留并重新显示。
- 过期 lease 回队列或失败，取决于幂等性。
- outbox cursor 从 durable checkpoint 恢复。

### 4. 安全 migration 与 backup

- migration 前生成带版本、hash 的本地备份，验证可打开。
- migration 在事务内执行；不可事务化步骤有明确 journal/恢复方案。
- UI 只展示版本/状态/导出诊断，不提供必失败的 rollback 按钮。
- 真正 rollback 只有在每个 migration 都有经过测试的 down migration 和数据兼容策略后开放。

### 5. 重做 Diagnostics

- MVP 只提供 Collect、Preview、Export、Show in Finder；删除假 Upload success。
- 从统一 tracing 配置获取真实日志位置。
- 每个候选文件先分类、限大小、脱敏、secret canary scan，再进入 bundle。
- UI 展示将导出的文件、大小、时间范围和被遮罩字段；用户可取消单项。
- 未来上传需明确 endpoint、TLS、用户同意、进度、失败和删除策略。

## 优化文档

- 结构化 tracing 全链路携带 workspace/thread/run/tool/correlation，但敏感字段不进入 span。
- 为 SQLite 增加 schema version、app version、last clean shutdown、repair history。
- 定期 checkpoint/backup 受大小和保留策略约束，提供用户控制。
- 对大 event/audit/terminal history 设置 retention/compaction；核心 message/artifact 引用不被误删。
- 建立 migration fixture：从每个已发布版本升级到当前，而不只测试空数据库。

## TDD 与验收

### Storage

- 跨 workspace/thread 组合写入失败。
- duplicate event sequence/idempotency key 被约束，不产生双重副作用。
- 关键 use case 注入故障后事务完全回滚。

### Migration / Recovery

- 空库、每个旧版本 fixture、迁移中断、磁盘满、corrupt DB 均有测试。
- 迁移失败可从已验证备份恢复；不会启动到半迁移状态。
- 模拟进程在 Running/WaitingApproval/leased job 时退出，重启状态符合策略。

### Diagnostics

- canary API key、MCP env、Authorization、terminal secret 不出现在 bundle。
- 找不到日志时明确报告 missing，不生成假日志。
- 发布 UI 不再有 placeholder rollback/upload success。

