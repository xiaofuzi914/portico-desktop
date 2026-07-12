# 07. Memory、Instructions、Context 与 RAG

## 结论

当前模块能存取部分 memory、instructions 和 chunks，也有 Inspector 页面，但这些数据没有进入真实 Agent prompt；新 workspace 又没有文件索引入口。因此它目前是独立数据面板，不是 Agent 能力。

## 当前功能与证据

- context inspector 能组装 instructions/memory/RAG 摘要：`crates/app-runtime/src/context.rs:88-147`，command 在 `src-tauri/src/commands/memory.rs:131-150`。
- AutoAgents executor 只使用本次 message，未注入上述 context：`crates/autoagents-adapter/src/executor.rs:52-71`。
- `index_document()` 有内部实现：`context.rs:149-210`，但没有生产 command、文件 watcher 或首轮 workspace scan。
- “Rebuild index”只重嵌入已有 chunks：`context.rs:265-305`；空 workspace 永远重建 0 个。
- RAG search 在一些错误路径返回空结果：`context.rs:217-244`，用户无法区分“没有相关内容”和“索引/embedding 故障”。
- command 接受调用方提供的 `workspace_root`：`commands/memory.rs:108-149`，没有统一由 workspace ID 后端解析。
- Sensitive memory 仍以明文存 SQLite；context summary 包含内容，只增加 privacy flag。
- 前端 memory query key 未完整包含 scope/thread/run，切换 scope 可能复用错误缓存。
- embedding provider 不可用时可静默退化到 hash embedding，状态对用户不透明。

## P1 修复文档

### 1. PromptAssembler 成为唯一入口

每次 run 创建 immutable prompt snapshot：

```text
system policy
→ workspace/project instructions
→ relevant thread history / summary
→ allowed memories
→ RAG citations
→ user message
→ tool definitions / constraints
```

记录每部分的 token 数、来源 ID、版本与截断原因，但不把敏感原文写普通日志。

### 2. 重建 scope 与 privacy 模型

- Workspace memory 必须绑定 workspace。
- Thread memory 必须绑定 thread，并验证 thread 属于 workspace。
- Run memory 必须绑定 run。
- Global memory 必须有清晰用户控制，默认不跨 workspace 注入敏感信息。
- `Sensitive` 默认不发送到外部 provider；需要逐项或按 workspace 显式 opt-in。
- UI 默认遮罩 sensitive 内容，搜索与 diagnostics 也遵循同样规则。

### 3. 实现 WorkspaceIndexer

首次建立索引：

1. 后端从 workspace ID 解析 canonical root。
2. 遵守 `.gitignore`、Portico ignore、allowed read paths。
3. 排除 `.git`、依赖目录、build artifacts、secret/key 文件、二进制和超大文件。
4. 分批读取、chunk、embed、事务写入，展示进度与可取消状态。
5. 记录 path、content hash、mtime、index version、embedding provider/model/dimension。

之后通过 watcher/显式 refresh 增量更新；删除文件时删除旧 chunks。

### 4. 让失败可见

- `EmptyIndex`、`NoMatches`、`EmbeddingUnavailable`、`IndexStale`、`PermissionDenied` 分开返回。
- 禁止 search 吞错后伪装空数组。
- hash embedding 只能是显式 local fallback，并在 capability/status 中显示。
- 重建任务进入 background task，报告 scanned/indexed/skipped/failed 及原因。

## 优化文档

- 检索采用 lexical + vector hybrid，结果保留文件路径、行范围、score 与 index version。
- 给模型的 RAG 片段必须带 citation；UI 能从回答定位原文件。
- context budget 使用可解释策略：最近对话、用户 pin、instruction 优先，RAG 按 relevance/novelty。
- 支持 project instructions 继承和冲突预览，不允许任意 workspace 文件成为高权限 system prompt。
- 对 prompt injection 敏感文件做来源隔离与“不可信内容”标注。
- index schema/version 变化时增量迁移或明确要求 rebuild。

## TDD 与验收

### Scope / Privacy

- 四种 scope 只返回正确记录；跨 workspace/thread/run 请求被拒绝。
- sensitive memory 默认不出现在 provider request、日志、diagnostics 或未解锁 UI。
- query key 包含 scope + entity ID，切换后不会串缓存。

### Indexer

- 新 workspace 选择 README 后能够检索其片段，而不是 rebuild 0。
- 修改、删除、rename 文件后索引正确增量变化。
- `.gitignore`、secret 文件、binary、oversized、symlink escape 被跳过并说明原因。
- embedding 中断可重试且不留下半版本索引。

### Agent 集成

- fixture provider 收到选定 instructions/history/memory/RAG，token budget 与顺序稳定。
- 回答 citation 能打开正确文件/行。
- embedding 故障显示 degraded，不默默变成“无相关内容”。

