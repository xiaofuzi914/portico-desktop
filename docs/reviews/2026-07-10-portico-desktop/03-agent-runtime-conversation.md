# 03. Agent Runtime、Conversation 与 Event

## 结论

这是产品核心，也是当前最关键的不可用模块。现有设计把 run 同时当“会话执行容器”和“单轮执行”，导致第一轮完成即 terminal；消息、工具、审批与事件又没有形成可恢复的统一时间线。

## 当前功能与证据

### 单轮后进入死状态

- 页面先手动 `startRun`，再 `submitMessage`：`apps/desktop-ui/src/routes/workspaces/$workspaceId/threads/$threadId/index.tsx:39-75`。
- runtime 拒绝向 terminal run 再提交：`crates/app-runtime/src/runner.rs:359-368`。
- 一次 submit 完成后立刻把 run 标记 `Completed`：`runner.rs:430-460`。
- Composer 只判断是否有 `runId`，不判断 terminal：`conversation-composer.tsx:29-35`。
- 页面显示 control error，却没有展示 submit error：thread route `:91-121`。

所以第一轮后输入框仍可用，但后端必然拒绝，用户也看不到准确原因。

### 历史与事件无法恢复

- 后端缺少 list-runs/get-latest-run/message history 的完整 IPC 表面：`src-tauri/src/commands/run.rs:9-107`。
- 用户消息没有持久化。
- AutoAgents executor 只在最后追加一个 `message_completed`，且 sequence 为 0：`crates/autoagents-adapter/src/executor.rs:141-150`。
- UI 只有 `event_type === "Message"` 才按消息渲染：`event-view-models.ts:129-141`，实际 `message_completed` 被当 diagnostic JSON。
- live 与 persisted events 直接拼接，无稳定 event ID 去重：`event-view-models.ts:168-183`。
- executor 每次只收到当前 message，不加载 thread history、instructions、memory 或 RAG：`executor.rs:52-71`。

### 运行控制并不控制真实执行

- pause/resume 主要修改状态，未证明能暂停正在发生的 provider stream 或工具进程。
- cancel 不能完整传播到 HTTP、shell、MCP 子进程。
- 启动时没有统一 reconciliation，遗留 `Running` / `WaitingApproval` 可能长期悬挂。
- EventBus 是内存通道；真实事件未全部 durable，lag/restart 会造成丢失。

## P0 修复文档

### 1. 固定领域语义

建议：

```text
Thread（对话容器）
  ├─ Message（用户/助手/系统的稳定内容）
  └─ Turn/Run（一次用户输入触发的一次执行）
       ├─ RuntimeEvent
       ├─ ToolInvocation
       ├─ ApprovalRequest
       └─ Artifact
```

- 每次发送创建一个新 run，不复用 terminal run。
- UI 不再要求用户先点击 Start Run。
- retry 创建新 attempt，并通过 `retry_of_run_id` 关联原 run。
- message 与 run 分离：失败的 run 仍保留用户消息与失败原因。

### 2. 设计原子 `send_message` use case

建议接口：

```text
send_message(thread_id, content, optional_model_id, client_request_id)
  1. 校验 thread/workspace/provider/capability/budget
  2. 事务写入 user Message
  3. 创建 Run + initial event
  4. 提交 durable job/outbox
  5. 返回 run snapshot
```

使用 `client_request_id` 保证 UI 重试不会重复创建消息/run。后台 executor 不应在数据库事务内直接调用网络。

### 3. 建立 append-only event journal/outbox

每个 event 至少包含：

- `event_id`
- `run_id`
- `sequence`
- `event_type`
- `schema_version`
- `occurred_at`
- `payload`
- `causation_id` / `correlation_id`

数据库增加 `UNIQUE(run_id, sequence)`。写业务状态与 outbox 在同一事务；event bridge 使用 cursor replay，UI 重连时从最后已确认 sequence 补齐。

### 4. 统一事件 vocabulary 与 reducer

最少支持：

- `user_message_created`
- `assistant_message_started/delta/completed`
- `tool_invocation_requested/started/completed/failed`
- `approval_requested/resolved`
- `artifact_created`
- `run_status_changed`
- `usage_recorded`

前端使用 normalized reducer：delta 聚合到同一 assistant message；live/persisted 以 `event_id` 去重；未知 version 安全降级但不破坏时间线。

### 5. 真实可取消的执行监督器

- 每个 run 有 cancellation token，传入 provider stream、tool invocation、MCP 和 shell process group。
- pause 要么真实背压/挂起，要么在 0.1 删除；不能只改 DB 状态。
- cancel 等待子任务确认退出，再进入 `Cancelled`。
- 启动时把不可恢复的 Running 标为 `Interrupted`；可恢复的 waiting approval 重新 hydrate。
- 状态转移集中在一个 state machine，数据库 update 需带 expected version 做并发控制。

## 优化文档

- `ConversationService` 负责 message/turn；`ExecutionService` 负责运行与工具；不要继续扩展 `PorticoRuntimeHandle` God Object。
- prompt assembler 明确组装 system instructions、thread history、memory、RAG、tool definitions 和 token budget。
- 历史压缩作为可观测步骤，保留被压缩消息范围与摘要来源。
- run snapshot 固化 provider/model、system prompt version、tool allowlist、workspace trust version，保证可审计与可复现。
- 流式 UI 只更新局部 message，不为每个 delta 创建独立卡片。
- 失败分类为可重试 provider/network、用户可修复 validation/permission、内部错误三类。

## TDD 与验收

### 状态机单测

- 所有合法/非法转移 table-driven。
- completed run 永远不可再 submit；新消息自动创建新 run。
- duplicate `client_request_id` 返回原结果，不产生第二条消息。
- cancel/complete、approve/cancel 等竞态只允许一个终态。

### 集成测试

- 事务失败时 message、run、outbox 均不残留。
- event sequence 唯一且连续；断开/重连无丢失、无重复。
- 进程重启后 Running → Interrupted，pending approval 可继续处理。
- mock streaming provider 产生 100 个 delta，UI 最终只显示一条完整 assistant message。

### 桌面 E2E

- 同一 thread 连续发送至少 5 轮，模型收到正确历史。
- 刷新和关闭重开后，user/assistant/tool/approval/artifact/status 与关闭前一致。
- provider 失败可见且可重试；submit 错误不会静默。
- cancel 后 provider、shell、MCP 子任务均停止，且不会晚到覆盖终态。

