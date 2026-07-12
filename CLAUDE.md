@AGENTS.md

## ⚠️ 这是 fork,不是上游(新 AI 首读)

> 本仓 = **fork**,由 iOfficeAI/aionrs 单向同步而来。**只上游 → fork,永不反向提 PR。**

| 项 | 值 |
|---|---|
| origin(fork) | [`gaogg521/aionrs`](https://github.com/gaogg521/aionrs)(本地 `../aionrs-local`,分支 `master`)|
| upstream(上游)| [`iOfficeAI/aionrs`](https://github.com/iOfficeAI/aionrs) |
| 已同步到 | **v0.2.2**(=upstream/main,2026-07-12 对齐,落后 0)|
| fork 领先 | 4 个自有 commit(fork 专属补丁)|

**master 上的 4 个 fork 专属补丁(上游没有,必保留)**:
- `3f7b9b5` 流式 tool_call 参数占位空对象空参 bug 修复。
- `81a1d06` thinking 阶梯基础设施(只在显式请求声明 thinking + 多级重试 level1/2)。
- `ea45450` **文本化工具历史回放(命脉)** = 兜 litellm-internal 网关无状态拒绝一切 tool_calls 历史(level3);详见 1oneUI 的 [`session-2026-07-10-thinking-param-and-rename.zh-CN.md`](../1oneUI/docs/guides/session-2026-07-10-thinking-param-and-rename.zh-CN.md)。
- `1ffc171` 测试修复(engine_test 补 `..Default::default()`)。

**关键认知**:上游 PR #203 只改 thinking **声明**(v0.2.2 已含),fork 的 4 级重试阶梯(0原样/1 content-block/2 省略/3 文本化)是**正交的 fork 专属基础设施**,同步上游时必保留。

**下游依赖**:1oneCore 的 `aion-* = { git="gaogg521/aionrs", branch="master" }` 直接吃本仓 master。改完 master 推 origin 后,1oneCore `cargo build` 自动对齐。

**三仓上游映射 + 同步套路** 见 [`../1oneUI/docs/guides/upstream-sync-reference.zh-CN.md`](../1oneUI/docs/guides/upstream-sync-reference.zh-CN.md)。
