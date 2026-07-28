# Portico agent product path

**Status:** product default (2026-07)

## Principles (phase 0)

1. **Main path = single agent + tools** (Codex / Claude Code style). One input box, Enter to send.
2. **Multi-agent is not a peer “mode”** — not a permanent tab that splits the product in two.
3. **Multi-agent is production, not experimental** — used in real work when a task benefits from multiple roles (security + explore, implement + review, etc.).

## What users see (phase 1)

| Surface | Behavior |
|---------|----------|
| **Send** | Default → `send_message` → one `AgentRun` + tool loop |
| **多角色协作** | Production action → `start_orchestration` when the user wants (or accepts a suggestion) |
| Timeline | Parent requests with user text; subagent child runs filtered |
| Suggestion | When the draft looks multi-step, offer multi-role without forcing it |

```text
[  输入框 …                                              ]
[  建议：此任务适合多角色 (explorer → security-reviewer)  ]
[  controls…          [多角色协作]  [发送]               ]
```

## Runtime model (phase 2)

### Default — single agent

```text
User message
  → send_message
  → submit_message
  → AutoAgentsExecutor tool loop (model chooses tools)
```

### When multi-agent is used (real usage)

```text
User chooses「多角色协作」or accepts suggestion
  → start_orchestration (User message on parent run)
  → plan (habits + task signals, max 2 roles)
       · deliverable tasks (PlantUML/图/生成/落地/实现…) → explorer+worker (not planner-only)
       · plan-only only when user says 仅计划/不要执行
  → subagent runs (each is still a tool-loop agent)
  → if user wanted a deliverable but no writer ran → auto worker follow-up
  → synthesize **result-first** summary (交付结果 + 角色摘要)
  → pattern observe (learn habits for next time)
```

**Result-oriented rule:** close the user's ask with a concrete outcome (artifact / paths / answer), not a plan that stops the loop.

Orchestration is a **first-class execution path for hard tasks**, not a lab feature.

## Habits / memory (phase 3)

- Patterns bias **role order and style** when multi-agent runs.
- Single-agent prompt is tool-first and **context-injected**:
  `runner::build_execution_prompt` + `ContextInspector::assemble_prompt_context`
  (AGENTS.md, non-sensitive memories, RAG excerpts).
- `rebuild_rag_index` scans the workspace tree from disk (not re-embed of empty store).
- Memory UI copy: habits support real agent work (single + multi-role), not “experiments only”.
- Safe / product tools (model-callable):
  - `fs_list`, `fs_read`, `fs_search`, `fs_edit`, `fs_write`
  - `git` — `status`/`diff` free; `add`/`commit` **Ask** approval
  - `shell_exec` — project shell after **Ask** approval (command denylist still applies)
  - `web_search`, `web_fetch` — public HTTP(S); localhost/private blocked
  - **MCP** — tools from 能力中心 after refresh; write tools **Ask**
- Still not agent tools: interactive Browser Use page, Skills invoke, Automations scheduler, native desktop automation.
- Multi-agent sessions are **durable** in SQLite `orchestrations`.

## Do not

- Force every message through multi-agent.
- Call multi-agent “experimental” in product UI.
- Make “对话 | 多 Agent” equal dual modes as the primary IA.

## Canvas view (project work map)

**Not a second chat mode.** Canvas is a spatial view over project work:

| Surface | Path | Purpose |
|---------|------|---------|
| Project canvas | `/workspaces/$id/canvas` | Organize chats (heuristic insights), decompose goals into stages, launch stages |
| Session mind map | Thread `?view=mindmap` | Thread-scoped canvas over the same graph engine |

- **Organize chats** → `extract_canvas_insights` (heuristic, conclusion-first; max 5 insights/thread; hierarchical layout without overlap; incremental column reuse)
- **Break down goal** → template stages (调研 → 方案 → 实现 → 验证), prompts include related canvas insights; related edges link insights → stages
- **Layers** → All / Chats / Goals filters on the same graph
- **Launch stage** → creates a session + `send_message` or `start_orchestration`, marks stage `InProgress`, navigates into chat

Default product path remains single-agent chat. Canvas launches re-enter that path.

**Closed loop (product):**
1. Organize chats → conclusion insights + layout  
2. Decompose goal → stages linked to insights  
3. Launch stage → new session + run; node `InProgress`  
4. Run completes/fails → stage auto `Done`/`Blocked` (and goal Done when all stages Done)  
5. Insight → open session + scroll/highlight source message  
6. Re-open canvas → reconcile any still-InProgress stages against latest run status

## Related code

- UI: `apps/desktop-ui/src/features/agent-client/conversation-composer.tsx`
- Canvas UI: `apps/desktop-ui/src/features/canvas/`
- Flag: `apps/desktop-ui/src/lib/feature-readiness.ts`
- Single path: `crates/app-runtime/src/runner.rs`
- Multi path: `crates/app-workflows/`
- Extract / goal: `crates/app-workflows/src/canvas_extract.rs`, `canvas_goal.rs`
- Tool loop: `crates/autoagents-adapter/src/executor.rs`
