## 1. i18n

- [x] 1.1 在 `locales/app.yml` 增加 `menu.hide_sidebar` / `menu.show_sidebar`（en + zh-CN）

## 2. 菜单与动作

- [x] 2.1 在 `menus.rs` 增加 `ToggleSidebar` action，绑定 `secondary-b`，并在 `register_actions` 转发到 `AppShell`
- [x] 2.2 调整 `build_menus` 接受侧栏是否打开，视图菜单项文案在 hide/show 间切换
- [x] 2.3 更新 `menus.rs` 内依赖 `build_menus()` 的测试（若有）使签名与行为一致

## 3. AppShell 状态与渲染

- [x] 3.1 在 `AppShell` 增加 `sidebar_open: bool`（默认 `true`）与 `toggle_sidebar` 处理器：翻转状态、`set_menus`、`notify`
- [x] 3.2 `set_language` 重建菜单时传入当前 `sidebar_open`
- [x] 3.3 `AppShell::render` 中侧栏子树按 `sidebar_open` 条件挂载（隐藏时立刻不占布局）

## 4. 验证

- [x] 4.1 运行 `cargo check && cargo clippy -- -D warnings && cargo test`
- [x] 4.2 在 Windows 真机手动确认：菜单文案随状态切换、`Ctrl+B` 切换、展开后搜索/分组/选中保持、语言切换后文案仍对应当前可见性
