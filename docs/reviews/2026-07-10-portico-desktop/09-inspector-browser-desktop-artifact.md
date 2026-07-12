# 09. Inspector、Browser、Desktop 与 Artifact

## 结论

Inspector 是有价值的审查界面，但当前面板与真实执行数据脱节；Browser、Desktop、Artifact 又同时受到 IPC、permission 和实现缺失影响。建议先把 Inspector 服务于核心 Files/Git/Run，再逐项恢复自动化能力。

## 当前功能与证据

### Inspector

- `<xl` 视口直接隐藏且无替代入口：`apps/desktop-ui/src/components/app-shell/inspector-shell.tsx:47`。
- Files panel 主要显示 Git status/diff，而不是文件树：`features/inspector/files-panel.tsx:15-125`。
- Terminal panel 是命令 form/history，不是流式 PTY session。
- 多个面板依赖手填 root/UUID，没绑定当前 workspace/thread/run。
- 当前没有足够 run/tool/artifact durable 数据供 Inspector 恢复。

### Browser

- command hardcode `trusted_workspace: false` 并请求缺少默认规则的 `desktop.control`：`src-tauri/src/commands/browser.rs:20-40`。
- 返回裸值，不兼容前端 envelope：`browser.rs:63-258`。
- `ExtractVisibleText` 执行 JS 后只返回 `"ok"`，没有拿到结果：`browser.rs:209-212`。
- click/type 的 eval 结果同样无法可靠传回 element-not-found。
- screenshot 失败时返回占位 SVG base64：`browser.rs:217-255`，会把失败伪装成成功。
- 没注册进 agent tool registry：`src-tauri/src/lib.rs:86-103`。

### Desktop

- 同样 hardcode false trust，且默认 policy 无 `desktop.control`：`src-tauri/src/commands/desktop.rs:12-43`。
- 返回裸值，与前端 envelope 不匹配：`desktop.rs:53-199`。
- 缺少 macOS Accessibility/Screen Recording 等权限 preflight 和可靠平台能力矩阵。
- 非支持平台可能 no-op/退化，却没有统一 `CAPABILITY_UNAVAILABLE`。

### Artifact

- command 返回裸值：`src-tauri/src/commands/artifact.rs:55-91`。
- preview 使用 hardcoded false trust；若简单放开又可能在缺少 canonical workspace bound 时读取任意路径。
- artifacts table 存在，但没有完整 CRUD/list/store，也没有生产者；`ArtifactCreated` 基本只存在于事件/UI 类型。
- preview 全量读取/base64，缺少 MIME、大小、binary、安全渲染限制。

## P0 修复文档

### 1. Inspector 先绑定真实上下文

- Inspector 由当前 workspace/thread 驱动；run/tool/artifact 可以深链定位。
- 1024px 使用 drawer，桌面宽屏使用固定面板。
- 每个 tab 统一 loading/error/empty/retry，错误带 correlation ID。
- 第一阶段只开放 Files/Git、Run details、Approval、Artifact；Terminal 按安全文档重建后再开放。

### 2. 暂时关闭 Browser/Desktop ready 状态

在完成下列条件前，feature flag 默认 false：

- 统一 IPC envelope 与 typed result。
- capability probe 与 OS permission preflight。
- 明确支持的平台与不可用原因。
- 接入 ExecutionContext/PolicyGate/ApprovalBroker。
- 注册为 agent tool 时使用 run tool allowlist。

### 3. Browser 采用可靠自动化后端

优先 CDP/WebDriver/受控 browser session，而不是依赖只返回 unit 的 eval：

- session 有 workspace/run owner、allowed origins、timeout 和 cancel。
- Navigate 校验 scheme/host/network policy。
- click/type/extract 返回结构化结果和 element diagnostics。
- screenshot 真失败就返回失败；图像保存为 artifact，不内联无限 base64。
- 限制 script capability，敏感页面/凭据需要显式授权。

### 4. Desktop 使用平台 adapter

- macOS/Windows/Linux 分别实现 capability probe 与 permission instructions。
- 高风险 input/click/keystroke 在执行前展示目标应用、窗口与动作摘要。
- 聚焦应用、截图、输入均有 timeout/cancel/audit。
- 不支持的平台返回稳定 unavailable，不允许假成功/no-op。

### 5. 建立 ArtifactService

- artifact 有 ID、workspace/thread/run、producer event、logical name、MIME、size、hash、storage path、created_at。
- 创建 artifact 与 `artifact_created` outbox 同事务。
- 存储位于受管目录或受控 workspace path，preview 根据 ID 解析；UI 不传任意 path。
- 文本、图片、PDF 等 renderer 设大小与 sandbox；未知/可执行类型只允许安全下载/在 Finder 显示。
- 删除遵循 retention 与引用关系。

## 优化文档

- Inspector tab 状态写 URL，可分享 run/tool/artifact 深链。
- Browser action 提供 screenshot-before/after、selector trace 与 network summary，便于调试。
- Desktop action 只作为实验能力发布，明确本地隐私与前台控制。
- Artifact 使用 content-addressed storage 去重，并能从 timeline、Inspector、notification 一致打开。
- 对超大文本/图片做分页、缩略图和按需读取，避免 WebView 内存峰值。

## TDD 与验收

### Inspector

- 1024px 可打开/关闭；当前 workspace/thread 自动生效；无需手填 UUID。
- deep link/刷新后仍定位同一 run/tool/artifact。

### Browser

- click 找不到元素必须失败，不得返回 ok。
- extract 返回真实可见文本；screenshot 可预览并持久化 artifact。
- blocked origin、timeout、cancel、session ownership 有集成测试。

### Desktop

- 无 OS 权限时不执行并给出平台引导。
- 不支持平台明确 disabled；成功操作有可验证反馈与 audit。

### Artifact

- Agent 生成文件后 timeline 与 Inspector 同时出现；重启仍可打开。
- 越界、symlink、超大文件、危险 MIME、缺失文件均安全失败。
- preview API 只接受 artifact ID，不接受任意本地路径。

