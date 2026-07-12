# 10. Automation、Background Tasks、Subagents 与 Notifications

## 结论

这些模块有表、command 和页面骨架，但缺少可靠的任务生命周期、恢复、取消与结果关联。当前更像管理后台原型，不应被当作无人值守自动化平台。

## 当前功能与证据

### Automation

- UI 默认 All workspaces，把 `null` 传给 `listAutomations()`：`apps/desktop-ui/src/features/operations/automation-summary.tsx:34-44`。
- 后端要求非 Optional workspace ID：`src-tauri/src/commands/automation.rs:15-22`，首屏调用即可能失败。
- 前端 `Automation` schema 缺少后端 `permission_policy`，update DTO 可能反序列化失败。
- update 不重算 `next_run_at`：`crates/app-workflows/src/scheduler.rs:92-99`。
- scheduler spawn 执行后很快返回：`scheduler.rs:117-145`；worker 把“已启动 run”当作 task completed，而不是等待 run terminal。
- permission policy 被存储但没有进入真实执行上下文。

### Background Tasks

- worker 只真正处理部分 task kind；其他 kind 可能以“task kind not handled”仍标成功：`src-tauri/src/lib.rs:207-230`。
- `recover_stalled` 有实现/测试，却没有在生产启动/循环中调用：`crates/app-workflows/src/task_queue.rs:139-146`。
- cancel 主要改数据库状态，不向关联 run/provider/process 传播。
- 缺少 handler registry、heartbeat、lease renewal、idempotency 和结果 deep link。

### Subagents / Orchestrator

- 规划主要依靠英文关键词选择 role：`crates/app-workflows/src/orchestrator.rs:281-320`。
- role system instructions/tool allowlist 没有完整传入 executor；executor 暴露整个 tool registry：`crates/autoagents-adapter/src/executor.rs:157-183`。
- child run 复用 parent thread/root，假 worktree 不提供隔离：`orchestrator.rs:224-260`。
- summary 查找事件名与真实 persisted event vocabulary 不一致，可能始终为空。
- 前端没有实际使用 orchestrator API 的完整流程。

### Notifications

- 通知主要依赖 run 状态；现场 0 run，因此无有效产品证据。
- Bell/query 缺少稳定 event invalidation/polling 和 deep link。
- test notification wrapper 与后端 workspace 参数存在契约失配。
- category/内部状态直接暴露，缺少用户语义和去重。

## P1 修复文档

### 1. 统一 Durable Job 模型

```text
Job
  id / kind / workspace_id / source
  idempotency_key
  payload_version / permission_snapshot
  status / attempt / lease_owner / lease_expires_at / heartbeat_at
  linked_run_id
  result / error_code / correlation_id
```

- handler registry 明确每种 kind；未知 kind 必须 Failed，不得 Completed。
- claim 使用原子 lease；执行中 heartbeat；启动和定时循环调用 recover stalled。
- handler 返回终态结果；若启动 run，必须等待 run terminal 再结算 job。
- cancel 传播到 run 和全部子资源。

### 2. 修正 Automation 契约与调度语义

- `listAutomations(workspaceId?:)` 真正支持全局列表，或 UI 强制选择 workspace；两端一致。
- 生成 DTO，消除 `permission_policy` schema 漂移。
- create/update 都验证 cron、timezone，并重算 `next_run_at`。
- 明确定时区、DST、App 关闭期间 misfire policy（skip/run once/catch up）。
- 每个 tick 生成稳定 idempotency key，防止重复触发。
- 自动化运行使用创建/最后确认时的 permission policy snapshot；高风险能力不能继承无限权限。

### 3. Subagent 先收缩为受控 RunSpec

在真实 worktree/allowlist 前禁用写编排。恢复时每个 child 需要：

- role/system prompt version
- explicit input/output schema
- tool allowlist 与 permission cap
- model/budget/time limits
- isolated workspace/worktree root
- parent/child cancellation
- typed result 与 provenance

用明确 DAG/step plan 代替关键词路由；并发受 repo/worktree/resource lock 约束。

### 4. Notifications 只由 durable event 产生

- outbox consumer 根据 run/job/approval/artifact 终态生成 notification，具备 dedupe key。
- notification 保存 workspace/thread/run/artifact deep link。
- UI 订阅事件或短轮询后 invalidates cache，支持批量已读。
- OS notification 尊重用户权限和 privacy preview 设置。

## 优化文档

- Operations 页使用 workspace selector，不显示内部 UUID 输入框。
- task 展示排队/运行/等待审批/重试/失败/完成，展开能看关联 run 和安全错误。
- automation 提供 Run now、next 5 occurrences preview、last result、pause 与 retry。
- child agent 只在单 Agent 黄金路径稳定后开放；先做串行、只读任务，再做并发写任务。
- 以 structured result 合并 child 输出，不从自由文本事件猜 summary。

## TDD 与验收

### Scheduler / Queue

- 假时钟覆盖 timezone、DST、misfire，同一 cron tick 只触发一次。
- App 在 claim 后崩溃，lease 过期可恢复且不重复副作用。
- run 失败/取消时 job 对应失败/取消；只有 run terminal success 才 completed。
- cancel task 能终止关联 provider/shell/MCP。

### Automation 产品

- All workspaces 首屏成功，或在无选中 workspace 时明确引导，不发错误请求。
- 修改 cron/timezone 后 next run 立刻正确更新。
- Run now 返回可点击 thread/run，错误可见。

### Subagent

- child 只能看到 allowlist 工具与隔离 root。
- parent cancel 会取消所有 children；budget/time limit 生效。
- typed result 与 event vocabulary 一致，summary 不再为空。

### Notification

- run 完成或 approval waiting 后 1 秒内出现且只出现一次。
- 点击进入正确 workspace/thread/run/artifact；重启后未读状态仍正确。

