# 修改清单记录

**日期：** 2026-07-20  
**范围：** 多 Agent 里程碑（模版库 / loop / DAG）与后续产品 UX / 稳定性修复  
**仓库：** `portico-desktop`  
**文档位置：** `docs/CHANGELOG-2026-07-20-multi-agent-ux.md`

---

## 1. 概述

本批改动围绕以下目标：

1. **多 Agent 里程碑**：可发现模版库（≥3）、durable `loop` 阶段、可编辑阶段 DAG、执行板一致性  
2. **产品交互收敛**：默认单 Agent「发送」；协作方式自动判定，不再平铺多按钮  
3. **会话与脑图体验**：消息顺序、气泡布局、连线方向、允许路径按文件夹展示  
4. **模型与运行稳定性**：Moonshot/Kimi 端点、失败文案、工具轮次打满后的收尾  

---

## 2. 多 Agent 引擎与存储

### 2.1 阶段模型（`crates/app-models`）

| 项 | 说明 |
|----|------|
| `OrchestrationStageKind::Loop` | 有界循环阶段 |
| 阶段字段 | `body_stage_ids`、`max_iterations`、`stop_flag_path`、`current_iteration` |
| `WorkflowTemplate` / `WorkflowTemplateId` | 可编辑工作流模版；支持 `FromStr` / `Display` |

### 2.2 阶段图（`crates/app-workflows/src/stage_graph.rs`）

| 项 | 说明 |
|----|------|
| 内置模版 ≥3 | `multi-lens-review`、`deep-research`、`iterative-refine` |
| 校验 | `validate_stage_dag`、`apply_stage_edit`、拓扑序、环检测 |
| Loop 逻辑 | `loop_should_stop`、`clamp_loop_max_iterations`（1–32） |
| 计划构建 | `plan_bundled_workflow`、`plan_from_stages`、`plan_adaptive_stage_graph` |

### 2.3 编排服务（`orchestration_service.rs`）

| 项 | 说明 |
|----|------|
| 模版 CRUD | seed 内置、`list/save/get/delete_workflow_template` |
| 启动解析 | catalog key / 模版 UUID / 自适应图 |
| `run_loop_stage` | 按轮执行 body；`pass` 停止或打满 max；每轮 board task |
| **异步启动** | plan 写入 `Running` 后 `tokio::spawn` 执行，**立即返回 session**，避免 UI 卡在「发送中」 |

### 2.4 存储与迁移

| 文件 | 说明 |
|------|------|
| `migrations/0035_workflow_templates.sql` | `workflow_templates` 表 |
| `storage.rs` | 模版 upsert/get/list/delete、按 `catalog_key` 查询 |

### 2.5 Tauri API（`src-tauri/src/commands/orchestrator.rs`）

- `list_workflow_templates` / `save_workflow_template` / `get_workflow_template` / `delete_workflow_template`  
- 既有 `list_bundled_workflows`、`start_orchestration(workflow_id)`  

---

## 3. 前端：模版库 / DAG / 执行板

### 3.1 Schema 与 API

| 文件 | 说明 |
|------|------|
| `schemas.ts` | stage kind 增加 `loop`；loop 字段；`WorkflowTemplate` |
| `tauri-api.ts` | 模版 list/save/get/delete |

### 3.2 纯逻辑与编辑器

| 文件 | 说明 |
|------|------|
| `workflow-dag-model.ts` | draft↔stage、校验、clamp loop；**保留 `output_payload`**（foreach seed） |
| `workflow-dag-editor.tsx` | 模版目录 + 阶段编辑；人话字段标签 |
| `workflow-catalog-copy.ts` | 内置模版中英文案（适合 / 区别） |
| `orchestration-queue.ts` | 排队携带 `workflowId`，drain 不丢模版 |

### 3.3 执行板

| 文件 | 说明 |
|------|------|
| `execution-board-model.ts` | loop 的 `currentIteration` / `maxIterations` / 轮次 tasks |
| `execution-board.tsx` | loop kind 展示 round n/max |

### 3.4 Skeptic 修复（保存后不丢 foreach）

| 问题 | 修复 |
|------|------|
| DAG 保存清空 `output_payload` | draft 全程保留 seed JSON |
| 忙碌排队丢 `workflowId` | `queueMultiRoleTask` + drain 用 `start.workflowId` |

---

## 4. Composer 交互演进

### 4.1 阶段一：合并为「多人协作」菜单

- 去掉平铺：模版库 / 多镜头 / 多角色 三个按钮  
- 单一 **多 Agent ▾** 菜单：自适应 + 目录 + 编辑 DAG  

### 4.2 阶段二：只保留「发送」+ AI 自动选型（当前）

| 项 | 说明 |
|----|------|
| 主按钮 | 仅 **发送** |
| `classify-task-mode.ts` | 规则分类：单人 / multi-lens / deep-research / iterative-refine / 自适应 |
| 轻提示 | 「将使用：…」 |
| 高级入口 | 小号文字「自定义步骤」→ DAG 编辑器 |

**分类示意：**

| 信号 | 模式 |
|------|------|
| 短句 / 寒暄 | 单人对话 |
| 审查 / 多角度 | multi-lens-review |
| 深入摸清 / 架构调研 | deep-research |
| 反复打磨 | iterative-refine |
| 明显多步骤 | 自适应图 |
| 其它 | 默认单人 |

---

## 5. 会话时间线与气泡

| 文件 | 说明 |
|------|------|
| `conversation-event-block.tsx` | 用户右气泡、助手左气泡；工具/错误仍全宽卡片 |
| `conversation-timeline.tsx` | **保留服务端消息顺序**；流式接在末尾；「运行中」只标当前 run 的助手 |
| `event-view-models.ts` | `mapMessageToBlock(message, index)`；同秒用户优先于助手；`sortConversationBlocks` |

### 5.1 消息顺序错乱修复

| 问题 | 原因 | 修复 |
|------|------|------|
| 助手气泡跑到「你好」上面 | `Date.parse` 同秒打平后按 UUID 排 | 用列表 index 作 sequence，保留 API 序 |
| 最后一轮被标到前面 | 旧消息也亮「运行中」 | 仅当前 run 的 assistant 显示运行中 |
| 流式样式/位置错 | 缺 `role`、sequence 用 `Date.now()` | 补 `role: "assistant"`，sequence 接在 durable 后 |

---

## 6. 脑图（Canvas）

| 项 | 说明 |
|----|------|
| 连线箭头 | `MarkerType.ArrowClosed`，from → to 有方向 |
| 实线 / 虚线 | Parent 实线结构；Related 虚线支撑；DerivedFrom 动画 |
| 锚点 | `resolveEdgeHandles`：上下 / 左右按相对位置选 handle |
| 节点 | 四向 in/out handle |
| 图例 | 工具栏「连线说明」：实线=结构，虚线=支撑 |
| 相关文件 | `canvas-view-model.ts`、`nodes/canvas-node.tsx`、`canvas-page.tsx` |

---

## 7. 模型提供商（Moonshot / Kimi）

| 项 | 说明 |
|----|------|
| 国内 / 国际端点 | `api.moonshot.cn/v1` vs `api.moonshot.ai/v1` 可切换 |
| 健康失败文案 | 区分缺密钥、401、模型不存在、超时、区域不匹配 |
| 更新密钥 | 保存后自动重测第一个模型 |
| 预设模型 | 增加 turbo / 128k 等 |
| 相关文件 | `model-provider-presets.ts`、`model-capabilities-panel.tsx`、`provider_factory.rs` |

---

## 8. 运行失败与工具循环

| 项 | 说明 |
|----|------|
| 工具轮次打满（16） | 不再硬失败：无工具总结一轮后 **Completed** 部分结论 |
| 上下文溢出 | `PROVIDER_CONTEXT_OVERFLOW` 专用提示 |
| 通用 Internal | 透出截断细节，避免只显示 unexpected error |
| 相关文件 | `autoagents-adapter/src/executor.rs`、`app-runtime/src/runner.rs` |

---

## 9. 审计：允许路径 → 允许文件夹

| 项 | 说明 |
|----|------|
| 展示维度 | 按**文件夹**一行，不再拆成两段读/写绝对路径列表 |
| 标注 | 项目根 / 可读 / 可写 |
| 聚合 | 同一路径读+写合并为一行双标签 |
| 相关文件 | `allowed-paths-summary-model.ts`、`allowed-paths-summary.tsx` |

---

## 10. 主要新增 / 重点文件一览

### 后端

- `crates/app-workflows/src/stage_graph.rs`  
- `crates/app-workflows/src/orchestration_service.rs`  
- `crates/app-runtime/migrations/0035_workflow_templates.sql`  
- `crates/app-runtime/src/storage.rs`  
- `crates/app-runtime/src/runner.rs`  
- `crates/autoagents-adapter/src/executor.rs`  
- `crates/autoagents-adapter/src/provider_factory.rs`  
- `src-tauri/src/commands/orchestrator.rs`  

### 前端

- `apps/desktop-ui/src/features/agent-client/workflow-dag-*.ts(x)`  
- `apps/desktop-ui/src/features/agent-client/orchestration-queue.ts`  
- `apps/desktop-ui/src/features/agent-client/classify-task-mode.ts`  
- `apps/desktop-ui/src/features/agent-client/execution-board*.ts(x)`  
- `apps/desktop-ui/src/features/agent-client/conversation-*.tsx`  
- `apps/desktop-ui/src/features/agent-client/event-view-models.ts`  
- `apps/desktop-ui/src/features/canvas/canvas-view-model.ts`  
- `apps/desktop-ui/src/features/canvas/nodes/canvas-node.tsx`  
- `apps/desktop-ui/src/features/capabilities/model-*.ts(x)`  
- `apps/desktop-ui/src/features/operations/allowed-paths-summary*.ts(x)`  
- `apps/desktop-ui/src/lib/schemas.ts` / `tauri-api.ts` / `i18n.ts`  

---

## 11. 验证（本批相关）

| 命令 / 范围 | 结果（开发时） |
|-------------|----------------|
| `cargo test -p app-workflows --lib stage_graph` | 通过 |
| `cargo test -p app-runtime --lib -- conversation_prompt_tests` | 通过 |
| `cargo test -p autoagents-adapter --lib` | 通过 |
| vitest：`workflow-dag-model` / `classify-task-mode` / `orchestration-queue` / `event-view-models` / `canvas-view-model` / `allowed-paths-summary-model` 等 | 通过 |
| `tsc --noEmit`（desktop-ui） | 通过 |

---

## 12. 产品路径（当前约定）

```
发送（唯一主按钮）
  ├─ 规则自动选型 → 单 Agent 对话（默认）
  └─ 或 multi-lens / deep-research / iterative-refine / 自适应图
自定义步骤（次要）→ DAG 编辑 → 保存副本 → 启动
执行板 → stage → task（含 loop 轮次）
```

**非目标（本批未做）：** 完整 pi JSON/TUI、自由 IDE 画布、动态代码 stage、双模式聊天信息架构。

---

## 13. 已知后续可改进

1. DAG 编辑器改为**图形化** React Flow（目前仍以表单+人话标签为主）  
2. 会话失败文案中文化更完整  
3. 大任务自动拆步 / 更智能的上下文裁剪  
4. 脑图支撑边在极复杂图上的进一步避让布局  

---

## 14. 修订记录

| 日期 | 说明 |
|------|------|
| 2026-07-20 | 初版：汇总本会话里程碑与 UX/稳定性全部修改 |

---

*本文档由开发会话整理，保存在项目 `docs/` 目录，便于评审与回溯。*
