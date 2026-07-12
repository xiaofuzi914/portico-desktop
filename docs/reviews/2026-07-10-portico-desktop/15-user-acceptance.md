# 15. 最终用户验收清单

验收对象：`target/release/bundle/dmg/Portico_0.1.0_aarch64.dmg`  
适用平台：Apple Silicon macOS  
目标：验证真实凭据、真实 WebView、系统 Keychain 与人工交互组成的最后一段黄金路径。

## 验收前说明

- 本地 DMG 使用 ad-hoc 签名，只用于本机验收；对外分发仍必须使用 Developer ID 签名并完成 Apple notarization/stapling。
- 可先运行 `pnpm tauri:e2e:desktop` 重复隔离构建的真实 WebView/IPC/Provider 重启持久化自动验收，再运行 `pnpm tauri:verify:macos:local` 重复 production DMG checksum、只读挂载、签名和测试插件缺失扫描；两者都不替代真实 Keychain/Provider 与 production 数据目录的人工交互验收。
- 桌面 E2E 的 WebDriver HTTP server 只存在于显式 `desktop-e2e` 测试构建；正常 DMG 不编译该插件，也不增加测试 capability。
- 先备份 `~/Library/Application Support/ai.portico.desktop`。不要为了测试删除真实工作区或 Keychain 凭据。
- 选择一个专用测试 Git 仓库；不要把 home、`/`、系统目录或包含真实秘密的仓库设为工作区。
- Terminal、Git 写操作、MCP、Browser/Desktop、自动化与多 Agent 扩展当前应保持不可用；它们不属于本次安全黄金路径。

## A. 安装与首次启动

1. 双击 DMG，确认出现 `Portico.app` 和 Applications 链接，图标正常。
2. 将应用拖入 Applications 后启动。
3. 确认应用窗口可打开，无白屏、无立即退出。
4. 关闭应用后重新打开，确认原窗口数据能够恢复。

通过标准：安装、启动、关闭、重开均成功；不会要求访问不相关系统目录。

## B. Workspace 边界

1. 创建专用测试 Workspace，选择一个存在的 Git 仓库子目录。
2. 初始状态应为 untrusted；手动设为 trusted。
3. 配置 workspace 内的 read/write allowed paths。
4. 尝试选择 `/`、home、`/private/tmp` 根或 workspace 外路径，确认被拒绝。

通过标准：只有 canonical workspace 内路径可保存；跨 workspace 与 symlink escape 均不可用。

## C. Provider 与模型

1. 创建一个真实 Provider，填写 Keychain reference，并保存 API key。
2. 创建对应模型，执行 Connection Test。
3. 只有状态为 `Ready` 后才能设为 active model。
4. 分别验证 Global 和当前 Thread selection；Thread selection 应覆盖 Global。
5. 修改 Provider URL 或 API key 后，旧健康状态应立即失效，必须重新测试。

通过标准：UI/错误/日志不显示 API key；认证、限流、超时显示为不同安全错误；run 记录不可变 provider/model snapshot。

## D. 五轮对话、取消与重启

1. 连续发送五轮带上下文的问题，第五轮要求模型复述第一轮的一个非敏感事实。
2. 发起一次较慢请求并点击 Stop，确认最终状态为 Cancelled，且不会产生迟到的完成消息。
3. 关闭并重开应用，确认五轮消息和 run 状态仍存在。

通过标准：历史顺序正确；同一 thread 同时只有一个活动 turn；取消和重启没有重复 assistant 消息。

## E. 安全工具与审批

1. 让 Agent 读取 allowed read path 内的一个 UTF-8 文件，确认能返回内容。
2. 让 Agent 写入 allowed write path 内的专用文件，确认出现 approval modal，且批准前文件不变化。
3. 第一次选择 Deny，确认本次 invocation 不执行。
4. 再发起一次写入并选择 Approve，确认文件只写一次。
5. 让 Agent 执行 `git status`/`git diff`，确认只读结果能回到对话。
6. 在 approval modal 未处理时退出应用，重开后确认审批仍存在；批准后只续跑一次。

通过标准：`fs_write` 必须审批；双击 Approve 不产生双写；workspace 外路径被拒绝；Git mutation 与 Terminal 不可调用。

## F. Diagnostics

1. 在 Settings 收集 diagnostics bundle。
2. 打开返回目录中的 `app.log` 与 `audit-summary.jsonl`。
3. `app.log` 应明确说明原始日志未被导出；audit 文件只包含聚合计数和 `payloads_included: false`。
4. 搜索真实 API key、对话正文和本地路径，均不得出现。

通过标准：diagnostics 不含原始日志、审计资源、prompt、token 或 API key；不存在“模拟上传成功”入口。

## 验收记录

| 项目 | 结果 | 备注 |
|---|---|---|
| 安装/启动/重开 | 待验收 | |
| Workspace 边界 | 待验收 | |
| Provider/Model | 待验收 | |
| 五轮/取消/恢复 | 待验收 | |
| 审批/文件写/Git diff | 待验收 | |
| Diagnostics 无泄漏 | 待验收 | |

全部通过后，才把“macOS 本机 UAT”标记为完成。Developer ID 签名、公证以及 Windows/Linux 安装包仍是独立发布门禁。
