# 先写 rastery-core，GPUI 骨架推后（与 spec 的 M1→M7 顺序相反）

spec §8 的里程碑顺序是 M1 骨架与语料（GPUI 脚手架 + 主界面导航 + i18n）→ M2 本地引擎。**我们反过来：先把 `rastery-core` 写到全绿，GPUI 骨架推到之后。** 原因是一条代码里看不见的约束——主开发机是一台无图形环境的云 VM——它使得两半代码的 AI 生成循环速度相差一个数量级。

## 代码里看不见的约束：开发机跑不了 GPUI

主开发环境是 KVM 云 VM，Ubuntu 24.04，4 核 / 7.8G，显卡为 QEMU 模拟的 Cirrus Logic GD 5446（无硬件加速）。`DISPLAY` 未设置，`XDG_SESSION_TYPE=tty`，无任何显示服务器进程，`xrandr` 报告 0 个连接的显示器。已安装 rustup target 只有 `x86_64-unknown-linux-gnu`，且无 mingw-w64 / cargo-xwin / lld-link，因此**连 `cargo check --target x86_64-pc-windows-msvc` 都无法执行**。

（Mesa 的 Vulkan ICD 含 `lvp`（Lavapipe 软件光栅化器），理论上可用 Xvfb + Lavapipe 做离屏冒烟测试。但软渲染下 NFR-03「冷启动 ≤ 1.5 秒」不可测量，FR-08 截图无屏可截，FR-09 取色无像素可取，自定义 Element 的交互手感也测不出来。）

真机验收在另外的机器上进行（Windows / macOS / Linux 桌面 + 多显示器混合 DPI 环境均具备）。

## 由此产生的循环不对称

- **`rastery-core` 是理想的 AI 循环**：Claude Code 写 → VM 上 `cargo test` → 自动收敛到全绿，开发者完全不必在场。图片进图片出的纯函数与 property test 全部 headless 可跑。spec 附录 4 所说的「以编译器为评审、自我修正循环」**只在这一半成立**。
- **`rastery-app` / `rastery-capture` 是较慢的循环**：VM 上能**类型检查**但不能**运行**。写完只能确认「编译得过」，要确认「渲染对不对、截图准不准、手感如何」必须 push → 在真机 pull、build、运行、肉眼判断 → 再把结果讲回来。**行为验证的每一次迭代开发者都必须在场。**

> **2026-07-15 更正**：本 ADR 初版称「VM 上连编译检查都做不了」，**该说法有误**。它对 Windows target 成立（无交叉工具链），但对 **Linux host target 是错的**——已实测：`gpui 0.2.2` + `gpui-component 0.5.1` 在本 VM 上 `cargo check` **3 分 14 秒干净通过**，756 个依赖，无缺失系统库。
>
> 这缩小了两半的差距，但**不推翻本 ADR 的结论**：
>
> - 好消息是，GPUI pre-1.0 的幻觉 API 风险**恰好是类型错误**，而类型检查在本机就能自动抓住。UI 代码的「写错 API」这一类问题不必等真机。
> - 但 core 的循环仍然严格更优：core 能在本机验证**行为**（property test），UI 只能验证**类型**。「能编译」离「渲染正确」很远。
> - 仍然验不了的：渲染正确性、FR-08 截图与 DPI 精度、FR-09 取色、自定义 Element 的交互手感、NFR-03 冷启动耗时。
> - 仍然检查不了的：`#[cfg(windows)]` / `#[cfg(target_os = "macos")]` 门控的平台特定代码——在 Linux 上编译时它们根本不参与编译。`rastery-capture` 的平台后端大部分属于此类，所以它是三个 crate 里本机覆盖最差的一个。

## 为什么 core 能立刻开工而 UI 不能

堵死 AI codegen 的那个前提——GPUI 是 pre-1.0、模型记忆中的 API 大概率过时，故 spec 附录 2 要求先建 `vendor-docs/`——**对 `rastery-core` 完全不适用**。core 依赖的 `image`、`imageproc`、`fast_image_resize`、`kamadak-exif`、`qrcode`、`gif` 全是 API 稳定、文档完备、模型掌握良好的 crate。

> **2026-07-15 更新**：本 ADR 写作时 `vendor-docs/` 不存在、GPUI 版本未锁定，故 UI 无法开工。**该门槛现已清除**：版本锁定为 `gpui = "=0.2.2"` + `gpui-component = "=0.5.1"`，`vendor-docs/` 已建立（内容全部从锁定版本的真实 crate 源码提取）。spec §9.2 第 5 项已关闭。
>
> 但**本 ADR 的结论不变，core 仍然优先**——门槛的清除不改变循环速度的差异（见上文「更正」）：core 能在本机验证行为，UI 只能验证类型。core 优先的理由从「UI 还开不了工」变成了「core 的反馈回路严格更短」。

## 后果

- **i18n 不在第一阶段**。`rastery-core` 是库，没有任何用户可见文本，spec 附录 7 的「禁止硬编码、缺失键视为编译错误」对它无事可做。i18n 应在 UI 动工的那一刻就位，而非在库动工时。
- ~~**`vendor-docs/` 与 GPUI 版本锁定**从 spec M1 的第一个任务，变为 UI 阶段启动前的准入条件。它仍然是硬门槛，只是不再阻塞开工。~~ **已于 2026-07-15 完成**，见 [`vendor-docs/README.md`](../../vendor-docs/README.md)。锁定 `gpui = "=0.2.2"` + `gpui-component = "=0.5.1"`。
- **UI 代码写完后必须在本机 `cargo check`**（Linux host target）。这能自动抓住幻觉 API——GPUI pre-1.0 的主要风险形态就是类型错误。不要跳过这一步直接推给真机。
- **`cargo check` 通过 ≠ 可以声称「已验证」**。渲染、截图、DPI、手感、冷启动全部需要真机。涉及这些的改动，如实说明「本机只做了类型检查」。
- `rastery-core` 的三平台回归由 GitHub Actions（`windows-latest` / `macos-latest` / `ubuntu-latest`）承担，零人工成本。CI 也可编译 `rastery-app` 以捕获 API 误用与平台条件编译错误，但无法验证渲染正确性、DPI 精度或冷启动耗时——CI runner 同样是无真实多显示器的虚拟机。

## 进度快照（2026-07-16，更新）

按本 ADR 的「core 优先、按可验证性分层」原则，v1 三个 crate 的当前状态：

| crate | 已完成且**本机可验证** | 已写但**只类型检查**（待真机） |
| --- | --- | --- |
| `rastery-core` | 全部本地引擎 + 31 个测试（含 Req 52–58 property test）全绿 | —— |
| `rastery-capture` | `annotate` 画笔/马赛克纯合成 + 6 个 property test 全绿 | `device`（截图/热键/取色/多屏 DPI）、`clipboard` 真实平台后端已写，`cargo check --features system` 在本机编译通过——但行为须真机验收 |
| `rastery-app` | 编译干净（含 `--features system`）；四板块导航（Req 34，新增「屏幕截图」入口）、中英切换（Req 35） | **自定义 Element 已实现**：`crop_frame`（选区拖拽/缩放/比例锁，Req 47.1–3）、`annotate_canvas`（画笔/马赛克标注，Req 47.4–7）、`capture::overlay` 截图覆盖层（区域选择 + 取色读数 + 标注 + 保存/复制，Req 13/14/48）。渲染、手感、DPI 精度须真机验收 |

**已实现但须真机验收的 v1**：自定义 Canvas_Element（Req 47）、截图覆盖层与区域捕获（Req 13/14/48）。真实屏幕抓取走 `system` feature（`xcap`），全局热键的事件泵→GPUI 主循环桥接（Req 13.1/49）尚未接通——须真机调试。不得在本机声称其「已验证」。
