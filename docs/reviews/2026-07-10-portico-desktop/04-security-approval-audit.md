# 04. Permission、Approval、Secrets 与 Audit

## 结论

当前安全层存在双重问题：正常写操作因审批链断开而不可用；Terminal、MCP、跨 workspace 路径等实际执行路径又绕过或错误使用策略。**安全修复必须成为所有工具的统一基础设施，而不是各模块自行拼装布尔值。**

## 当前功能与证据

### Approval 是孤岛

- runtime 能创建 approval 并进入 `WaitingApproval`：`crates/app-runtime/src/runner.rs:627-675`，但生产工具路径没有调用。
- filesystem 写入遇到 Ask 直接返回 PermissionDenied：`crates/app-tools/src/filesystem.rs:331-358`。
- MCP 写工具同样直接拒绝：`crates/autoagents-adapter/src/mcp_bridge.rs:60-79`。
- `ApprovalModal` 没有页面挂载：`apps/desktop-ui/src/components/approval/approval-modal.tsx:13-66`。
- 没有 list-pending-approvals/hydration command。
- approve 主要改变 run 状态，没有保存可继续的 tool invocation：`runner.rs:708-720`。

### Policy 使用不一致

- 默认 policy 包含 shell、filesystem、MCP、network，但缺少 git 和 desktop/browser 的完整规则：`crates/app-security/src/permission.rs:119-143`。
- Browser/Desktop/Artifact 硬编码 `trusted_workspace: false`；filesystem/Git 某些路径又硬编码 true。
- Terminal 根本没有调用 `PermissionEngine`。
- `NetworkPolicy::allow_request` 只在定义/测试中出现，provider/MCP HTTP 没有生产接线。

### Secret 与诊断泄漏

- provider plaintext fallback 会将 key reference/内容相关信息写 warning：`crates/autoagents-adapter/src/provider_factory.rs:62-99`。
- MCP env 以 JSON 明文存 SQLite，并能在 UI 直接显示。
- terminal audit 保存完整命令，命令内 token/password 会入库：`crates/app-tools/src/terminal.rs:83-89,130-136`。
- diagnostics 只脱敏 audit summary，却直接复制 app log，然后把 bundle 标为 `redacted=true`：`src-tauri/src/commands/diagnostics.rs:34-76`。

## P0 修复文档

### 1. 所有能力必须携带 ExecutionContext

```text
ExecutionContext
  workspace_id
  thread_id
  run_id
  actor_id / source
  canonical_workspace_root
  trust_version
  granted_paths
  tool_allowlist
  correlation_id
```

工具 API 不再接受调用方自由拼接的 `trusted_workspace`、root 或默认 nil workspace。缺少 context 即拒绝。

### 2. 建立中央 PolicyGate

执行顺序固定为：

1. 验证 capability 是否启用。
2. 验证 workspace/thread/run 归属和状态。
3. canonicalize resource；验证 workspace scope。
4. 计算 `Allow / Ask / Deny`。
5. Allow 执行；Deny 记录原因；Ask 创建持久 ToolInvocation + ApprovalRequest。

Terminal、Git、FS、MCP、network、browser、desktop、artifact 必须使用同一个 gate；禁止 adapter 自行 hardcode trust。

### 3. 实现可恢复 ApprovalBroker

`Ask` 时持久化：

- action 与资源摘要
- 精确的不可变 invocation payload 或安全重建信息
- policy/trust/tool version
- 风险说明、预计副作用
- expiry、status、version
- continuation token

运行中的进程可使用受监督 channel 等待；App 重启后 worker 由数据库恢复 waiting invocation。approve 后同一 invocation **恰好执行一次**；deny/expire/cancel 绝不执行。

UI 同时提供 timeline inline card 与全局 pending queue；禁止直接 `resume WaitingApproval` 绕过 approval resolution。

### 4. 重建 secret 管理

- API key、MCP env、Authorization header 只存 Keychain/secret manager reference。
- 数据库只存不敏感 metadata 和 reference ID。
- 禁止 plaintext fallback；提供一次性迁移并要求用户确认删除旧值。
- 结构化日志默认对 key/token/password/authorization/cookie/env/command 敏感片段脱敏。
- diagnostics 导出前逐文件扫描，并允许用户预览清单；不能用一个布尔字段声称已经脱敏。

### 5. 强化 Audit

每条安全相关记录包含：workspace/thread/run/tool/approval/correlation、decision、policy rule、resource hash/安全摘要、结果、耗时。

不要记录完整 secret、完整终端命令或未限制的模型 payload。需要取证时使用可配置的安全摘要与显式用户同意。

## 优化文档

- policy 采用版本化声明规则，UI 能展示“为什么允许/拒绝/询问”。
- grant 区分 once、this run、this workspace，并绑定 resource pattern 与 policy version。
- trust 降级、allowed path 变化、plugin/MCP 更新时自动撤销相关长期 grant。
- network policy 在 DNS 解析后和每次 redirect 后重复检查，防止 SSRF/DNS rebinding。
- 给高风险 action 增加不可伪造的人类可读 preview，例如文件 diff、命令 argv/cwd、MCP server/tool 与参数摘要。
- 审计查询按当前 workspace 自动过滤，支持时间、action、decision、run 与导出，不要求手填 UUID。

## TDD 与验收

### 权限矩阵

每种 action 至少覆盖 Allow/Ask/Deny；每个测试同时验证副作用和 audit，而不只验证返回值。

### Approval 竞态

- approve 两次只执行一次。
- approve 与 cancel/expire 并发只有一个结果。
- deny 后零副作用。
- App 在 waiting 时崩溃，重启后仍可安全 approve/deny。

### Secret canary

使用 canary secret 跑 provider、MCP、terminal、错误、diagnostics 全路径；日志、SQLite、bundle、UI 错误中零明文命中。

### 发布门禁

- 任一工具绕过 PolicyGate 即 CI 失败。
- 所有危险工具均记录 workspace/thread/run/correlation。
- 未脱敏 diagnostics 或 plaintext secret fallback 不得进入 release build。
- Critical/High dependency advisory 和 secret scan 阻断发布。

