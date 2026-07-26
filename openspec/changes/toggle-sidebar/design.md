## Context

`AppShell` 手写左侧导航（固定 216px），顶栏品牌徽标目前不可点；分组折叠用 `collapsed_groups`，没有整栏显隐。菜单经 `build_menus()` + `App::set_menus` 注册；Windows/Linux 由自研 `MenuBar` 读同一份数据，macOS 用系统菜单栏。语言切换时已会重建菜单。

本 change 只动 `impressy-app` 壳层，不动图像引擎与 AI crate。

## Goals / Non-Goals

**Goals:**

- 会话内布尔控制侧栏整栏显示/隐藏；隐藏时立刻不渲染，工作区占满横向空间。
- 「视图」菜单文案随状态切换：「隐藏侧栏」↔「显示侧栏」（中英同步）。
- 快捷键 `Ctrl/Cmd+B`（`secondary-b`）与菜单项共用同一动作。
- 展开后侧栏内部状态与收起前一致（搜索、分组折叠、当前功能选中等仍挂在 `AppShell` 上，仅条件渲染）。

**Non-Goals:**

- 顶栏品牌徽标按钮（可选后续）。
- 显隐偏好持久化到 `AppConfig`。
- 宽度动画、可拖拽改宽、图标轨（gpui-component `Sidebar::collapsed`）。
- 迁移到手写侧栏以外的组件库 `Sidebar`。

## Decisions

### 1. 状态：`sidebar_open: bool`，默认 `true`

挂在 `AppShell`。`true` = 渲染现有侧栏子树；`false` = 省略该子树。不销毁 `nav_search` / `collapsed_groups` / `open` 等，故「恢复当前」自然成立。

**备选**：迁到 `gpui-component::Sidebar` 的 `collapsed`（48px 轨）——与「完全隐藏」需求不符，且重构面大，否决。

### 2. 动作：`ToggleSidebar` + 处理器翻转布尔并重建菜单

在 `menus.rs` 的 `actions!` 中新增 `ToggleSidebar`；`install` 绑定 `KeyBinding::new("secondary-b", ToggleSidebar, None)`；`register_actions` 转发到 `AppShell::toggle_sidebar`（或等价名）。处理器：`sidebar_open = !sidebar_open`，然后 `cx.set_menus(build_menus(/* 当前可见性 */))`，`cx.notify()`。

### 3. `build_menus` 接受侧栏可见性以选择菜单文案

今日 `build_menus()` 无参。改为传入侧栏是否打开（或「下一项应显示的文案键」），在「视图」菜单中：

- 打开时：`menu.hide_sidebar`（隐藏侧栏 / Hide Sidebar）
- 关闭时：`menu.show_sidebar`（显示侧栏 / Show Sidebar）

`AppShell::set_language` 重建菜单时必须带上当前 `sidebar_open`，避免语言切换把文案打回错误状态。

**备选**：固定「切换侧栏」——实现更简单，但不符合已定产品文案。

### 4. 渲染：条件挂载，无动画

`AppShell::render` 中侧栏 `v_flex` 用 `.when(self.sidebar_open, …)`（或等价）包裹；`false` 时工作区 `flex_1` 自然占满。不做宽度过渡。

### 5. i18n

在 `locales/app.yml` 增加 `menu.hide_sidebar` / `menu.show_sidebar`（`en` + `zh-CN`），禁止硬编码。

## Risks / Trade-offs

- **[Risk] 菜单打开时切换状态，下拉缓存仍显示旧文案** → Mitigation：与现有 `MenuBar` 弹层缓存行为一致；再次打开「视图」会读新的 `get_menus()`。可接受。
- **[Risk] `secondary-b` 与未来输入框快捷键冲突** → Mitigation：当前全局绑定无 `b`；输入框聚焦时若组件库未占用则全局生效。若日后冲突再收窄 context。
- **[Trade-off] 不持久化** → 重启后侧栏总是展开；换取更小 diff，后续可加配置字段。

## Migration Plan

纯 UI 行为增量，无数据迁移。回滚即去掉状态、动作与菜单项。

## Open Questions

无（快捷键、文案形态、动画、按钮与持久化范围已在探索中拍板）。
