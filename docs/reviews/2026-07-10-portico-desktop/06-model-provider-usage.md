# 06. Provider、Model、Usage 与 Budget

## 结论

当前设置页能保存 provider/model/key，但运行时 executor 只在应用启动时构建一次；保存配置不代表真实执行会使用它。未配置时静默 Mock 又让产品产生“已经成功调用模型”的错误印象。

## 当前功能与证据

- executor 在 Tauri setup 阶段构建一次：`src-tauri/src/lib.rs:105-127`。
- 构建失败或无配置时保存为 None，submit 再静默走 Mock：`src-tauri/src/lib.rs:111-115`、`crates/app-runtime/src/runner.rs:400-426`。
- provider/model/key command 只更新 registry/keychain，不重建 executor：`src-tauri/src/commands/model.rs:44-155`、`commands/secrets.rs:17-51`。
- 默认选择“第一个 enabled provider + 第一个 model”，而不是用户明确选择：`crates/autoagents-adapter/src/provider_factory.rs:212-243`。
- UI 没有稳定的 workspace/thread/run active model 语义。
- Google/Azure 等 provider 出现在配置表面，但 factory 不支持真实运行：`provider_factory.rs:191-200`。
- retry/fallback/default headers/org/project 等配置未完整进入执行路径。
- `record_usage` 没有生产调用；费用和 budget UI 长期可能为 0。

本机 SQLite 有 1 个 DeepSeek provider，但 0 model info、0 run、0 usage；配置存在不等于能力可用。

## P0 修复文档

### 1. 禁止静默 Mock

- release build 无 provider 时返回 `PROVIDER_UNAVAILABLE` 并展示 onboarding。
- Mock executor 只在测试、显式 Demo Mode 或开发 feature 中启用。
- Demo Mode 在 topbar、message、run metadata 中持续标识，不能伪装真实回答。

### 2. ProviderResolver 按 run 解析

不要持有单个启动期 executor。每次创建 run：

1. 解析 thread/workspace/global 的 active model policy。
2. 读取 provider metadata 与 Keychain secret。
3. 验证 provider health 与 model capability。
4. 创建/复用可安全缓存的 client。
5. 把 provider/model/config version 固化到 run snapshot。

配置变更通过 version/invalidation 立即生效，不需要重启；运行中的 run 继续使用自己的不可变 snapshot。

### 3. 明确选择层级

建议优先级：

```text
run override > thread selection > workspace default > global default
```

UI 至少显示当前 model，并允许 thread 级切换。未支持的 provider/model 不允许保存为 active；而不是保存后在执行时才失败。

### 4. 增加连接测试与健康状态

保存 provider 前后执行最小、可取消、限时的 health check：

- secret 是否可取
- base URL/scheme 是否允许
- authentication 是否成功
- model 是否存在/可调用
- streaming/tool-calling capability 是否满足

状态区分 `Unconfigured / Checking / Ready / Degraded / Invalid credentials / Unsupported`，保留最后检查时间与安全错误摘要。

### 5. Usage 与 Budget 接入真实路径

- adapter 把 input/output/cache tokens、provider request ID、估算/实际 cost 返回 runtime。
- usage 与 run terminal event 同事务/幂等写入。
- run 创建前做预算 guard，stream 中可做软阈值预警，结束后结算。
- 明确预算 scope（全局/workspace）、周期和币种；无定价信息时显示 unknown，不能显示 0。

## 优化文档

- `ProviderAdapter` 统一 streaming、tool calls、usage、cancel、retry 分类；provider-specific 配置只留在 adapter。
- retry 只针对可重试错误，使用退避与 jitter；tool side effect 发生后不得盲目重放整个 run。
- fallback model 必须是用户配置的 policy，并记录实际切换原因；不能静默。
- custom base URL 使用 NetworkPolicy，校验 DNS、redirect、私网和 TLS。
- model catalog 带 capability、context window、price、last verified；过期数据明确标识。
- secret 永不进入 DTO、日志、diagnostics 和错误 details。

## TDD 与验收

### Provider 集成

- 应用启动后新增 provider，无需重启，下一 run 使用新配置。
- 本地 mock HTTP server 证明真实请求收到了正确 model/history，并能 stream/cancel。
- secret 错误、model 不存在、timeout、429、5xx 映射为稳定错误码。
- 配置在 run 中途变化不影响该 run，下一个 run 使用新版本。

### Usage / Budget

- 同一 provider response 重放不会重复记账。
- 一次真实/fixture 调用后 UI usage 与 cost 更新，不再恒为 0。
- 达到 hard budget 时在网络请求前阻止并解释；unknown cost 不被当作 zero。

### 产品 E2E

- Fresh install 无模型时无法误发送，topbar 显示 Setup required。
- Test connection 成功后立刻变 Ready；发送消息能在 run details 查看实际 provider/model。
- 不支持的 provider 不能被选为 active。
- 全流程没有静默 Mock 或静默 fallback。

