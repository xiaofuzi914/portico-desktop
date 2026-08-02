# Multi-Agent × DeepSeek 成熟度 9+ 闭环清单

> **For Claude / Grok:** 实现时按 Phase 顺序推进；每 Phase 有独立验收门禁。  
> 关联：`docs/AGENT-PRODUCT-PATH.md`、`docs/reviews/.../16-memory-conditioned-multi-agent.md`

**Goal:** 把「角色 tool 硬隔离」「DeepSeek thinking + Flash/Pro 分阶段选型」「超时/部分失败可重试与进度 UX」打成**一条产品闭环**，使多 Agent 产品闭环、可靠性、DeepSeek 深度适配均达到 **≥9/10**。

**Architecture:** 在现有 `OrchestrationService` + `AutoAgentsExecutor` + `RegistryExecutorResolver` 上扩展 **Run 级执行规格（`RunExecutionSpec`）**：每个子 Agent/阶段 run 携带 `tool_allowlist`、`model_policy`、`thinking_mode`、`timeout_budget`、`retry_policy`。规格在 plan 阶段写入 durable plan，执行期强制生效，UI 全程可观测，失败可定点重试。

**Tech Stack:** Rust (`app-models` / `app-workflows` / `autoagents-adapter` / `app-runtime` / `src-tauri`) + React (`agent-client` / `capabilities`) + SQLite migrations + Vitest / Cargo test。

**当前基线（约 2026-08）：**

| 维度 | 现分 | 9+ 目标 |
|------|------|---------|
| 多 Agent 产品闭环 | ~7.5 | ≥9.0 |
| 多 Agent 可靠性 | ~6.5 | ≥9.0 |
| DeepSeek 可用性 | ~8.0 | ≥9.0 |
| DeepSeek 深度适配 | ~5.5 | ≥9.0 |

---

## 0. 整合原则（三点如何合成一条闭环）

不要做成三个互不相干的 PR 堆叠。统一以 **「一次编排会话 = 可恢复的阶段状态机」** 为轴：

```text
用户意图
  → 分类/模版/自适应 plan
  → 为每个 stage/role 生成 RunExecutionSpec
       ├─ (A) tool_allowlist  （角色硬边界）
       ├─ (B) model_id + thinking_mode  （DeepSeek 分阶段策略）
       └─ (C) timeout_ms + retry_class  （失败可解释/可重试）
  → 异步执行（每 stage 进度 durable）
  → 部分失败 → PartialCompleted + 可重试单元
  → 合成摘要（含失败角色/重试历史）
  → Pattern observe（含模型策略与失败类信号）
  → UI：进度 / 重试 / 模型标签 / 工具边界可见
```

**闭环定义：** 任意一条用户任务，从「发送」到「结果或可操作失败」之间：

1. 有 durable 状态（重启不丢）；
2. 有可观测进度（阶段/角色/模型/工具/超时）；
3. 有安全边界（角色不能越权用工具）；
4. 有智能资源分配（Flash 探路 / Pro 交付，thinking 可控）；
5. 有恢复动作（单 stage 重试、跳过、改单 Agent 续跑）；
6. 有学习回写（成功/失败模式进 habit）。

缺任一项，不算 9 分。

---

## 1. 目标成熟度评分卡（验收用）

### 1.1 多 Agent 产品闭环 ≥9

| # | 验收项 | 通过标准 |
|---|--------|----------|
| P1 | 默认仍是单 Agent | 普通短句绝不自动多角色 |
| P2 | 多角色可发现、可解释 | 发送前可见「将用哪些角色/模版/模型策略」 |
| P3 | 角色 tool 硬隔离 | explorer 无法 `fs_write`/`shell_exec`（advertise + gate 双拒绝） |
| P4 | Worker 写路径安全 | 写角色在 worktree 或 Ask；禁止静默写主工作区（策略可配置但默认 fail-closed） |
| P5 | 执行板 = 真相源 | stage/task/subagent 状态与 SQLite 一致，2s 内反映 |
| P6 | 结果优先合成 | 有交付物时摘要以交付为主，附角色失败说明 |
| P7 | 习惯学习 | 成功/失败 outcome 进入 pattern；recall 影响下次角色与模型策略 |

### 1.2 多 Agent 可靠性 ≥9

| # | 验收项 | 通过标准 |
|---|--------|----------|
| R1 | 子 run 超时分级 | soft timeout 可续 / hard timeout 标记 Failed+原因，不卡死 session |
| R2 | 部分失败不整单作废 | 一角色失败 → `PartialCompleted`，其它结果仍合成 |
| R3 | 定点重试 | UI/API 可重试失败 stage 或 subagent，不重跑已完成 |
| R4 | 取消可传播 | cancel 后 pending stage → Skipped，running → cancel_run |
| R5 | 进程崩溃恢复 | 重启后 Interrupted/Running 可 reconcile，不盲目重放副作用 |
| R6 | 进度事件 | 至少：Planning / StageStarted / StageProgress / StageFailed / StageRetried / Completed |
| R7 | 超时预算可配置 | 按角色 class 默认不同；provider timeout 与 subagent budget 协调 |

### 1.3 DeepSeek 可用性 ≥9

| # | 验收项 | 通过标准 |
|---|--------|----------|
| D1 | 一键就绪 | 预设 + Key + health Ready 后即可跑 tool loop |
| D2 | 模型 ID 官方对齐 | `deepseek-v4-pro` / `deepseek-v4-flash` 可健康检查与运行 |
| D3 | 错误可行动 | 401/404 model/timeout/429 中文+修复指引 |
| D4 | base URL 规范化 | 接受 `api.deepseek.com` 与带 `/v1` 变体；错误 URL 在 health 明确提示 |
| D5 | 无静默 Mock | 未配置 provider 时明确 `PROVIDER_UNAVAILABLE` |

### 1.4 DeepSeek 深度适配 ≥9

| # | 验收项 | 通过标准 |
|---|--------|----------|
| S1 | Thinking 模式可控 | `Off` / `On` / `Auto` 写入 run snapshot 并真正影响请求 |
| S2 | 分阶段模型策略 | explorer/research 默认 Flash；reduce/worker/doc 默认 Pro（同 provider 内） |
| S3 | 策略可覆盖 | thread/stage 可强制某模型；override 写进 snapshot |
| S4 | 多 Agent 成本可见 | 执行板展示每 stage 使用的 model + thinking |
| S5 | Tool call 与 thinking 共存 | thinking On 时 tool loop 仍稳定；失败有降级（关 thinking 重试一次） |
| S6 | Embedding 可选 | 若 DeepSeek 提供 embeddings 则可用；否则不阻塞主路径 |

**综合 9+ 门槛：** 上表 **P/R/D/S 全部通过**，且下列黄金路径 E2E 全绿（见 §7）。

---

## 2. 领域模型：统一 `RunExecutionSpec`

### 2.1 新增类型（`crates/app-models`）

```rust
/// How a single agent run should execute (tools + model + budget).
pub struct RunExecutionSpec {
    pub role: String,                         // explorer | worker | ...
    pub allowed_tools: Vec<String>,           // product tool names: fs_read, ...
    pub denied_tools: Vec<String>,            // optional explicit denylist
    pub model_policy: ModelSelectionPolicy,
    pub thinking_mode: ThinkingMode,          // Off | On | Auto
    pub timeout_ms: u64,                      // hard budget for this sub-run
    pub soft_timeout_ms: Option<u64>,         // optional early warning
    pub max_tool_steps: u32,
    pub retry_class: RetryClass,              // Transient | IdempotentOnly | Never
    pub write_isolation: WriteIsolation,      // None | PreferWorktree | RequireWorktree
}

pub enum ThinkingMode { Off, On, Auto }

pub enum ModelSelectionPolicy {
    /// Use thread/workspace active model as-is.
    InheritActive,
    /// Prefer a speed/cost tier within the same provider family.
    PreferTier { tier: ModelTier },
    /// Pin exact model id (must belong to configured provider).
    PinModel { model_id: ModelId },
    /// Pin by provider model_name string (resolved at run start).
    PinModelName { model_name: String },
}

pub enum ModelTier {
    Fast,    // DeepSeek Flash, mini, turbo...
    Strong,  // DeepSeek Pro, flagship
    Balanced,
}

pub enum RetryClass {
    /// Network/timeout/429 — safe to retry whole stage if no side effects committed
    Transient,
    /// Only retry if no durable tool side effects recorded
    IdempotentOnly,
    Never,
}

pub enum WriteIsolation {
    None,
    PreferWorktree,
    RequireWorktree,
}

pub enum OrchestrationStatus {
    // existing...
    /// Some stages failed but usable partial results exist
    PartialCompleted,
}
```

### 2.2 挂载点

| 对象 | 字段 | 用途 |
|------|------|------|
| `AgentDefinition` | `allowed_tools` 改为 **product 名**；新增 `model_tier` / `thinking_default` / `timeout_class` | 角色静态规格 |
| `OrchestrationStage` | `execution_spec: Option<RunExecutionSpec>`（或拆字段） | plan 时固化 |
| `OrchestrationStageTask` | `attempt` / `last_error_code` / `retry_count` | 重试 UX |
| `SubagentRun` | `child_run_id`（已有链路）+ `execution_spec` 快照 | 审计 |
| `RunModelSnapshot` | `thinking_mode` / `selection_reason` | 可解释 |
| `AgentRun` / storage | 可选 `execution_spec_json` | 恢复 |

### 2.3 角色 → 工具映射（硬表，唯一真相）

**只允许 product tool 名**（与 `PorticoToolRegistry::is_durable_builtin` + MCP 注册名一致）。废弃 `filesystem.read` 等旧名。

| 角色 | 允许工具 | 写隔离 | 默认 tier | 默认 thinking |
|------|----------|--------|-----------|---------------|
| `default` | fs_list, fs_read, fs_search, git(status/diff), web_search, web_fetch, memory 相关只读 | None | Balanced | Auto |
| `explorer` | fs_list, fs_read, fs_search, git(status/diff), web_search, web_fetch | None | **Fast** | **Off**（快扫） |
| `planner` | fs_list, fs_read, fs_search, memory.search | None | Strong | On |
| `researcher` | fs_*, git read, web_* | None | Fast | Auto |
| `reviewer` | fs_read, fs_search, git(status/diff) | None | Strong | On |
| `security-reviewer` | 同上 + web_fetch（查 CVE 等） | None | Strong | On |
| `tester` | fs_read, fs_search, shell_exec(**Ask**), git read | PreferWorktree | Fast | Off |
| `worker` | fs_*, git(add/commit Ask), shell_exec(Ask) | **RequireWorktree**（可信 workspace 可降为 Prefer） | **Strong** | Auto |
| `doc-writer` | fs_read, fs_write, fs_edit, fs_list | PreferWorktree | Strong | Off |
| `reduce`/`synthesizer`（阶段角色） | **无工具** 或仅 fs_read | None | **Strong** | On |

MCP：默认 **不** 注入多角色子 Agent；若产品要开，仅 `worker` 且写 MCP 仍走 Ask。单 Agent 主路径保留 MCP。

### 2.4 git 子命令细粒度

`git` 是单 tool 多 action：allowlist 层用 **capability tag**：

- `git:read` → status/diff/log
- `git:write` → add/commit（仍 Ask）
- `git:destructive` → 永不进角色表（force push 等全局拒绝）

PolicyGate 在 `git` 调用时检查 action 是否在角色 capability 内。

---

## 3. 闭环 A — 角色 Tool 硬隔离

### 3.1 执行链路改造

```text
plan stage
  → AgentRegistry.built_in(role) → RunExecutionSpec
  → store on stage/task

run_subagent / stage task
  → runtime.submit_message_with_spec(run_id, prompt, spec)
  → RegistryExecutorResolver.resolve_with_spec(...)
  → PorticoToolRegistry.filtered_view(spec.allowed_tools)
  → AutoAgentsExecutor.llm_tools() 仅广告允许集
  → PolicyGate / SafeToolExecutor：不在 allowlist → ToolDenied (ROLE_TOOL_DENIED)
```

**双保险：**

1. **Advertise 层**：模型看不到禁用工具（减少胡调）。  
2. **Gate 层**：即使模型 hallucinate 工具名，也拒绝并回写错误结果，不执行副作用。

### 3.2 关键文件

| 动作 | 路径 |
|------|------|
| 角色表 + 旧名迁移 | `crates/app-workflows/src/agent_registry.rs` |
| 规格构建 | 新：`crates/app-workflows/src/execution_spec.rs` |
| 子 Agent 提交 | `crates/app-workflows/src/orchestrator.rs`、`orchestration_service.rs` |
| resolve 带 spec | `crates/autoagents-adapter/src/provider_factory.rs` |
| filtered registry | `crates/autoagents-adapter/src/tool_adapter.rs` |
| executor 拒绝 | `crates/autoagents-adapter/src/executor.rs` |
| PolicyGate | `crates/app-runtime/src/tool_execution.rs`、`safe_tools.rs` |
| runner API | `crates/app-runtime/src/runner.rs`、`executor.rs` |
| 类型/bindings | `crates/app-models` + regenerate |
| 测试 | `app-workflows` 单元；`safe_tool_golden_path` 扩展角色拒绝；adapter 过滤测试 |

### 3.3 验收用例（必须自动化）

1. `explorer` allowlist 不含 `fs_write` → `llm_tools()` 无 `fs_write`。  
2. 强制 invoke `fs_write` → `ROLE_TOOL_DENIED`，磁盘无变更。  
3. `worker` 可 `fs_write` 但在未信任 workspace 触发审批/拒绝（既有策略）。  
4. MCP 工具不出现在 explorer 广告列表。  
5. 单 Agent（无 multi-role spec）行为与现网一致（全量 safe tools + MCP）。

### 3.4 闭环检查项（A）

- [ ] 角色表只使用 product tool 名  
- [ ] plan 阶段每个 stage 写入 `execution_spec`  
- [ ] advertise + gate 双拒绝  
- [ ] 审计日志含 `role` + `denied_tool`  
- [ ] 前端执行板展示「本角色可用工具」折叠信息  
- [ ] 文档 `AGENT-PRODUCT-PATH.md` 更新角色工具表  

---

## 4. 闭环 B — DeepSeek thinking + Flash/Pro 分阶段选型

### 4.1 模型策略解析器

新模块建议：`crates/autoagents-adapter/src/model_policy.rs`（或 `app-runtime`）

```text
resolve_model(spec.model_policy, workspace, thread, provider_kind):
  InheritActive → 现有 resolve_active_model
  PreferTier(Fast|Strong|Balanced):
    若 active provider 为 DeepSeek:
      Fast   → deepseek-v4-flash（同 provider 下已注册则用之，否则 catalog 匹配）
      Strong → deepseek-v4-pro
      Balanced → 跟随 active；若 active 是 Flash/Pro 之一则保持
    若 Moonshot/OpenAI/...: 映射到该 provider 的 mini/flagship 启发式
    若无法映射 → InheritActive + selection_reason="tier_fallback_active"
  PinModel / PinModelName → 校验归属后使用
```

**同 provider 内选型，不跨供应商静默跳转**（避免 Key 不匹配）。

### 4.2 Thinking 模式

DeepSeek V4：thinking / non-thinking 双模式（官方文档）。

实现策略：

1. 在 `build_llm_provider` / chat 请求路径增加 `thinking_mode` 参数（具体字段跟 AutoAgents OpenAI 兼容层能力对齐；若 upstream 仅支持 extra body / header，集中在 adapter 一处）。  
2. `ThinkingMode::Auto` 规则：
   - reduce / planner / security-reviewer / Strong tier → On  
   - explorer / Fast tier / 工具密集扫描 → Off  
   - worker 实现阶段 → Off（降延迟）；worker 自检/说明 → On 可选  
3. 降级：`TOOL_LOOP_UNSTABLE` 或 provider 报 thinking 冲突 → 自动 `Off` 重试 **一次**，snapshot 记 `thinking_degraded=true`。

### 4.3 UI / 配置

| 面 | 行为 |
|----|------|
| 能力中心 DeepSeek | 展示 Flash/Pro；可选默认 thinking；Login 仍可手贴 Key |
| Thread 模型选择器 | 可固定模型；显示「多角色时将按阶段覆盖 tier」hint |
| 多角色预览 | 列表：`explorer → Flash · thinking off` / `reduce → Pro · thinking on` |
| 执行板 stage 行 | badge：model 短名 + 💭/— |
| 设置（可选 advanced） | 「多角色模型策略：自动分层 / 全部跟随当前模型」 |

### 4.4 关键文件

| 动作 | 路径 |
|------|------|
| preset 保持官方 ID | `apps/desktop-ui/.../model-provider-presets.ts` |
| provider factory + thinking | `crates/autoagents-adapter/src/provider_factory.rs` |
| chat 参数 | `crates/autoagents-adapter/src/executor.rs` |
| DeepSeek URL 规范化 | `provider_factory.rs` + health message |
| 策略解析 | 新 `model_policy.rs` |
| stage plan 写入 tier | `stage_graph.rs`、`execution_spec.rs`、`memory_plan.rs` |
| snapshot 扩展 | `app-models` `RunModelSnapshot` + migration 如需列 |
| i18n | `apps/desktop-ui/src/lib/i18n.ts` |
| 选择器 UI | `thread-model-selector*.tsx`、`execution-board*.tsx`、`conversation-composer.tsx` |

### 4.5 验收用例（B）

1. DeepSeek provider + 两模型已注册时，adaptive plan 中 explorer stage 解析到 flash，reduce 到 pro。  
2. 用户 Pin `deepseek-v4-pro` 且策略为「全部跟随」时，所有 stage 用 Pro。  
3. thinking On 的纯文本 reduce 成功；thinking On 的 tool loop 失败时自动 Off 重试一次。  
4. health check 对 `https://api.deepseek.com` 与 `.../v1` 均可通过（规范化后）。  
5. snapshot 含 `selection_reason`（如 `tier:fast@deepseek`）。

### 4.6 闭环检查项（B）

- [ ] `ThinkingMode` 进领域模型 + TS bindings  
- [ ] DeepSeek tier 映射表单测  
- [ ] 分阶段模型真实出现在 `run_model_snapshots`  
- [ ] 执行板可见 model/thinking  
- [ ] thinking 降级可观测、不静默  
- [ ] 文档说明 Flash/Pro 何时用  

---

## 5. 闭环 C — 超时 / 部分失败 / 重试 / 进度 UX

### 5.1 超时预算分层

| 层级 | 默认 | 说明 |
|------|------|------|
| Provider HTTP | 120s（已有） | 单次 LLM HTTP |
| Soft sub-run | 角色表：explorer 90s / worker 180s / reduce 120s | 触发「即将超时」事件，可续 budget 一次 |
| Hard sub-run | soft × 1.5（上限 300s，对齐 `DEFAULT_RUN_TIMEOUT`） | 取消 child run，标记 Failed |
| Stage wall | sum(children) + 缓冲 | 防 foreach 爆炸；foreach 并行上限 N=3 |
| Orchestration wall | 15–20 min | 全局 cap，防僵尸 |

协调规则：

- `subagent_hard_timeout_ms <= provider_timeout * expected_rounds` 仅作告警，不自动改 provider。  
- 超时错误码统一：`SUBAGENT_TIMEOUT` / `STAGE_TIMEOUT` / `ORCH_TIMEOUT` / `PROVIDER_TIMEOUT`。

### 5.2 部分失败状态机

```text
Running
  → 所有必需 stage Completed → Completed
  → 可选 stage 失败但核心交付存在 → PartialCompleted
  → 必需 stage 失败且无交付 → Failed
  → 用户取消 → Cancelled
```

「必需」启发式：

- `worker` / `doc-writer` / reduce 交付 stage：必需（当 task `wants_deliverable`）  
- 额外 reviewer：可选  
- explorer：deliverable 任务中为必需前置；纯问答可为可选  

### 5.3 重试 API

```text
retry_orchestration_stage(orchestration_id, stage_id)
retry_orchestration_task(orchestration_id, stage_id, task_id)
```

规则：

1. 仅 `Failed` / `Cancelled`（非用户全局 cancel）可重试。  
2. `retry_count < max_retries`（默认 2）。  
3. `RetryClass::IdempotentOnly`：若 child run 已有 durable 写工具成功记录 → 拒绝自动重试，要求用户确认「强制重试」。  
4. 重试创建 **新 child run**，旧 run 保留审计；task.attempt++。  
5. 成功后重新跑下游依赖（仅 `depends_on` 边指向且 status 非 Completed 的，或标记 stale 的 reduce）。

### 5.4 进度事件

扩展 `RuntimeEvent` / orchestration 轮询 DTO：

```ts
type OrchestrationProgress = {
  id: string
  status: OrchestrationStatus
  percent: number // 粗算 completed_tasks / total_tasks
  current_stage_id?: string
  stages: Array<{
    id: string
    title: string
    agent_name: string
    status: string
    model_name?: string
    thinking_mode?: string
    attempt: number
    error_code?: string
    error_message?: string
    started_at?: string
    updated_at?: string
    tasks_completed: number
    tasks_total: number
  }>
  can_retry_stage_ids: string[]
  result_summary?: string
}
```

UI：

- Composer 运行条：百分比 + 当前角色 + 取消  
- Execution board：stage 时间线、失败红条、「重试此步」「跳过并合成」「改用单 Agent 续写」  
- 超时文案：区分网络超时 vs 角色预算超时，并给建议（换 Flash、缩小任务、单 Agent）

### 5.5 关键文件

| 动作 | 路径 |
|------|------|
| 状态枚举 | `app-models` `OrchestrationStatus` |
| 执行/finalize | `orchestration_service.rs` |
| 超时 | `orchestrator.rs` `SUBAGENT_TIMEOUT` → 按 spec |
| 重试 command | `src-tauri/src/commands/orchestrator.rs` |
| tauri-api | `apps/desktop-ui/src/lib/tauri-api.ts` + schemas |
| board UX | `execution-board.tsx`、`execution-board-model.ts` |
| composer 进度 | `conversation-composer.tsx` |
| i18n | `i18n.ts` |
| 恢复 | `app-runtime` recovery + e2e restart path |

### 5.6 验收用例（C）

1. 模拟 explorer 超时 → stage Failed + 其它 stage 仍可完成 → PartialCompleted + 可重试 explorer。  
2. 重试 explorer 成功 → reduce 自动重跑或标记需重跑并执行。  
3. cancel 中途 → pending Skipped，无新副作用。  
4. 重启 app：Running orchestration reconcile 为 Interrupted 或续跑策略明确（二选一写进文档，推荐：标记 Interrupted + UI「继续」）。  
5. foreach 3 路并行，1 路失败 → 正确百分比与 can_retry。

### 5.7 闭环检查项（C）

- [ ] `PartialCompleted` 全链路（存储/IPC/UI）  
- [ ] 分层超时 + 错误码  
- [ ] 定点重试 API + 幂等保护  
- [ ] 进度 DTO 与执行板  
- [ ] 崩溃恢复策略产品化（不是 silent drop）  
- [ ] E2E：超时 → 重试 → 完成  

---

## 6. 三点交汇：端到端黄金路径

### 路径 G1 — 审查类（multi-lens）

1. 用户贴 Key，DeepSeek Ready，active 任意 Flash/Pro。  
2. 输入「请做代码审查并给风险清单」。  
3. 自动选 `multi-lens-review`。  
4. plan 展示：plan(Flash/Off) → foreach reviewers(Pro/On, 只读工具) → reduce(Pro/On, 无写工具)。  
5. 运行中执行板显示百分比与模型 badge。  
6. 故意 kill 一个 reviewer 超时 → PartialCompleted，可重试该 lens。  
7. 重试后 Completed；摘要含风险清单；pattern 学习「审查类 → multi-lens」。

### 路径 G2 — 交付类（adaptive）

1. 「实现 X 并写测试」。  
2. adaptive：explorer(Flash, 只读) → worker(Pro, 写工具+worktree)。  
3. explorer 不可写文件（注入攻击/幻觉工具被拒）。  
4. worker 写文件走隔离/审批。  
5. 合成「交付结果」含路径。  

### 路径 G3 — DeepSeek thinking 降级

1. 强制 stage thinking On + 多 tool。  
2. 若失败，自动 Off 重试一次并在 UI 显示「已关闭深度思考并重试」。  
3. snapshot `thinking_degraded=true`。

### 路径 G4 — 单 Agent 回归

1. 短句「hi」仍单 Agent，全工具（含 MCP 若已连）。  
2. 不引入多角色超时/allowlist 副作用。

---

## 7. 分 Phase 实施清单（建议 PR 切分）

### Phase 0 — 规格与类型地基（0.5–1d）

- [ ] 新增 `RunExecutionSpec` / `ThinkingMode` / `ModelTier` / `PartialCompleted`  
- [ ] `ts-rs` bindings 导出  
- [ ] 角色表迁移到 product tool 名 + tier/thinking/timeout 元数据  
- [ ] 单测：角色表快照  
- [ ] 文档：本清单 §2 冻结为 ADR 级约定  

**门禁：** `cargo test -p app-models -p app-workflows` 相关新测通过。

### Phase 1 — 闭环 A 硬隔离（1–2d）

- [ ] `execution_spec.rs` 从角色生成 spec  
- [ ] `filtered_view` + gate `ROLE_TOOL_DENIED`  
- [ ] `submit_message` / resolve 贯通 spec（单 Agent 传 full allowlist）  
- [ ] orchestration 创建 child 时绑定 spec  
- [ ] 审计字段  
- [ ] UI：角色工具只读展示  
- [ ] 测试：§3.3  

**门禁：** explorer 无法写盘；单 Agent 回归绿。

### Phase 2 — 闭环 B 模型策略 + thinking（1–2d）

- [ ] `model_policy` 解析器  
- [ ] DeepSeek URL 规范化  
- [ ] thinking 请求参数 + Auto 规则 + 一次降级重试  
- [ ] `RunModelSnapshot` 扩展  
- [ ] Composer 预览 + 执行板 badge  
- [ ] 测试：§4.5  

**门禁：** G1 plan 模型分层正确；thinking 降级可测。

### Phase 3 — 闭环 C 可靠性与 UX（2–3d）

- [ ] 分层超时  
- [ ] `PartialCompleted` + finalize 合成含失败说明  
- [ ] `retry_orchestration_stage` IPC  
- [ ] Progress DTO + board/composer  
- [ ] 崩溃恢复策略（Interrupted + Continue）  
- [ ] 测试：§5.6 + 前端 vitest  

**门禁：** 超时→重试→完成；取消干净。

### Phase 4 — 交汇硬化与 E2E（1–2d）

- [ ] G1–G4 自动化（能 mock LLM 的用 fixture；macOS E2E 至少 G1 进度+重试 UI 烟测）  
- [ ] 覆盖率：workflows/adapter 变更行 ≥80%  
- [ ] 更新 `AGENT-PRODUCT-PATH.md`、`16-memory-conditioned-multi-agent.md`  
- [ ] 成熟度评分卡自测勾选全部 ✅  

**门禁：** 评分卡全过 → 宣布 9+。

### Phase 5 — 打磨（可选同迭代）

- [ ] foreach 并行度与公平调度  
- [ ] 成本/ usage 按 stage 聚合展示  
- [ ] 「全部跟随当前模型」高级开关  
- [ ] Worker RequireWorktree 产品文案与创建引导  

---

## 8. 测试矩阵（最低集）

| 层 | 必测 |
|----|------|
| Unit | 角色 allowlist 快照；tier 映射；thinking Auto；timeout class；DAG retry 依赖 |
| Adapter | filtered tools；ROLE_TOOL_DENIED；thinking degrade；DeepSeek default URL/model |
| Workflows | partial complete；retry stage；cancel；spec 写入 plan |
| Runtime | submit_with_spec；snapshot 持久化；恢复 Interrupted |
| UI | classify 仍默认 single；board retry 按钮；badge 文案；进度条 |
| E2E | 配置 DeepSeek mock 或录制 fixture：多角色进度 + 失败重试 |

---

## 9. 明确非目标（本迭代不做，防止 scope 爆炸）

- 跨云厂商自动 failover（OpenAI 挂了切 DeepSeek）  
- 真实多进程 OS 级沙箱  
- 完整 Browser Use 进多角色工具集  
- Automations 调度器产品化  
- 为每个角色训练独立微调模型  
- 实时 token 流式计费结算（可先展示 model，usage 后续）

---

## 10. 风险与缓解

| 风险 | 缓解 |
|------|------|
| AutoAgents 对 DeepSeek thinking 字段支持不足 | adapter 集中适配；不支持则 feature-detect + UI 标明「当前仅 Off」但不阻塞 Flash/Pro |
| 硬隔离导致 worker 外角色「太傻」 | 交付类任务强制 worker follow-up（已有 needs_execution_followup，保留并加强） |
| 重试放大费用 | max_retries=2；UI 确认；PartialCompleted 允许用户停止 |
| Worktree 强制影响 UX | 可信 workspace 默认 Prefer；不可信 Require |
| 计划字段膨胀 | `execution_spec` JSON 列优于散落 20 个 nullable 列 |

---

## 11. 完成定义（DoD）— 成熟度 9+ 签字栏

当且仅当：

1. §1 评分卡 P1–P7、R1–R7、D1–D5、S1–S6 **全部勾选**；  
2. §6 G1–G4 **全部通过**（自动化 + 手测记录）；  
3. 默认产品路径仍是单 Agent；多角色为增强路径；  
4. 无静默 Mock、无角色越权写盘、无「失败即整单空白」；  
5. README / AGENT-PRODUCT-PATH 与实现一致。

则更新基线分：

| 维度 | 目标 |
|------|------|
| 多 Agent 产品闭环 | **9.2** |
| 多 Agent 可靠性 | **9.0** |
| DeepSeek 可用性 | **9.3** |
| DeepSeek 深度适配 | **9.0** |

---

## 12. 建议实现顺序（一句话）

**先类型与角色表（Phase 0）→ 硬隔离（A）→ 模型/thinking（B）→ 超时重试 UX（C）→ 黄金路径 E2E（交汇）**。  
A 是安全底线，B 是 DeepSeek 体感，C 是可靠性体感；三者共享 `RunExecutionSpec`，避免返工。

---

## 13. 快速对照：现有代码锚点

| 能力 | 今日锚点 | 改造后 |
|------|----------|--------|
| 角色定义 | `app-workflows/agent_registry.rs` | product tools + tier/thinking |
| 编排 | `orchestration_service.rs` | spec + partial + retry |
| 子 Agent 跑 | `orchestrator.rs` `run_subagent` | timeout from spec + submit_with_spec |
| 工具广告 | `tool_adapter.rs` `llm_tools` | `filtered_view` |
| 模型解析 | `provider_factory.rs` `RegistryExecutorResolver` | `resolve_with_spec` + tier |
| 单 run 超时 | `runner.rs` 300s / orchestrator 240s | 分层 budget |
| UI 分类 | `classify-task-mode.ts` | 预览 spec 摘要 |
| 执行板 | `execution-board*.tsx` | progress + retry + model badge |
| DeepSeek preset | `model-provider-presets.ts` | 保留 V4 ID + thinking 说明 |

---

**文档状态：** 已实现主闭环（2026-08-02 落地）。

### 实现摘要（已合并进代码）

| Phase | 状态 | 关键落点 |
|-------|------|----------|
| 0 类型地基 | ✅ | `RunExecutionSpec` / `ThinkingMode` / `ModelTier` / `PartialCompleted` in `app-models` |
| 1 工具硬隔离 | ✅ | `agent_registry` product tools · `filtered_for_allowlist` · `ROLE_TOOL_DENIED` · `bind_run_execution_spec` |
| 2 模型分层 + thinking | ✅ | `pick_model_for_tier` · DeepSeek Flash/Pro · thinking prompt directive · snapshot metadata |
| 3 超时/部分失败/重试 | ✅ | per-role timeout · `PartialCompleted` · `retry_orchestration_stage` · board retry UX |
| 4 文档 | ✅ | `AGENT-PRODUCT-PATH.md` 已更新 |

### 补全轮（第二轮，已落地）

| 缺口 | 状态 |
|------|------|
| thinking 失败自动 Off 重试 + event/snapshot | ✅ |
| soft + hard 超时（软超时续跑一次） | ✅ |
| `OrchestrationProgress` + `get_orchestration_progress` | ✅ |
| `Interrupted` + 启动 reconcile + `continue_orchestration` | ✅ |
| RequireWorktree 硬失败 | ✅ |
| selection_reason / thinking_* 持久化 snapshot 列 | ✅ migration 0038 |

剩余非阻断：foreach 并行度调优、usage 按 stage 聚合、thinking 走 DeepSeek 原生 API 字段（当前为 prompt 指令层 + 降级）。
