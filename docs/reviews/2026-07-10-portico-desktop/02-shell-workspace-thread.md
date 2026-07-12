# 02. App Shell、i18n、Accessibility、Workspace 与 Thread

## 结论

这一层有可识别的产品外形，但导航、项目创建、信任边界和 thread 生命周期仍是“演示骨架”。它应承担新用户首次成功路径，而不是把内部 UUID、路径和运行状态暴露给用户。

## 当前功能与证据

### App Shell

- 侧栏在较窄视口直接隐藏，缺少 hamburger/抽屉替代入口：`apps/desktop-ui/src/components/app-shell/app-sidebar.tsx:18`。
- Inspector 在 `<xl` 视口隐藏且没有其他打开方式：`components/app-shell/inspector-shell.tsx:47`。
- Topbar 无条件显示 `Ready`：`components/app-shell/app-topbar.tsx:73-77`，与真实 provider、IPC 和 WebView 状态无关。
- 多个页面依赖 mutation/query 局部错误，但没有一致的全局错误、toast 和恢复入口。

### Workspace

- 新建项目把 folder 标为 optional，空值写入 `"."`：`routes/workspaces/index.tsx:222-237`。
- `create_workspace` 接受前端传来的任意 root 与 trusted：`src-tauri/src/commands/workspace.rs:15-25`。
- 存储层直接插入路径，没有统一校验存在、目录、重复、根目录/用户目录风险：`crates/app-runtime/src/storage.rs:410-442`。
- allowed paths 有后端能力，但产品缺少完整编辑、解释与冲突恢复流。
- 路径检查主要是 lexical `starts_with`，不是完整 canonical/symlink 安全模型：`crates/app-runtime/src/workspace.rs:177-195`。

### Thread

- 可以创建和列出 thread，但名称普遍是“新会话”，没有 rename/delete/archive/auto-title。
- 打开项目可自动创建空 thread，用户没有真正发消息也会污染列表。
- 普通 thread 链接不携带 run 信息；页面只从 URL/本地状态恢复 active run。
- 没有服务端“最近 run/最后消息/未完成审批” hydration，因此重启和普通导航后的界面状态不可信。

### i18n 与 Accessibility

- 前端维护英文/中文两份大型手写词表：`apps/desktop-ui/src/lib/i18n.ts:1-910`；目前没有构建期 key parity、未使用 key 或硬编码文案扫描。
- 语言只保存在 WebView localStorage，并设置 document lang：`apps/desktop-ui/src/lib/i18n-react.tsx:16-38`；workspace/thread 数据、后端错误、日期/数字/费用格式没有统一 locale 协议。
- 基础组件有部分 `aria-label`、`role="alert"` 和 focus ring，但没有键盘全流程、focus trap/return、screen reader、对比度或 200% zoom 的自动/人工验收。
- Approval modal 声明 dialog ARIA，但产品尚未挂载；未来接入时必须验证初始焦点、Tab trap、Escape 策略和关闭后的焦点恢复。

## P0 修复文档

### 1. 重做首次启动向导

建议一次完成：

1. 选择已有本地目录，不允许默认 `.`。
2. 后端 canonicalize 并校验：存在、是目录、可读、不是危险系统根。
3. 以 `untrusted` 创建 workspace。
4. 展示将允许读取/写入的根目录，用户单独确认提升 trust。
5. 选择并测试真实 provider/model。
6. 创建第一个 thread，并让用户直接发送第一条消息。

任一步失败都不得产生半成品 workspace；创建应使用事务或补偿删除。

### 2. 后端拥有路径与归属规则

- UI 只提交用户选择；`WorkspaceService` 生成 canonical root、默认 allowed paths 与 trust。
- 拒绝 `/`、home、应用数据目录、临时系统目录等危险根，除非单独高风险确认。
- 唯一约束使用 canonical path，而不是用户输入字符串。
- `start/send` 必须验证 thread 属于 workspace，不能只验证两者分别存在。
- workspace 删除默认只删 Portico 元数据，绝不删除用户目录；清理 worktree/artifact 需单独确认。

### 3. 统一 capability 与状态展示

Topbar 状态来自后端 probe：

- `Setup required`：无真实 provider。
- `Ready`：provider health、DB、event bridge、workspace 均正常。
- `Degraded`：某非核心能力故障。
- `Running / Waiting approval / Paused`：当前 thread 活动状态。
- `Offline`：桥接失联或 WebView/后台服务异常。

静态 `feature-readiness.ts` 只能表示编译时 feature，不得表示运行可用。

### 4. 完成 Thread 基础生命周期

- 新 thread 在首条用户消息成功提交时创建，或把未发送的 draft thread 保存在前端而非数据库。
- 首条消息生成可编辑标题；支持 rename、archive、delete。
- 列表按最后 message/turn 活动时间排序。
- thread query 返回 `lastMessage`、`lastRunStatus`、`pendingApprovalCount`。
- 打开 thread 自动 hydrate 全部 conversation 状态，不依赖 URL 中存在 `runId`。

### 5. 建立 i18n 与 Accessibility 发布基线

- 把 translation key 变成类型化事实来源；构建时校验 en/zh key 完全一致、无空值、无未知 key。
- 扫描发布页面中的硬编码用户文案；后端错误通过稳定 error code 在前端本地化，不能直接翻译任意内部字符串。
- 日期、时间、数字、货币、token/cost 使用 `Intl` 和当前 locale；存储层保持 UTC/原始数值。
- 路由、modal、drawer、tabs、notification、composer 和 approval 完成语义结构、键盘顺序、focus management 与 live-region 设计。
- 目标至少达到 WCAG 2.2 AA：颜色对比、非颜色提示、44px 关键触控目标、200% zoom/reflow、减少动画偏好。

## 优化文档

### Shell

- 1024px 下使用可折叠侧栏，Inspector 使用 drawer；状态和 tab 写入 URL。
- 为每个 route 增加 error boundary、empty state 与 retry。
- 命令面板提供切换 workspace/thread、打开 Inspector、设置模型等高频动作。

### Workspace

- 支持最近目录、Git repo 检测、默认分支/脏状态提示。
- 项目设置中把 trust、allowed read paths、allowed write paths、terminal/MCP 能力分开解释。
- trust 变更显示影响预览，并使旧的长期授权失效。

### Thread

- 支持 pin、archive、搜索和自动标题，但不应抢在持久化闭环之前实现。
- 对 interrupted/failed turn 提供“重试本轮”而不是复用原 run。
- 深链使用 `workspaceId/threadId`；run/tool/approval 作为可选定位，不作为恢复前提。

### i18n / Accessibility

- 语言选择跟随系统作为首次默认，用户选择可覆盖；未来增加语言时按资源文件分包，而不是继续扩大单个 TypeScript 文件。
- 给长文案、CJK、英文、缺失翻译和伪本地化保留布局空间，避免按钮/侧栏截断。
- 使用成熟、可审计的 headless dialog/menu/tabs 语义或完善现有组件，避免每个页面自行实现键盘交互。
- 状态变化通过可控 `aria-live` 宣告；streaming token 不逐 token 轰炸 screen reader，而在语义阶段更新。

## TDD 与验收

### Workspace 测试

- 空路径、`.`、不存在路径、文件路径、重复路径、`/`、home、symlink 逃逸全部失败且不产生数据库行。
- 创建永远默认 untrusted；提升 trust 需要独立 command 和明确用户确认。
- A workspace 的 thread 不能与 B workspace 组合创建 run。

### Shell / Thread E2E

- 1024px 下 workspace、thread、settings、Inspector 均可到达。
- Fresh install 在无 provider 时明确阻止发送并引导设置，不显示 Ready。
- 发送首条消息后才出现持久化 thread，并生成可辨识标题。
- 刷新、关闭重开、从侧栏再次进入后，当前 thread 和状态完全恢复。
- 所有创建/重命名/归档失败都有用户可见错误，不产生重复或幽灵记录。

### i18n / Accessibility 验收

- en/zh key parity、fallback、未翻译 key 和用户可见硬编码文案均有 CI 检查。
- 切换语言后导航、日期、数字、费用、错误与空状态一致更新，重启后选择保留。
- 仅键盘可完成 workspace → thread → send → approval → Inspector 黄金路径，焦点始终可见且顺序合理。
- dialog/drawer 能 trap focus 并在关闭后返回触发点；动态错误和 run 状态能被 screen reader 感知。
- axe 自动检查无严重问题，并对 VoiceOver、200% zoom、reduced motion、浅/深色对比进行人工 release checklist。
