# 16. Memory-Conditioned Multi-Agent（松耦合闭环）

> **Product note (2026-07):** Default UX is **single-agent chat + tools**
> (see [`docs/AGENT-PRODUCT-PATH.md`](../../AGENT-PRODUCT-PATH.md)).
> Multi-agent orchestration is a **production** secondary path for real
> multi-role work (composer「多角色协作」), not an experiment and not a peer mode.

## 目标

- 多 Agent **可规划、可执行、可汇总、可学习**（实验能力 / 高级路径）。
- 工程侧依赖 **用户习惯记忆**，而不是预置模板超市。
- **记忆层与编排层不强耦合**：任一侧升级/失败不应拖垮另一侧。
- **产品默认**仍为单 Agent tool loop；本闭环不重新变成用户必选「模式」。

## 解耦架构

```text
app-memory                 app-workflows              src-tauri (composition root)
┌──────────────┐          ┌──────────────────┐       ┌─────────────────────┐
│ PatternStore │          │ PatternSource    │◀──────│ PatternStoreAdapter │
│ (trait+SQL)  │          │ PatternSink      │       │ (only place that    │
│              │          │ (ports only)     │       │  knows both crates) │
│ WorkflowPattern         │ OrchestrationService     └─────────────────────┘
└──────────────┘          │  recall → plan → run → observe
                          └──────────────────┘
```

- `app-workflows` **不依赖** `app-memory`。
- `app-memory` **不依赖** `app-workflows`。
- 缺省 `NoopPatternSource` / `NoopPatternSink`：记忆离线时编排仍可跑（任务信号兜底）。
- 记忆 `apply_outcome` 失败 **不** 回滚编排结果。

## 闭环

1. UI 提交任务 → `start_orchestration`
2. `PatternSource.recall` 取用户/工作区习惯
3. `build_memory_conditioned_plan`：习惯优先，任务信号补全
4. 子 Agent 执行（现有 runtime run + safe tools）
5. 合成摘要 → 写入会话 assistant 消息
6. `PatternSink.observe` 强化/降权模式，或沉淀 Suggested 新模式

## IPC

- `list_agents`
- `recall_workflow_patterns`
- `preview_orchestration_plan`
- `start_orchestration`
- `get_orchestration` / `list_thread_orchestrations`
- `cancel_orchestration`
- `mute_workflow_pattern`

## 后续

- 强模型 refinement：仅在 Plan Schema 内改写，仍经校验
- 按 AgentSpec 强制 tool allowlist
- 编排会话 SQLite 持久化（当前会话索引为进程内 + subagent 行 durable）
- 写隔离 worktree 再开 Worker 真写
