# 05. Files、Git、Terminal 与 Worktree

## 结论

文件与 Git 是 Portico 作为本地开发 Agent 的核心能力，应该优先修复；Terminal 与 Worktree 当前安全性和语义都不足，应先关闭。四者必须共享 workspace-scoped root 和 ExecutionContext。

## 当前功能与证据

### Files / Git 跨 workspace

- `StorageRepoRootProvider` 返回全部 workspace roots：`crates/app-runtime/src/repo_root_provider.rs:28-36`。
- filesystem read/list/search 虽接收 workspace ID，却只验证路径是否落在任一 root：`crates/app-tools/src/filesystem.rs:57-83,180-280`。
- filesystem write permission 硬编码 trusted：`filesystem.rs:331-340`。
- Git 同样使用全局 roots，并构造不可靠的 trust context：`crates/app-tools/src/git.rs:131-171`。

结果是 A workspace 的操作可能访问 B workspace；若用户创建 root `/`，边界可扩大到全盘。

### Terminal 绕过安全层

`crates/app-tools/src/terminal.rs:60-143`：

- 使用 `sh -c` 执行任意字符串。
- cwd 由调用方提供，没有 canonical workspace 校验。
- 不调用 PermissionEngine；默认 shell deny 实际被绕过。
- denylist 可被绝对路径、wrapper、重定向、命令替换等绕过。
- 无 PTY、超时、输出上限、可靠取消和子进程组清理。
- `.output()` 等待并全量缓冲，可能挂死或耗尽内存。

### Worktree 是假隔离

- `WorktreeManager` 只 `create_dir_all` / remove dir：`crates/app-runtime/src/worktree.rs:40-63`。
- `/private/tmp/portico-wt-test` 和 `/private/tmp/portico-orchestrator-test` 的结果不是 Git repo。
- 因此 UI/编排显示 worktree 并不代表代码被隔离。

### 产品层问题

- Files Inspector 实际主要是 Git status/diff，不是文件树。
- Terminal Inspector 是命令表单 + history polling，不是终端 session；默认 cwd 还可能为空。
- Git 独立页要求用户手填 root/UUID，违背当前 workspace 上下文。
- mutation 错误在多个页面没有统一反馈。

## P0 修复文档

### 1. Workspace-scoped RootResolver

- 所有工具只通过 `workspace_id` 后端解析 canonical root；不接受 UI 传 root 作为权限依据。
- root、cwd、target、symlink target 全部 canonicalize 并验证仍处于当前 workspace 的允许范围。
- 对创建目标使用父目录 canonicalization + 安全 open；考虑 symlink/TOCTOU 置换。
- 每次调用带 workspace/thread/run/correlation，并验证归属。
- allowed read/write paths 分开；写入默认 Ask。

### 2. 先完成文件与 Git 黄金能力

Files：

- read/list/search 使用大小、文件数、二进制和 ignore 限制。
- write/patch 前生成 diff preview，通过 ApprovalBroker 后原子写入。
- 使用临时文件 + fsync/rename，失败不留下截断文件。

Git：

- status/diff 为 read action；stage/commit/checkout/branch/write 操作为 Ask 或明确 grant。
- Git repo 必须位于当前 workspace/worktree root。
- commit message、pathspec 使用结构化参数，避免 shell。
- 脏工作树、冲突、detached HEAD 有稳定错误码与 UI 恢复建议。

### 3. Terminal 暂停开放并重建

P0 期间从 UI 与 agent tool registry 移除直接 shell。

重新启用要求：

- `TerminalSession` 固定 workspace/thread/run/canonical cwd。
- 优先结构化 argv；需要 shell grammar 时必须单独审批完整脚本。
- sanitized env，secret 通过临时受控注入且不进入日志。
- PTY/streaming stdout-stderr、输出 ring buffer、超时、空闲超时。
- 进程组 cancellation，确保子孙进程退出。
- 命令 policy 作为补充风险分类，不再把 denylist 当沙箱。

### 4. 实现真实 Git worktree adapter

- 使用 `git worktree add` 创建、`git worktree remove` 清理，并维护 registry。
- 创建前验证 repo、branch/ref、目标目录和并发占用。
- run 的 root/cwd/tool context 绑定 worktree，而非 parent workspace root。
- 清理失败进入可恢复状态，不直接递归删除未知目录。
- 启动时 reconcile registry 与 `git worktree list --porcelain`，提供 prune/repair。

## 优化文档

- 文件树按需加载，搜索与 Git status 使用增量 watcher，而不是全量轮询。
- diff viewer 支持按 hunk 审批和应用；artifact 与文件修改共享 preview 组件。
- Terminal session metadata 可恢复，但原始敏感输出按 retention policy 存储或不持久化。
- 为 Git mutation 提供 undo/恢复指引；不要自动执行 destructive reset/clean。
- worktree 资源由 supervisor 管理，支持配额、磁盘空间预检、父子 run 引用计数。
- Inspector 自动使用当前 workspace/thread 上下文，不要求用户输入内部 ID。

## TDD 与验收

### 路径安全

- A workspace 不能 read/write/search/git B。
- 覆盖 `..`、absolute path、symlink、symlink swap、大小写/Unicode 差异、目标不存在时的父目录逃逸。
- `/`、home、app data 和不存在路径无法成为普通 workspace root。

### Terminal

- 未审批 shell 不启动进程。
- cwd 越界、绝对路径 wrapper、换行、`&&/||/&`、重定向、substitution 有 table/fuzz 测试。
- 无限进程会超时；大输出受限；取消后子孙进程全部退出。

### Git / Worktree 集成

- 在临时真实 repo 中完成 status → diff → stage → commit；写动作均走 approval。
- 创建 worktree 后 `git rev-parse --is-inside-work-tree` 为 true，分支隔离真实有效。
- 多 run 并发不能绑定同一可写 worktree；崩溃后可 reconcile/repair。

### 产品 E2E

- Agent 读取当前项目文件、提出 patch、用户预览并批准、diff 正确展示。
- Terminal `pwd` 默认是当前 workspace/worktree；长任务可停止。
- 全流程不要求用户填写 root、workspace UUID 或 run UUID。

