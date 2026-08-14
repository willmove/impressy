## 目标

在 `feature/menus` 分支上落地 P0,让菜单栏「丰满」起来。全部是 UI 层改动(`impressy-app`),不动 `impressy-core`。新增 **7 个菜单动作**,两端(macOS 原生 / Win·Linux 自研 `MenuBar`)共享同一份 `set_menus` 数据。

---

## 改动清单(按文件)

### 1. `crates/impressy-app/src/menus.rs`(菜单核心)

**(a) `actions!` 宏新增 7 个动作**(带文档注释,沿用现有风格):
- `ShowShortcuts` — Help · 快捷键参考
- `OpenDocs` — Help · 在线文档
- `OpenIssueTracker` — Help · 反馈问题
- `GoToBasicImage` / `GoToAiGeneration` / `GoToIndustryTools` / `GoToCreativeOutput` — View · Go to 板块跳转

**(b) `build_menus(sidebar_open)` 更新结构**(签名不变,避免动 4 个测试调用点):

```
View:
  Home                         (GoHome)
  Go to ▶                      (Submenu)
      Basic Image Processing   (GoToBasicImage)
      AI Generation & Editing  (GoToAiGeneration)
      Industry AI Tools        (GoToIndustryTools)
      Creative Output          (GoToCreativeOutput)
  ───
  Hide/Show Sidebar            (ToggleSidebar)
  Switch Language              (SwitchLanguage)
Help:
  Keyboard Shortcuts…          (ShowShortcuts)
  Online Documentation…        (OpenDocs)
  Report an Issue…             (OpenIssueTracker)
  ───
  About Impressy               (ShowAbout)
```
- Go to 用 `MenuItem::submenu(Menu { name: tr("menu.goto"), items: [...] })`,文案复用已有的 `nav.basic_image` / `nav.ai_generation` / `nav.industry_tools` / `nav.creative_output`(不需要新建 i18n 键)。
- **不做勾选态**:`MenuItem::Action` 无 checked 字段,做勾选需改签名且收益低,明确排除。

**(c) `install()` 补快捷键**(`bind_keys`):
- `GoHome` → `secondary-shift-h`
- `SwitchLanguage` → `secondary-shift-l`
- `RevealOutputDirectory` → `secondary-shift-r`(顺手补全第 4 个无快捷键项)
- 其余新动作按惯例无快捷键。

**(d) `register_actions` 接 7 个 `on_menu_action!`** → 对应 `shell.rs` 处理器。

**(e) Windows/Linux 子菜单渲染修复**(`build_popup` 的 match,当前 `menus.rs:257-265` 的 `_ => popup` 吞掉了 `OwnedMenuItem::Submenu`):
```rust
OwnedMenuItem::Submenu(sub) => popup.submenu(
    sub.name.clone(), window, cx,
    |menu, window, cx| menu.with_menu_items(sub.items.clone(), window, cx),
),
```
`PopupMenu` 原生支持 `.submenu()`(`popup_menu.rs:604`)和递归 `with_menu_items()`(`:666`)。Go to 子菜单只 4 项,远低于 20 项触发自动 scrollable 的阈值。

**(f) 修正模块顶部过时文档注释**(`menus.rs:3-5` 称用 `AppMenuBar`,但 `:185-187` 又说不用 —— 改为「自研 `Button`+`PopupMenu`」与实际一致)。

**(g) 测试更新**:
- `menus_cover_every_action_exactly_once`:`expected` 数组加 7 个动作名;`menus.len()` 断言保持 **4**(Go to 是 View 内的 submenu,不新增顶层菜单)。
- 另两个测试(menu 名 i18n、sidebar 文案)不受影响。

### 2. `crates/impressy-app/src/shell.rs`(处理器 + About)

**(a) 新增 7 个 `menu_*` 处理器**(`shell.rs:1152-1242` 区域,签名 `(&mut self, _: &mut Window, cx: &mut Context<Self>)`):
- `menu_show_shortcuts` → `window.open_dialog` 渲染快捷键对照表(两列:操作名 | 键位)。修饰键用 helper 平台化(macOS `⌘`,其余 `Ctrl`)。
- `menu_open_docs` → `cx.open_url(IMPRESSY_README_URL)`。
- `menu_open_issue_tracker` → `cx.open_url(IMPRESSY_ISSUES_URL)`。
- `menu_goto_basic_image` → `self.open_feature(Feature::Edit, cx)`,其余 3 个分别跳 `TextToImage` / `OldPhotoRestoration` / `Collage`(各板块首个 feature,来自 `navigation_groups`)。
- **`menu_show_about` 扩展 body**(不加 footer 按钮,规避 Dialog footer API 风险;打开外链由 Help 菜单承担):在现有「标题/版本/副标题」后追加 License 行、仓库 URL 文本、`Built with …`(gpui · gpui-component · image · …)致谢行。

**(b) URL 常量**(模块级 `const`):
- `IMPRESSY_README_URL = "https://github.com/willmove/impressy#readme"`
- `IMPRESSY_ISSUES_URL = "https://github.com/willmove/impressy/issues/new"`
(取自 `git remote`:origin = `willmove/impressy`,公开仓库。)

### 3. `crates/impressy-app/locales/app.yml`(i18n,`menu:` 段新增,en + zh-CN 双套)

| 新 key | en | zh-CN |
|---|---|---|
| `menu.goto` | Go to | 转到 |
| `menu.shortcuts` | Keyboard Shortcuts… | 快捷键参考… |
| `menu.docs` | Online Documentation… | 在线文档… |
| `menu.report_issue` | Report an Issue… | 反馈问题… |
| `menu.about_license` | License | 许可证 |
| `menu.about_repo` | Repository | 仓库 |
| `menu.about_built_with` | Built with | 构建自 |
| `menu.kbd_*`(快捷键对照表用,约 10 个) | Open Image / Paste / … | 打开图片 / 粘贴 / … |

(板块名复用 `nav.*`,不新增。)

---

## 不在本次范围(留给后续)

- About 对话框的交互按钮(主页/复制)—— 需 Dialog footer API,留作 P1。
- Recent Files 子菜单、预览缩放、Undo/Redo、主题切换 —— 需后端能力,属 P1/P2。

---

## 风险与验证

1. **Win/Linux 子菜单首次启用**:`PopupMenu::submenu` 本仓库首次使用,需本机 Windows release 验证渲染与点击分发(macOS 原生呈现理论上开箱即用,但本机无法验证,留 CI/后续真机)。
2. **质量门禁**:`cargo check && cargo clippy -- -D warnings && cargo test` 全绿(尤其 `menus_cover_every_action_exactly_once` 同步)。
3. **不碰 `impressy-core`**,不引入网络/Provider 依赖;`cx.open_url` 仅在用户主动点击「文档/反馈」时调用,不违反 v1 离线原则。