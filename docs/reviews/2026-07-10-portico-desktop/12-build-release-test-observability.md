# 12. Build、Release、Testing 与 Observability

## 结论

当前仓库在本机 sibling 环境中可编译和通过单测，但 CI/release 本身不可复现，也没有能证明桌面产品可用的测试层。交付体系目前是 P0 阻断项。

## 当前功能与证据

### 不可复现依赖

根 `Cargo.toml:41-48` 把 AutoAgents 指向 `../AutoAgents/...` sibling path。GitHub Actions 只 checkout 当前仓库：`.github/workflows/ci.yml:13-14`、`release.yml:16-17`，干净 runner 无法解析该依赖。

### 工具链和 artifact 路径漂移

- `package.json:21` 声明 `pnpm@10.6.0`，CI/release 安装 pnpm 9：`ci.yml:41-44`、`release.yml:27-30`。
- release 上传 `src-tauri/target/release/bundle/**`：`release.yml:54-65`，但 workspace target 通常位于根 `target/**`。
- Linux Tauri 系统依赖没有明确安装；三平台 keyring、desktop/browser adapter 和 capability 未形成矩阵。
- `tauri.conf.json` 宣称多平台/updater 配置，但二进制没有形成被验证的完整 updater plugin、签名与恢复链。

### 测试盲区

- 前端 138、Rust 240 个测试通过，但没有 Playwright/桌面 E2E、真 Tauri IPC 或真实 provider integration。
- 没有 coverage script/threshold，无法验证 AGENTS 要求的 80%。
- 21 个 Tauri command 模块以及 runner/storage/tool adapter 等关键文件缺少对应边界测试。
- `cargo fmt --check` 当前失败；前端 Prettier check 有 51 个文件失败。
- `pnpm audit` 因 `happy-dom@17.6.3` 失败：1 critical、2 high；`cargo-audit`/`cargo-deny` 未安装，Rust advisory 是盲区。
- 前端主 chunk 约 517.58 kB，构建已有 size warning。

### 可观测性不足

- `/private/tmp/portico-desktop-tauri.log` 大量 SQL trace 和周期性空轮询，信噪比低。
- WebView content process terminated 只有单行，缺少 session/correlation/crash context。
- UI IPC 错误通常不会进入后端日志。
- 没有围绕 run/provider/tool/job 的 latency、failure、queue、approval 等可操作指标。

## P0 修复文档

### 1. 让依赖可复现

AutoAgents 选择一种明确策略：

- 同 monorepo 并一起 checkout；或
- Git dependency 固定 commit/revision；或
- 发布 crate 并锁定 version/checksum；或
- vendor 经过审计的源代码。

不能保留只在某台开发机 sibling 目录存在的 path dependency。`Cargo.lock` 与 `pnpm-lock.yaml` 都使用 frozen/locked install。

### 2. 修正 CI 基线

CI 必须包含：

- pinned Rust toolchain、Node 与 `packageManager` 一致的 pnpm。
- `cargo fmt --check`、Clippy、tests、coverage。
- frontend format、lint、typecheck、tests、coverage、build。
- dependency audit、license/SBOM、secret scan。
- generated SDK drift check。
- 真 Tauri command contract smoke。

先格式化当前基线再启用门禁，避免永久红灯；格式化是机械改动，应与功能修复分开提交。

### 3. 建立测试金字塔

必须按 TDD 补齐：

1. Domain/unit：状态机、policy、path、scheduler、provider error mapping。
2. Repository/integration：SQLite transaction/constraints/migrations/recovery。
3. Adapter contract：provider HTTP、MCP、Git、PTY、browser/desktop capability。
4. Tauri IPC：真实参数序列化与 response envelope。
5. Desktop E2E：真实打包/或 Tauri test harness 的黄金路径。

覆盖率：整体 ≥80%；security、IPC、conversation runtime 分支 ≥90%。覆盖率不能替代安全 table/fuzz 与桌面 E2E。

### 4. 修复 release pipeline

- 三平台分别安装 Tauri system dependencies。
- 修正 bundle artifact 路径，并在上传前 assert 至少存在一个期望安装包。
- macOS signing/notarization、Windows signing、Linux package 依赖显式验证。
- 安装包 smoke：fresh install、启动、migration、关闭重开、卸载不删除用户项目。
- updater 必须有真实 endpoint、production public key、签名校验、失败回退；未完成前移除 updater 宣称。
- 发布附 SBOM、checksums、provenance 与 release notes。

### 5. 立即处理安全依赖

- 升级 `happy-dom` 到已修复版本并验证 Vitest 兼容性。
- CI 安装并运行 Rust advisory/license 检查。
- Critical/High 阻断；无法立即修复必须有带期限、owner 和影响分析的 exception。

## 优化文档

### Observability

- JSON tracing 分环境设置 level；默认不输出每次空 SQL poll。
- 统一 correlation ID，贯穿 command → use case → run → provider/tool → event/job。
- 指标：command error rate/latency、provider TTFT/tokens/failures、run terminal distribution、approval wait、tool failures、queue depth/lease recovery、WebView crash。
- 本地优先：默认只保存在设备；外发 telemetry 必须 opt-in、可预览、可关闭。

### 性能

- 前端 route/feature code splitting，Browser/Desktop/Operations 延迟加载。
- 设定 bundle budget 并在 CI 对回归报警。
- 后台轮询改 event-driven 或指数退避；App idle 时降低无意义 DB query。
- 给 event/log/artifact 设内存与磁盘上限。

## TDD 与发布验收

- 全新 checkout（无 sibling AutoAgents）能一条命令完成 locked build/test。
- CI 与 release 使用同一工具链版本和构建脚本。
- 三平台 job 至少完成 compile/package smoke；未支持 capability 由平台测试证明 disabled。
- 安装 artifact 路径 assert 后才能上传，不允许“job 成功但无安装包”。
- 三条最小桌面 E2E：首次 provider 设置、多轮工具审批对话、取消/重启恢复。
- `cargo fmt`、Prettier、coverage、audit、secret scan、SDK drift 全部为必需检查。
- 日志能通过 correlation ID 解释一次失败，但不包含 canary secret。

