## Why

工作区需要更大画面时，左侧导航侧栏占固定宽度且无法关闭。用户应能通过菜单与快捷键立刻隐藏侧栏，再按需恢复，而侧栏内部状态（搜索、分组折叠、当前选中）保持不变。

## What Changes

- 新增会话内布尔状态控制左侧导航侧栏的显示/隐藏；隐藏时整栏立刻不渲染，工作区占满横向空间。
- 「视图」菜单增加随状态切换的文案项：「隐藏侧栏」↔「显示侧栏」。
- 绑定快捷键 `Ctrl/Cmd+B`（gpui `secondary-b`）触发同一动作。
- 顶栏品牌徽标作为可选第三入口，本 change 可不实现。
- 侧栏显隐偏好本 change 不持久化（可后续单独加）。

## Capabilities

### New Capabilities

- `sidebar-visibility`: 左侧导航侧栏的显示/隐藏、菜单文案随状态切换、以及 `Ctrl/Cmd+B` 快捷键。

### Modified Capabilities

- （无；`openspec/specs/` 尚无既有能力。）

## Impact

- `impressy-app`：`AppShell` 渲染与状态、`menus.rs` 动作/快捷键/菜单构建、`locales/app.yml` 中英文案。
- 不影响 `impressy-core` / `impressy-ai` / `impressy-presets`。
- 无 API、依赖或跨 crate 契约变更。
