# Impressy

[![CI](https://github.com/willmove/impressy/actions/workflows/ci.yml/badge.svg)](https://github.com/willmove/impressy/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](#许可证)

[English](README.md) | **简体中文**

**Impressy** 是一款跨平台原生桌面图像工具箱（Rust + [GPUI](https://www.gpui.rs/)），聚焦轻量、快速的**本地图片处理**。它**本地优先**：v1 的每一项功能都完全离线，无需账号、无需 API Key。

> Windows 10/11 · macOS · Linux（X11 / Wayland）

---

## 项目阶段

Impressy 已按两个阶段建设：

- **v1 —— 本地功能（已实现）。** 完全离线的图片处理工具，不联网、不需要 API Key。屏幕截图、全局热键与屏幕取色仍不属于当前范围。
- **v2 —— AI 图片功能（已实现，待真实服务商验收）。** 文生图 / 图生图、12 个 AI 改图预设、13 个行业工具和海报设计均通过 BYOK 提供；Seedream、Nano Banana、OpenAI 密钥只存入操作系统凭据管理器。三个视频入口仍是明确的规划占位，不属于当前标记为 v1/v2 的图片功能范围。

范围如此切分的理由见 [ADR-0001](docs/adr/0001-v1-scope-local-only.md)。
截图能力剥离的决策见 [ADR-0003](docs/adr/0003-remove-screen-capture.md)。
v2 Provider 契约与调研结论见 [ADR-0004](docs/adr/0004-v2-provider-contract.md)。

## 功能（v1）

界面按四大**板块**组织，v1 填满其中两个本地板块：

### 基础图片处理

| 功能 | 说明 | 引擎 |
| --- | --- | --- |
| 裁剪 / 缩放 / 旋转 | 比例预设、无损 90° 旋转、高质量重采样 | `impressy_core::transform` |
| 拼图拼接 | 纵向、横向或网格拼接多张图 | `impressy_core::collage` |
| 批量处理 | 批量转换 / 压缩 / 缩放 / 加水印，失败项单独标记不中断整批 | `impressy_core::batch` · `watermark` |
| 切图 | 把图片切成精确的 N×M 网格 | `impressy_core::slice` |
| 二维码 | 生成与识别二维码，支持自定义颜色 | `impressy_core::qr` |
| EXIF 管理 | 查看并清除相机 / GPS 元数据，**像素分毫不变** | `impressy_core::exif` |
| 截图美化 | 为导入的现有图片添加圆角、内边距、渐变、描边、阴影；不包含屏幕捕获 | `impressy_core::beautify` |

### 创作输出

| 功能 | 说明 | 引擎 |
| --- | --- | --- |
| GIF 制作 | 把多帧合成 GIF（延迟 + 正序 / 倒序 / 乒乓） | `impressy_core::animation` |
| 海报设计 | 生成 9:16 AI 底图，拖拽 / 缩放三个原生文字图层并在本地离屏合成 | `impressy-ai` · `impressy_core::poster` |

### AI 生成与行业工具（v2）

- 文生图与参考图生成，参数随 Provider 能力动态调整，结果支持多选保存。
- 12 个 AI 改图预设；局部消除支持交互式选区与透明掩码。
- 13 个锁定提示词行业工具：老照片修复、证件照、头像、表情包、写真、试穿、改色、促销海报、平台适配、封面、文章配图、美食优化、室内预览。
- 对证件照、3:4 促销海报、头像和平台规格执行精确本地后处理。

## 架构

一个包含四个 crate 的 Cargo workspace：

| Crate | 职责 | 可无头测试？ |
| --- | --- | --- |
| [`impressy-core`](crates/impressy-core) | 纯本地图像引擎：裁剪、压缩、拼图、切图、GIF、二维码、EXIF、水印、美化。**无 UI、无网络、无全局状态。** | ✅ 完全可测 —— property test 覆盖 |
| [`impressy-app`](crates/impressy-app) | GPUI UI：四板块导航、功能页与自定义裁剪 Element | ⚠️ 无头机器上仅能类型检查 |
| [`impressy-ai`](crates/impressy-ai) | Provider trait、Seedream / Nano Banana / OpenAI 适配器、HTTPS、能力声明与系统凭据存储 | ✅ 假传输契约测试 |
| [`impressy-presets`](crates/impressy-presets) | AI 改图与 13 个行业工具的编译期锁定提示词模板 | ✅ 内嵌资源测试 |

设计原则：本地优先、隐私优先（无遥测、无内嵌密钥）、原生轻量（单二进制、**无浏览器内核依赖**）、GPU 直渲。

## 快速开始

### 前置条件

- **Rust** stable ≥ 1.85（workspace 使用 edition 2024）。
- *运行*应用需要桌面环境（GPUI 需要显示器）。编译与类型检查可在无头环境完成。

### 编译与测试

```bash
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`impressy-core` 由 property test 覆盖（如无损往返逐像素一致、裁剪比例精确、N 张图 → N 帧 GIF、EXIF 清除保持像素等不变量）。

### 运行应用

```bash
cargo run -p impressy-app
```

## 开发

- **GPUI 版本已精确锁定**（`gpui = "=0.2.2"`、`gpui-component = "=0.5.1"`）—— 这是唯一配套的一对。GPUI 是 pre-1.0；写 UI 代码前先查 [`vendor-docs/`](vendor-docs)（从锁定版本源码提取），不要凭模型记忆。
- **Definition of Done：** `cargo check && cargo clippy -- -D warnings && cargo test`，外加对应需求的验收标准。
- **CI** 在 Ubuntu / Windows / macOS 上运行完整质量门禁。
- 渲染正确性、自定义 Element 手感、冷启动耗时只能在真机上验收 —— 见 [ADR-0002](docs/adr/0002-core-before-ui.md) 与 [v1 桌面真机验收清单](docs/testing/v1-desktop-acceptance.md)。Core 回退可用[性能探针](docs/testing/performance-probe.md)对比；打包方式见[发布指南](docs/release.md)。
- 界面文案通过 `rust-i18n` 多语言（简体中文 + 英文），运行时切换无需重启。

## 文档

- [`docs/spec/requirements.md`](docs/spec/requirements.md) 与 [`docs/spec/design.md`](docs/spec/design.md) —— 唯一真相源。
- [`docs/adr/`](docs/adr) —— 范围与顺序决策（ADR 胜过 spec 里与之冲突的表述）。
- [`CONTEXT.md`](CONTEXT.md) —— 领域词汇表。

## 许可证

采用 [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0) 许可。
