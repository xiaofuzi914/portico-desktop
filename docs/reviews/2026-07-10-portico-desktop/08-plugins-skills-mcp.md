# 08. Plugins、Skills 与 MCP

## 结论

Plugins/Skills 当前主要是 metadata catalog，却使用了“安装/启用/调用”的产品语言；MCP 则已经能启动进程和访问网络，但缺少可信安装、secret 与网络边界。前者属于能力误导，后者属于 P0 安全风险。

## 当前功能与证据

### Plugin / Skill

- `install_plugin` 主要把 manifest 写入 registry：`crates/app-plugins/src/registry.rs:133-179`；没有 package 下载、签名、隔离加载或执行注册。
- plugin manifest 中的 skills 不会自动写入独立 skills 表；`register_skill` 主要只在测试中使用：`registry.rs:248-309`。
- 前端暴露 `invoke_skill`：`apps/desktop-ui/src/lib/tauri-api.ts:469-473`，后端没有 handler。
- enable/disable metadata 不会可靠改变 agent tool registry。

### MCP

- `add_mcp_server` 没有可信来源、审批和命令验证：`src-tauri/src/commands/plugins.rs:127-136`。
- list/refresh enabled server 时会连接；stdio 能直接启动任意 command + args + env：`crates/app-plugins/src/mcp.rs:213-260,550-557`。
- HTTP transport 对任意 URL POST，未经过 NetworkPolicy：`mcp.rs:619-680`。
- env 以明文 JSON 存 SQLite，UI 可显示 `KEY=value`。
- 工具副作用按名称关键词猜测：`mcp.rs:301-312`；未知/恶意名称可能错误当作 read-only。
- MCP adapter 遇到 Ask 直接 PermissionDenied，没有 approval continuation。

## P0 修复文档

### 1. 先诚实降级 Plugin/Skill

在可执行平台完成前：

- 将 Plugin 页面命名为 Catalog/Manifest Registry，或整体 feature-flag。
- “Install” 改为“Register metadata”，明确不会下载或执行代码。
- 隐藏 invoke/enable 对运行能力的承诺。
- 删除不存在的 `invoke_skill` API，或实现最小、受控的内建 skill invocation。

### 2. 暂停任意 MCP stdio

默认关闭 stdio MCP。重新启用前配置必须经过 trust ceremony：

- 展示 executable canonical path、args、工作目录、env key 名、来源与 hash。
- executable 必须来自允许位置或受信 package；禁止 UI 任意字符串直接 spawn。
- env 值只存 Keychain reference，子进程获得最小白名单环境。
- 设置资源限制、启动/调用 timeout、输出上限、进程组 cancel 与 crash audit。
- server config 变化使旧 grants 失效。

### 3. MCP HTTP 接入 NetworkPolicy

- 限制 `https`；localhost 仅显式 local-server mode。
- URL parse、DNS resolve、最终 IP、每次 redirect 都检查 loopback/link-local/private/metadata ranges。
- 防 DNS rebinding；限制 redirect 次数、response size、并发、timeout。
- proxy/TLS 行为明确，不允许自动继承不受控环境。
- Authorization/cookie 不写日志或 diagnostics。

### 4. 显式 capability schema

MCP tool 必须声明或由用户确认：

- read/write/network/process/secret scopes
- side-effect level
- resource patterns
- input/output schema

未知 tool 默认 Ask，不再按名字猜。所有调用通过 PolicyGate + ApprovalBroker，保存 tool/server/version snapshot。

## P1：真正的 Plugin/Skill 平台

需要先确定插件定义：本地目录包、签名远程包还是内建扩展。完整平台至少包含：

- manifest schema/version/permission validation
- package provenance、hash、signature 与 dependency resolution
- 原子安装、注册、启用、禁用、升级、rollback、卸载清理
- 运行时隔离与资源配额
- 工具/skill allowlist 与版本绑定
- 安装/升级时权限 diff 复审
- 插件故障不能拖垮主进程

Skill invocation 应把 required tools、input schema、system prompt、output schema 固化为 RunSpec，不允许只存一段字符串名称。

## 优化文档

- MCP 设置页显示 health、latency、server version、tools count、最后错误和 restart。
- refresh mutation 必须 await 并展示结果，不能 `void` 吞错。
- tool browser 展示 schema、permission、side effects 与来源。
- 为本地官方 connector 提供预审 profile；第三方默认更严格。
- 插件市场/远程安装属于后期能力，在核心本地 Agent 稳定前不投入。

## TDD 与验收

### MCP 安全

- localhost、私网 literal、域名解析私网、redirect 私网、DNS rebinding 均拒绝。
- 任意 executable、危险 args、越权 cwd/env 无法启动。
- 未声明 side effect、配置发生变化、未知 tool 均 Ask。
- timeout、超大响应、异常退出能取消、限流、审计且不泄漏 secret。

### Approval 集成

- read-only 已授权 tool 可执行；write tool 产生 approval；approve 后恰好一次，deny 零副作用。
- App 重启后 pending MCP invocation 可安全处理。

### Plugin/Skill

- 若 UI 仍显示“安装”，必须验证安装后能力真实出现、禁用后立即不可调用、卸载无残留。
- manifest 无效、签名不可信、权限升级均阻断。
- 在完整平台未达到上述验收前，发布构建中只能呈现 catalog，不得宣称可执行插件。

