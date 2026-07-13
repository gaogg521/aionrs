@AGENTS.md

## ⚠️ 这是 fork,不是上游(新 AI 首读)

> **本仓 = fork，只单向同步上游 → fork，永不反向提 PR。** 三仓对上游的映射、版本对照、当前同步状态、同步套路与不变量见：[`../1oneUI/docs/guides/upstream-sync-reference.zh-CN.md`](../1oneUI/docs/guides/upstream-sync-reference.zh-CN.md)

**master 上的 6 个 fork 专属补丁(上游没有,必保留)**:
- `3f7b9b5` 流式 tool_call 参数占位空对象空参 bug 修复。
- `81a1d06` thinking 阶梯基础设施(只在显式请求声明 thinking + 多级重试 level1/2)。
- `ea45450` **文本化工具历史回放(命脉)** = 兜 litellm-internal 网关无状态拒绝一切 tool_calls 历史(level3);详见 1oneUI 的 [`session-2026-07-10-thinking-param-and-rename.zh-CN.md`](../1oneUI/docs/guides/session-2026-07-10-thinking-param-and-rename.zh-CN.md)。
- `1ffc171` 测试修复(engine_test 补 `..Default::default()`)。
- `8de0bf5` **deferred 工具 schema 命中即提升(命脉)** = 兜 GLM 等受约束解码渠道对 stub schema 只能生成 `{}` 空参的死循环(ToolSearch 命中/空参失败均提升为全量申报);详见 1oneUI 的 [`session-2026-07-13-deferred-schema-and-assistant-skills.zh-CN.md`](../1oneUI/docs/guides/session-2026-07-13-deferred-schema-and-assistant-skills.zh-CN.md)。
- `92d9242` **GLM 盲搜纠偏(命脉,与 8de0bf5 同源)** = 系统提示 + ToolSearch 未命中消息把 GLM 从「把延迟工具引导过度泛化、盲搜核心工具/技能」拉回直接调用 Skill/直连工具;同上文档。

> **8de0bf5 + 92d9242 是机制级修复,无任何模型名硬判**(符合 No Hardcoded Provider Quirks):修的是延迟工具机制本身对受约束解码模型不友好的缺陷,对所有模型生效,GLM 只是第一个踩崩的。若将来提示级纠偏不够,后备是 `ProviderCompat.eager_tool_schemas`(按 provider 配置关 deferral,仍非按模型名),别退回到 `if model==...` 特判。

**关键认知**:上游 PR #203 只改 thinking **声明**(v0.2.2 已含),fork 的 4 级重试阶梯(0原样/1 content-block/2 省略/3 文本化)是**正交的 fork 专属基础设施**,同步上游时必保留。

**下游依赖**:1oneCore 的 `aion-* = { git="gaogg521/aionrs", branch="master" }` 直接吃本仓 master。改完 master 推 origin 后,1oneCore `cargo build` 自动对齐。

**三仓上游映射 + 同步套路** 见 [`../1oneUI/docs/guides/upstream-sync-reference.zh-CN.md`](../1oneUI/docs/guides/upstream-sync-reference.zh-CN.md)。
