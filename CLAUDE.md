# impressy

Impressy 是一款跨平台原生桌面图像工具箱（Rust + GPUI），聚焦轻量、快速的本地图片处理。

## 工具链与 workspace

- **Rust**：stable（本机 1.96.1）。`gpui 0.2.2` 与 `gpui-component 0.5.1` 均为 **edition 2024**，Impressy 的 crate 一律用 `edition = "2024"`（需 Rust ≥ 1.85）。两者都未声明 MSRV。
- **Workspace crate 职责**（v1 先建前两个；v2 已按 [ADR-0004](./docs/adr/0004-v2-provider-contract.md) 启动后两个）：

| crate | 职责 | v1 |
| --- | --- | --- |
| `impressy-core` | 本地图像引擎：裁剪/压缩/拼接/切图/GIF/二维码/EXIF/水印。**纯函数，无 UI 无网络**，可完全 headless 测试 | ✅ |
| `impressy-app` | GPUI UI 层：四大板块导航 + 各功能页面 + 自定义 Element | ✅ |
| `impressy-ai` | AI Provider 抽象层与各家适配器 | ✅ v2 |
| `impressy-presets` | 行业工具锁定提示词模板库 | ✅ v2 |

## 动工前先读

1. **[`docs/spec/requirements.md`](./docs/spec/requirements.md)** 与 **[`docs/spec/design.md`](./docs/spec/design.md)** —— 唯一真相源。
2. **[`docs/adr/`](./docs/adr/)** —— 范围与顺序决策。ADR 胜过 spec 里任何与之冲突的表述。
3. **[`CONTEXT.md`](./CONTEXT.md)** —— 领域词汇。输出中涉及领域概念时用这里的词，不要漂移到同义词。

`docs/spec/archive/impressy-spec-v0.3.md` **已冻结**，仅作历史参考，**不要据此写代码**——它描述的是 v1+v2 全量，且里程碑顺序已被推翻。

## v1 本地功能边界与 v2 当前状态（硬约束）

**v1 = 本地功能**：完全离线、不需要 API Key 的图片处理功能集合。FR-08 屏幕截图、FR-09 屏幕取色及其支撑需求已按 ADR-0003 从当前范围剥离。
**v2 = AI 功能**：Requirement 4、5、16、17、18、19–31、33、50、59，以及所有标 `〔v2〕` 的 AC。

v1 阶段：

- **不建 `impressy-capture`、`impressy-ai`、`impressy-presets`。** workspace 只有 `impressy-app`、`impressy-core`。
- **不实现** Provider trait、BYOK、keyring / Credential Store、FR-11 海报设计。
- 主界面上「AI 生成与改图」与「行业定制 AI 工具」两个板块**是空的**，做「开发中」占位即可（与三个视频功能一样）。

上述为 v1 的历史边界。**当前 v2 已由 ADR-0004 启动**：workspace 有四个 crate，两个 AI 板块与海报设计实现完整图片功能；只有三个视频入口继续为「开发中」。任何后续修改都不得让 `impressy-core` 依赖网络、API Key 或 Provider 初始化。

理由见 [ADR-0001](./docs/adr/0001-v1-scope-local-only.md)：代码主要由 AI 生成，瓶颈在验收不在写码；AI 功能的验收标准是人眼主观判断的「保持不变项」，最好写、最难验。

## 建设顺序：core 优先

**先把 `impressy-core` 写到全绿，GPUI 骨架推后**（design.md 的 Phase 1–2 → Phase 5）。理由见 [ADR-0002](./docs/adr/0002-core-before-ui.md)。

i18n 随 UI 走，不随库走——`impressy-core` 是库，没有用户可见文本。

## 环境约束：本机跑不了 GPUI

主开发机是 **headless 云 VM**：无 `DISPLAY`、无显示服务器、0 个显示器、模拟显卡（Cirrus GD 5446，无硬件加速）。已装 rustup target 只有 `x86_64-unknown-linux-gnu`，无 mingw / cargo-xwin，**无法 check Windows target**。

因此：

- **`impressy-core` 在本机全自动收敛**：写 → `cargo test` → 绿。无需人工介入。这是首选工作方式。
- **UI 代码在本机能类型检查、不能运行**。已实测：`gpui 0.2.2` + `gpui-component 0.5.1` 在本机 `cargo check` 干净通过（3m14s，756 依赖，无缺失系统库）。**写完 GPUI 代码必须先在本机 `cargo check`**——它能自动抓住幻觉 API，那正是 GPUI pre-1.0 的主要风险形态。
- **但「能编译」≠「已验证」**。渲染正确性、自定义 Element 手感、冷启动耗时，本机一律测不了，必须由开发者在真机（Windows / macOS / Linux 桌面）上验收。**涉及这些的改动，如实说明「本机只做了类型检查」，不要声称已验证。**

## 质量门禁（每个任务的 Definition of Done）

```
cargo check && cargo clippy -- -D warnings && cargo test
```

外加对应需求的 AC。利用 Rust 编译错误质量高的特点，以「编译通过 + clippy 无警告 + 测试全绿」作为自我修正循环的收敛条件——**但注意这只对 `impressy-core` 完全成立**，UI 能编译不等于渲染正确。

## GPUI 锁定版本（不得依赖模型记忆）

```toml
gpui = "=0.2.2"            # crates.io 最新稳定版，2025-10-22
gpui-component = "=0.5.1"  # crates.io 最新稳定版，2026-02-05，依赖 gpui ^0.2.2
```

**用 `=` 精确锁定。** GPUI 是 pre-1.0，版本间有破坏性变更。升级作为独立任务，必须两者同步升级并重新生成 `vendor-docs/`。

**不要改用 Zed 仓库的 git HEAD**：git 版远新于 0.2.2，但 `gpui-component 0.5.1` 要求 `gpui ^0.2.2`，锁 git 会失去那 60+ 个组件。0.2.2 + 0.5.1 是当前唯一配套的一对。

### 写 GPUI 代码前必须先读 `vendor-docs/`

**[`vendor-docs/gpui-0.2.2/API-NOTES.md`](./vendor-docs/gpui-0.2.2/API-NOTES.md) 的「记忆陷阱」一节是强制阅读项。** 模型记忆中的 GPUI 是 context 重构**之前**的版本，与 0.2.2 差异巨大——已核实 `WindowContext`、`ViewContext` 在 0.2.2 中**根本不存在**，`AppContext` 从结构体变成了 trait，入口改为 `Application::new().run(|cx: &mut App| ...)`，视图创建改为 `cx.new(...)`。凭记忆写基本每行都错。

- API 存疑 → 查 [`vendor-docs/gpui-0.2.2/examples/`](./vendor-docs/gpui-0.2.2/examples/)（28 个官方示例，版本匹配，真实可编译）。**不要猜。**
- 自定义 Element（裁剪框）→ 参考 `examples/input.rs`，那是唯一实现完整三阶段管线的示例。
- 组件与 feature 陷阱 → [`vendor-docs/gpui-component-0.5.1/COMPONENTS.md`](./vendor-docs/gpui-component-0.5.1/COMPONENTS.md)。**不要启用 `webview` 或 `inspector` feature**——它们会引入 `wry` 浏览器内核，违反 §1.3 原则 5「无浏览器内核依赖」。

## 跨平台约定

三平台同时支持（Windows 10/11 x64、macOS、Linux）。

- 文件路径一律用 `std::path::PathBuf`，禁止字符串拼接。
- `impressy-core` 的三平台回归由 GitHub Actions（`windows-latest` / `macos-latest` / `ubuntu-latest`）承担，零人工成本。

## 多语言：用 `rust-i18n` v3（这不是选择题）

UI 支持简体中文 + 英文，运行时切换无需重启。所有用户可见文本必须通过 i18n 键引用，**禁止硬编码**；中英资源同步维护，缺失键视为编译错误。（仅适用于 UI 阶段——`impressy-core` 是库，没有用户可见文本。）

**i18n 方案已定：`rust-i18n` v3。** CFG-04 原文写「建议 `fluent` 或 `rust-i18n`」像是个偏好选择，但它不是——`gpui-component 0.5.1` 本身就依赖 `rust-i18n` v3（实际解析 3.1.5），且自带的 `locales/ui.yml` 已含 `en` 与 `zh-CN` 翻译。选 `fluent` 会让一个二进制里并存两套 i18n 系统，切换语言要同时驱动两边。

切换语言调 `gpui_component::set_locale("zh-CN")`，它转发给 `rust_i18n::set_locale`，同时切换组件库自身文案。资源沿用 `_version: 2` 的多语言合并格式，样例见 [`vendor-docs/gpui-component-0.5.1/upstream-ui-locale.yml`](./vendor-docs/gpui-component-0.5.1/upstream-ui-locale.yml)。

## Agent skills

### Issue tracker

Issues and PRDs live as GitHub issues in `willmove/impressy`, managed via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage roles, each label string equal to its role name (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `CONTEXT.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.
