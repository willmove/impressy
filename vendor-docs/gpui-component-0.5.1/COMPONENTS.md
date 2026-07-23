# gpui-component 0.5.1 — 组件清单与 i18n

内容摘自 `gpui-component 0.5.1` 的真实 crate 源码（185 个源文件）。

## 版本配套

`gpui-component 0.5.1` 依赖 `gpui ^0.2.2`、`gpui-macros ^0.2.2`、`gpui-component-macros ^0.5.1`。见 [`../README.md`](../README.md)。

## ⚠️ Feature 陷阱：不要启用 `webview`

```
全部 feature: decimal, inspector, tree-sitter-languages, webview
default    : 无（Cargo.toml 中没有 default 键）
```

- **`webview`** —— 启用会引入 **`wry`**（`lb-wry` 0.53.3，WebView 封装）。这**直接违反 spec §1.3 设计原则 5「原生轻量：单二进制分发、无浏览器内核依赖、GPU 直渲」**。该模块被 `#[cfg(feature = "webview")]` 门控，默认关闭。**保持关闭。**
- **`tree-sitter-languages`** —— 会拉入十几个 tree-sitter 语法 crate。Impressy 不做代码编辑，不需要。开启会白白撑大二进制（关联 NFR-03「安装包 ≤ 30MB」）。
- **`decimal`** —— 引入 `rust_decimal`。按需。
- **`inspector`** —— 调试用，需与 `gpui/inspector` 配套；注意它同时启用 `wry/devtools`，**因此会引入浏览器内核**。仅可用于本地调试，绝不可进发布构建。

> 已核实：默认配置下 `wry` **不在**依赖树中，无 webkit / javascriptcore 等浏览器内核 crate。原则 5 成立。
> （依赖树中的 `tao-core-video-sys` 是 macOS CoreVideo 绑定，不是浏览器内核。）

## 组件清单（真实的 `pub mod` 列表）

```
accordion   alert       animation   avatar      badge       breadcrumb  button
chart       checkbox    clipboard   collapsible color_picker  description_list
dialog      divider     dock        form        group_box   highlighter history
input       kbd         label       link        list        menu        notification
plot        popover     progress    radio       resizable   scroll      select
setting     sheet       sidebar     skeleton    slider      spinner     switch
tab         table       tag         text        theme       tooltip     tree
webview（feature 门控）
另经 `pub use time::{calendar, date_picker}` 导出日历与日期选择器
```

### 与 Impressy 需求的对应

| 组件 | 用途 |
| --- | --- |
| `color_picker` | **FR-09 屏幕取色**的色值展示与格式切换（§9.2 待明确项 3：HEX/RGB） |
| `clipboard` | **FR-08** 截图复制到剪贴板、FR-09 色值复制。注意 spec §3.4 计划「GPUI 内置剪贴板 API 优先，图像/特殊格式回退 `arboard`」——先看这个模块是否已覆盖 |
| `form` `input` `button` `checkbox` `radio` `select` `slider` `switch` | 表单类页面的主力。spec §3.4 称组件库「覆盖表单类页面的绝大部分 UI 需求」，这些就是依据 |
| `table` `list` | **FR-03 批量处理**的任务列表（虚拟化，可承载上百张图） |
| `progress` `spinner` `skeleton` | **FR-03** 批量进度反馈（AC 要求「批量任务有进度反馈，失败项单独标记不中断整批」） |
| `dock` `sidebar` `tab` `breadcrumb` | 主界面四大板块导航（Requirement 34） |
| `notification` `dialog` `sheet` `popover` `tooltip` | 错误与反馈（Requirement 36） |
| `theme` | 主题系统 |
| `chart` `plot` `highlighter` `tree` `kbd` | Impressy 暂无对应需求 |

## i18n —— 这不是个选择题

**`gpui-component` 依赖 `rust-i18n` v3（实际解析为 3.1.5），并自带 `locales/ui.yml`，其中已包含 `en` 与 `zh-CN` 翻译**——正好是 Impressy 需要的两种语言（NFR-04、CFG-04）。

CFG-04 原文写的是「建议 `fluent` 或 `rust-i18n`」，像是个偏好选择。**它不是**：选 `fluent` 会导致一个二进制里并存两套 i18n 系统（组件库的 rust-i18n + 应用的 fluent），运行时切换语言必须同时驱动两边，且两套资源格式、两套缺失键检查。**选 `rust-i18n` v3。**

### 切换语言的真实 API

`gpui-component` 直接导出（`src/lib.rs`）：

```rust
pub fn locale() -> impl Deref<Target = str>;
pub fn set_locale(locale: &str);   // 内部转发 rust_i18n::set_locale(locale)
```

调 `gpui_component::set_locale("zh-CN")` 即可同时切换组件库自身的文案。这满足 CFG-04「切换语言立即生效无需重启」。

### 资源格式

`upstream-ui-locale.yml` 是组件库自带资源的原样拷贝，可作为 rust-i18n v2 格式的实例参考：

```yaml
_version: 2
Calendar:
  week.0:
    en: Su
    zh-CN: 日
    zh-HK: 日
    it: Do
```

注意这是 **`_version: 2` 的多语言合并格式**（每个键下并列各语言），不是每语言一个文件的格式。Impressy 自己的资源沿用同一格式可减少认知负担。

> 组件库只带了 `en` / `zh-CN` / `zh-HK` / `it` 等有限语言。Impressy 只需中英，覆盖完整。

## examples 与 component-demos

`gpui-component` 的 crates.io 发布包**不含 examples**，因此本目录下的示例是从 GitHub `longbridge/gpui-component` 的 **`v0.5.1` tag** 取的（**不是 main 分支**——main 与锁定版本不符）。

- **`examples/`** —— 官方「一个示例只做一件事」的独立示例。其中 **`brush.rs`（466 行）是 FR-08 画笔标注的首要参考**：用 `canvas()` + `MouseDownEvent` / `MouseMoveEvent` + 笔画状态实现了完整的画笔，含清空画布与网格。`dialog_overlay.rs` 是 FR-08 截图 overlay 的参考。
- **`component-demos/`** —— 26 个组件的官方用法 demo，取自仓库的 `crates/story`。想知道 `color_picker` / `table` / `form` 怎么用，先看这里。

> `hello_world.Cargo.toml.reference` 用的是 `gpui.workspace = true` 形式，看不到显式版本号——版本以 [`../README.md`](../README.md) 的锁定表为准。

组件的权威用法也可直接读上游 `crates/ui/src/` 下对应模块的源码与文档注释，那与锁定版本 100% 匹配。
