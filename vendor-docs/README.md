# vendor-docs — GPUI 锁定版本参考语料

**写 GPUI 代码前必须先读这里。不得依赖模型记忆中的 GPUI API。**

> ## ⚖️ 第三方代码，非 Impressy 自有
>
> 本目录下的 `examples/`、`component-demos/`、`upstream-docs/`、`upstream-ui-locale.yml` 均为**上游第三方代码原样拷贝**，版权归原作者，按 **Apache-2.0** 授权：
>
> - `gpui-0.2.2/` —— © 2022–2025 Zed Industries, Inc.（见 [`gpui-0.2.2/LICENSE-APACHE`](./gpui-0.2.2/LICENSE-APACHE)）
> - `gpui-component-0.5.1/` —— © Longbridge（见 [`gpui-component-0.5.1/LICENSE-APACHE`](./gpui-component-0.5.1/LICENSE-APACHE)）
>
> 只有 `README.md`、`gpui-0.2.2/API-NOTES.md`、`gpui-component-0.5.1/COMPONENTS.md` 三份是 Impressy 自己写的说明。
>
> **不要把本目录的代码直接复制进 `impressy-app`** —— 参考写法，不是拿来即用的源码。真要整段借用，注意保留 Apache-2.0 的署名义务。本目录不参与构建（不在 workspace members 中）。

## 锁定版本

| crate | 版本 | 发布日期 | 说明 |
| --- | --- | --- | --- |
| `gpui` | **0.2.2** | 2025-10-22 | crates.io 上的最新稳定版 |
| `gpui-component` | **0.5.1** | 2026-02-05 | crates.io 上的最新稳定版，依赖 `gpui ^0.2.2` |
| `rust-i18n` | **3.1.5** | — | 经 `gpui-component` 间接引入，见下 |

在 workspace 中用 `=` 精确锁定：

```toml
gpui = "=0.2.2"
gpui-component = "=0.5.1"
```

**为什么是 crates.io 的 0.2.2 而不是 Zed 仓库的 git HEAD**：`gpui` 在 Zed 主仓库中是 in-tree 开发的，git HEAD 远新于 0.2.2。但 `gpui-component 0.5.1` 声明依赖 `gpui ^0.2.2`——锁 git HEAD 会让这个约束失效，从而失去 60+ 个桌面组件（spec §3.4 称其覆盖表单类页面的绝大部分需求）。**0.2.2 + 0.5.1 是当前唯一配套的一对。**

升级作为独立任务处理，且必须同时升级两者并重新生成本目录。

## 目录内容

```
vendor-docs/
├── README.md                          ← 本文件
├── gpui-0.2.2/
│   ├── API-NOTES.md                   ← 关键 API 真实签名 + 记忆陷阱清单【先读这个】
│   ├── upstream-docs/                 ← gpui 0.2.2 自带官方文档（原样）
│   │   ├── contexts.md                   context 参数体系，理解重构后的模型必读
│   │   └── key_dispatch.md               按键分发与 action 体系
│   └── examples/                      ← gpui 0.2.2 自带官方示例（原样，28 个）
└── gpui-component-0.5.1/
    ├── COMPONENTS.md                  ← 组件清单 + feature 陷阱 + i18n【先读这个】
    ├── examples/                      ← 官方独立示例（每个只演示一件事）
    │   ├── hello_world.rs                最小 gpui-component 应用
    │   ├── input.rs                      输入框
    │   ├── dialog_overlay.rs             overlay（FR-08 截图 overlay 参考）
    │   ├── window_title.rs               自定义标题栏
    │   ├── app_assets.rs                 资源加载
    │   ├── brush.rs                      **画笔实现（FR-08 画笔标注的首要参考）**
    │   ├── dock.rs                       Dock 布局（主界面导航参考）
    │   ├── hello_world.Cargo.toml.reference
    │   └── UPSTREAM-README.md            官方对这些示例的说明
    ├── component-demos/               ← 26 个组件的官方用法 demo（来自 story crate）
    └── upstream-ui-locale.yml         ← 组件库自带的 rust-i18n 资源实例
```

**全部内容均从锁定版本的真实源码提取**，非模型记忆产物：

- `gpui-0.2.2/*` —— 从 crates.io 下载 `gpui-0.2.2.crate` 解包取得。
- `gpui-component-0.5.1/COMPONENTS.md`、`upstream-ui-locale.yml` —— 从 crates.io 的 `gpui-component-0.5.1.crate` 解包取得。
- `gpui-component-0.5.1/examples/`、`component-demos/` —— 发布包不含 examples，故从 GitHub `longbridge/gpui-component` 的 **`v0.5.1` tag** 取得（**不是 main 分支**）。

> 注意：仓库根 `Cargo.toml` 的 `version = "0.58.0"` 是 workspace 内部版本号，与发布的 crate 版本 0.5.1 无关，别看岔。`gpui-component` crate 本体在仓库的 `crates/ui/`。

### 刻意排除的内容

以下上游示例**故意没有收录**，因为它们演示的能力与 Impressy 的架构承诺冲突，放进来等于诱导 codegen 去用：

- `webview.rs`、`html.rs`、`markdown.rs`、`editor.rs` —— 依赖 `wry`（浏览器内核）或 tree-sitter。前者违反 §1.3 原则 5「无浏览器内核依赖」，后者白白撑大二进制（NFR-03「安装包 ≤ 30MB」）。
- `chart` / `calendar` / `date_picker` / `kbd` / `otp_input` 等组件的 demo —— Impressy 无对应需求。
- `examples/image/`、`examples/svg/` 素材（约 4.5MB）—— 只有 `.rs` 源码有参考价值。

需要时从 crates.io / GitHub tag 重新获取。

## 使用规则

1. **写 UI 代码前**：先读 `gpui-0.2.2/API-NOTES.md` 的「记忆陷阱」一节，再读与任务最接近的 example。
2. **写代码时**：以仓库内现有代码和本目录的 example 为准，模仿现有写法。
3. **API 存疑时**：查 `examples/` 里的真实用法。仍不确定就去 scratchpad 解包 crate 源码读，**不要猜**。
4. **本目录只读**：不要在这里写 Impressy 自己的代码或笔记。

## 与 examples 对应的 Impressy 需求

`examples/` 里有几个直接对得上：

| example | 对应需求 |
| --- | --- |
| `input.rs` | **整个 examples 里唯一实现完整自定义 Element 的**。裁剪框（FR-01）、标注画布（FR-08）、文字图层（FR-11，v2）的首要参考 |
| `painting.rs` | `PathBuilder` + `canvas()` 低层绘制。画笔标注（FR-08）参考 |
| `gif_viewer.rs` | GIF 显示（FR-10） |
| `image_loading.rs` / `image_gallery.rs` | 图片加载与展示（FR-01、FR-02） |
| `drag_drop.rs` | 拖入图片（FR-03 批量） |
| `window.rs` / `window_positioning.rs` | 截图 overlay 窗口（FR-08） |
| `opacity.rs` / `shadow.rs` / `gradient.rs` | 截图美化的阴影/渐变（FR-07） |
| `uniform_list.rs` / `data_table.rs` | 批量任务列表（FR-03） |
| `text.rs` / `text_layout.rs` / `text_wrapper.rs` | 文本排版（FR-11，v2） |

> 素材文件（`examples/image/`、`examples/svg/`，约 4.5MB）未拷入——只有 `.rs` 源码有参考价值。需要时从 crates.io 重新下载 `.crate` 包。
