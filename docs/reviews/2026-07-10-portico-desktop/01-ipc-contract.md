# 01. IPC、类型与错误协议

## 结论

当前 IPC 是全产品最先要修的横切层。前端 mock 测试通过不能抵消真实 Tauri 参数反序列化与返回 envelope 的系统性失配。**在本模块完成前，不应继续逐页面修按钮。**

## 当前功能与证据

### 参数命名失配

Tauri command 宏默认把 Rust 参数暴露为 lower camelCase；当前 command 没有全局 `rename_all = "snake_case"`。本机依赖实现位于 `tauri-macros-2.5.2/src/command/wrapper.rs:47-51,460-466`。

前端 `apps/desktop-ui/src/lib/tauri-api.ts` 仍大量传入 snake_case，覆盖 run、Git、terminal、memory、MCP、automation、browser、desktop 等模块，例如：

- run：`tauri-api.ts:243-267`
- Git：`tauri-api.ts:152-220`
- memory/context：`tauri-api.ts:353-442`
- automation/background：`tauri-api.ts:519-574`
- browser/desktop：`tauri-api.ts:610-667`

当前 dirty working tree 中，用户已经把 thread/provider/model 的一部分参数改为 camelCase；这是正确方向，但只覆盖少量 command。现有测试的其他断言仍在固化错误的 snake_case 合约。

### 返回 envelope 失配

前端 `invokeCommand()` 在 `tauri-api.ts:72-83` 强制按 `{ success, data, error }` 解包。

以下后端却返回裸 `Result<T, String>`、`Vec<T>` 或值：

- `src-tauri/src/commands/browser.rs:63-258`
- `src-tauri/src/commands/desktop.rs:53-199`
- `src-tauri/src/commands/artifact.rs:55-91`

所以即使后端成功，前端仍会因为 `response.success === undefined` 报错。

### API 表面漂移

- 前端暴露 `invoke_skill`：`tauri-api.ts:469-473`，后端没有注册对应 handler。
- 后端已有 `get_workspace`、`get_thread`、`check_budget`，前端没有完整 wrapper。
- Rust models、手写 TypeScript schema、手写 command wrappers 是三份事实来源；生成的 bindings 没有成为前端唯一入口。
- 错误多为任意字符串，UI 无法稳定区分 validation、permission、not found、conflict、provider、transport 等状态。

## P0 修复文档

### 1. 确定唯一边界规范

建议采用：

- JavaScript/Tauri 参数一律 camelCase。
- Rust 内部保持 snake_case，由生成层转换。
- 所有 command 返回 `CommandResponse<T>`，业务失败使用稳定 `CommandError`。
- Tauri transport reject 只表示桥接/序列化崩溃，不承载业务错误。

示意：

```ts
type CommandError = {
  code: string
  message: string
  retryable: boolean
  correlationId: string
  details?: unknown
}

type CommandResponse<T> =
  | { ok: true; data: T; correlationId: string }
  | { ok: false; error: CommandError; correlationId: string }
```

不要保留 `success?: boolean` 这类能容忍错误 envelope 的宽松 schema。

### 2. 生成 SDK，取消手写 key

- 从 Rust command DTO 生成 TypeScript 类型和 command client。
- 构建时比较“注册 command 清单”和“生成 client 清单”，双向不允许遗漏。
- Zod 只做不可信数据运行时校验，并由同一 schema/metadata 生成；不再维护独立手写模型。
- 先迁移核心链：workspace → thread → message/run → events → approval → tools；再覆盖设置与扩展能力。

### 3. 统一错误语义

最少定义：

- `VALIDATION_ERROR`
- `NOT_FOUND`
- `CONFLICT`
- `PERMISSION_DENIED`
- `APPROVAL_REQUIRED`
- `PROVIDER_UNAVAILABLE`
- `CAPABILITY_UNAVAILABLE`
- `CANCELLED`
- `TIMEOUT`
- `INTERNAL`

内部错误保留详细 cause 到结构化日志，UI 只获得安全的用户消息和 correlation ID。

### 4. 清理失配 API

- 删除或实现 `invoke_skill`，不能继续保留永远失败的公开 wrapper。
- 对每个 backend command 明确 owner、调用页面、是否发布。
- 未发布 command 不进入生成 SDK；实验 command 需 capability flag。

## 优化文档

- Application service DTO 与数据库模型分离，避免把存储结构直接暴露给 UI。
- 支持 command version 或 schema version，便于未来桌面自动更新时兼容数据。
- 为长任务使用 `operationId` + event stream，而不是让 command 长时间阻塞。
- 用统一 query-key factory 绑定 DTO，避免 scope/thread/run 参数漏入缓存 key。
- 在开发模式提供 IPC inspector：command、耗时、错误码、correlation ID；敏感字段自动遮罩。

## TDD 与验收

### RED

1. 枚举所有注册 command，对现状调用生成 client，证明 snake_case 与 raw envelope 失败。
2. 对 browser、desktop、artifact 写成功返回值测试，证明旧 `invokeCommand` 会误判。
3. 对前端存在、后端不存在的 command 写清单一致性失败测试。

### GREEN

- 所有 command 通过同一个 response builder。
- 生成 client 调用全部使用 camelCase。
- 成功、validation、permission、not found、Tauri reject 五类均能正确映射。

### 发布验收

- 100% 注册 command 通过真实 Tauri 参数反序列化 smoke test。
- 100% mutation 有成功与失败契约测试。
- `rg 'workspace_id:|thread_id:|run_id:' apps/desktop-ui/src/lib/tauri-api.ts` 不再发现手写 IPC key。
- 不存在 raw/envelope 混用、不存在 UI-only command，也不存在 backend-only 的发布 command。

