# gpui 0.2.2 — 关键 API 真实签名

全部签名摘自 `gpui 0.2.2` 的真实 crate 源码。**不要用你记忆里的 GPUI API。**

> **本文档已经编译器验证**（2026-07-15，`gpui = "=0.2.2"`）：
> 1. 下文的 `examples/hello_world.rs` 入口代码 —— `cargo check` 通过。
> 2. 下文 `Element` trait 的完整签名 —— 照抄实现了一个最小自定义 Element，`cargo check` 通过。
>
> 即：这里的签名不是「看起来合理」，是**实测编译得过**的。

---

## ⚠️ 记忆陷阱：这些类型在 0.2.2 中已不存在

GPUI 做过一次大规模 context 重构。模型记忆中的 GPUI 多半是**重构之前**的版本，那套 API 几乎每一行都编译不过。已在 0.2.2 源码中逐一核实：

| 记忆中的旧 API | 0.2.2 中的真实情况 |
| --- | --- |
| `WindowContext` | **不存在**。已拆为 `window: &mut Window` + `cx: &mut App` 两个参数 |
| `ViewContext<T>` | **不存在**。改用 `cx: &mut Context<'_, T>` |
| `AppContext`（结构体） | 现在是 **trait**，不是结构体。主结构体叫 **`App`** |
| `cx.new_view(...)` | 改为 **`cx.new(...)`** |
| `App::new().run(...)` | 入口改为 **`Application::new().run(|cx: &mut App| ...)`** |

仍然存在的：`AppContext`（trait）、`VisualContext`（trait，`: AppContext`）、`AsyncWindowContext`（struct）、`AsyncApp`（struct）、`Context<'a, T>`（struct）。

**判断方法**：若你写出的代码里出现 `WindowContext` 或 `ViewContext`，一定是记忆污染，停下来查 `examples/`。

---

## 入口与最小应用

摘自 `examples/hello_world.rs`（真实可编译）：

```rust
use gpui::{
    App, Application, Bounds, Context, SharedString, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size,
};

struct HelloWorld { text: SharedString }

impl Render for HelloWorld {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().flex().flex_col().bg(rgb(0x505050)).size(px(500.0))
            .child(format!("Hello, {}!", &self.text))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(500.), px(500.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| HelloWorld { text: "World".into() }),
        )
        .unwrap();
        cx.activate(true);
    });
}
```

注意 `cx.open_window` 的闭包签名是 `|_, cx|`（两个参数），视图创建是 `cx.new(...)`。

## prelude

```rust
pub use crate::{
    AppContext as _, BorrowAppContext, Context, Element, InteractiveElement, IntoElement,
    ParentElement, Refineable, Render, RenderOnce, StatefulInteractiveElement, Styled, StyledImage,
    VisualContext, util::FluentBuilder,
};
```

建议 `use gpui::prelude::*;`，避免逐个 import trait。

## Render / RenderOnce

```rust
pub trait Render: 'static + Sized {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement;
}

pub trait RenderOnce: 'static {
    // 与 Render::render 不同，此方法接管 self 的所有权
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement;
}
```

---

## Element trait —— 三阶段管线（自定义 Element 的核心）

spec §3.4 要求交互式画布（裁剪框拖拽、画笔/马赛克标注、海报文字图层拖动）以自定义 Element 实现。**这是 0.2.2 的真实签名**：

```rust
pub trait Element: 'static + IntoElement {
    type RequestLayoutState: 'static;
    type PrepaintState: 'static;

    fn id(&self) -> Option<ElementId>;

    fn source_location(&self) -> Option<&'static panic::Location<'static>>;

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState);

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState;

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    );
}
```

**极易遗漏的两点**：

1. **`inspector_id: Option<&InspectorElementId>`** —— 三个方法都有这个参数。旧版没有，凭记忆写必漏。
2. **`window: &mut Window, cx: &mut App`** 是两个独立参数，不是一个 `WindowContext`。

`source_location()` 与 `id()` 也是必须实现的（不是 provided method）。

**参考实现**：`examples/input.rs` —— 整个 examples 目录里**唯一**实现完整自定义 Element 的示例。做裁剪框/标注画布之前先读它。

---

## canvas() —— 不写完整 Element 的低层绘制出口

并非所有自定义绘制都需要完整 Element。`canvas()` 是官方提供的捷径：

```rust
/// Construct a canvas element with the given paint callback.
/// Useful for adding short term custom drawing to a view.
pub fn canvas<T>(
    prepaint: impl 'static + FnOnce(Bounds<Pixels>, &mut Window, &mut App) -> T,
    paint: impl 'static + FnOnce(Bounds<Pixels>, T, &mut Window, &mut App),
) -> Canvas<T>
```

`Canvas<T>` 自身就是一份完整的三阶段 Element 实现（见 `gpui/src/elements/canvas.rs`），可作为写自己的 Element 时的样板。

**怎么选**：

- **`canvas()`** —— 纯绘制、不需要跨帧状态与命中测试。例如画笔轨迹渲染、马赛克预览。
- **完整 `impl Element`** —— 需要 `ElementId`、跨帧状态、命中测试、手势。例如裁剪框（拖拽把手）、文字图层（选中与拖动）。

`examples/painting.rs` 演示 `PathBuilder::fill()` + 低层绘制路径。

---

## 相关 features

`gpui 0.2.2` 的 feature 列表（真实）：

```
default, inspector, leak-detection, macos-blade, runtime_shaders,
screen-capture, test-support, wayland, windows-manifest, x11
```

与 Rastery 直接相关的：

- **`screen-capture`** —— FR-08 屏幕截图。启用前先确认它与 `xcap` 的分工（spec §3.4 计划用 `xcap` 做跨平台截图，可能与此 feature 重叠，需要决策）。
- **`x11` / `wayland`** —— Linux 后端。Linux 上截图还涉及 portal 权限，需实机验证。
- **`windows-manifest`** —— Windows 打包相关。
- **`test-support`** —— 测试用，`dev-dependencies` 里启用。
- **`inspector`** —— 与 `gpui-component` 的 `inspector` feature 配套。

## 官方文档

`upstream-docs/` 是 gpui 0.2.2 自带的文档，原样拷贝：

- **`contexts.md`** —— context 参数体系（`App`、`Context<T>`、`Window` 等的关系）。**理解重构后的 context 模型必读。**
- **`key_dispatch.md`** —— 按键分发与 action 体系。FR-08 全局热键相关。
