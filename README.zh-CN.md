# Rastery

[![CI](https://github.com/willmove/rastery/actions/workflows/ci.yml/badge.svg)](https://github.com/willmove/rastery/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](#许可证)

[English](README.md) | **简体中文**

**Rastery** 是一款跨平台原生桌面图像工具箱（Rust + [GPUI](https://www.gpui.rs/)），把**屏幕截图**与**图片处理**二合一。它**本地优先**：v1 的每一项功能都完全离线，无需账号、无需 API Key。

> Windows 10/11 · macOS · Linux（X11 / Wayland）

---

## 项目阶段

Rastery 分两阶段交付：

- **v1 —— 本地功能（当前重点）。** 完全离线的图像与截图工具（FR-01 – FR-10），不联网、不需要 API Key。
- **v2 —— AI 功能（推迟）。** 文生图、AI 改图、行业预置工具，通过用户自备 API Key（BYOK）实现。两个 AI 板块与三个视频工具已在界面中作为「开发中」占位存在。

范围如此切分的理由见 [ADR-0001](docs/adr/0001-v1-scope-local-only.md)。

## 功能（v1）

界面按四大**板块**组织，v1 填满其中两个本地板块：

### 基础图片处理

| 功能 | 说明 | 引擎 |
| --- | --- | --- |
| 裁剪 / 缩放 / 旋转 | 比例预设、无损 90° 旋转、高质量重采样 | `rastery_core::transform` |
| 拼图拼接 | 纵向、横向或网格拼接多张图 | `rastery_core::collage` |
| 批量处理 | 批量转换 / 压缩 / 缩放 / 加水印，失败项单独标记不中断整批 | `rastery_core::batch` · `watermark` |
| 切图 | 把图片切成精确的 N×M 网格 | `rastery_core::slice` |
| 二维码 | 生成与识别二维码，支持自定义颜色 | `rastery_core::qr` |
| EXIF 管理 | 查看并清除相机 / GPS 元数据，**像素分毫不变** | `rastery_core::exif` |
| 截图美化 | 圆角、内边距、渐变、描边、阴影 | `rastery_core::beautify` |
| 截图 + 标注 | 区域截图、实时取色、画笔与马赛克标注、全局热键 | `rastery-capture` |

### 创作输出

| 功能 | 说明 | 引擎 |
| --- | --- | --- |
| GIF 制作 | 把多帧合成 GIF（延迟 + 正序 / 倒序 / 乒乓） | `rastery_core::animation` |

## 架构

一个包含三个 crate 的 Cargo workspace（v1）：

| Crate | 职责 | 可无头测试？ |
| --- | --- | --- |
| [`rastery-core`](crates/rastery-core) | 纯本地图像引擎：裁剪、压缩、拼图、切图、GIF、二维码、EXIF、水印、美化。**无 UI、无网络、无全局状态。** | ✅ 完全可测 —— property test 覆盖 |
| [`rastery-capture`](crates/rastery-capture) | 系统集成：屏幕捕获、全局热键、剪贴板、取色，以及纯画笔 / 马赛克标注合成 | ⚠️ 仅标注可测；平台后端在 `system` feature 后 |
| [`rastery-app`](crates/rastery-app) | GPUI UI：四板块导航、功能页、自定义裁剪 / 标注 Element、截图覆盖层 | ⚠️ 无头机器上仅能类型检查 |

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

`rastery-core` 与标注引擎由 property test 覆盖（如无损往返逐像素一致、裁剪比例精确、N 张图 → N 帧 GIF、EXIF 清除保持像素等不变量）。

### 运行应用

```bash
cargo run -p rastery-app
```

默认构建**不含**真实平台捕获后端（因此可在任何地方编译，包括 CI 与无头 VM）。要启用真实的屏幕捕获、全局热键、系统剪贴板与取色，请启用 `system` feature：

```bash
cargo run -p rastery-app --features system
```

在 **Linux** 上，`system` feature 会链接原生捕获库，请先安装开发包：

```bash
sudo apt-get install -y pkg-config clang libclang-dev \
  libwayland-dev libpipewire-0.3-dev libdbus-1-dev \
  libxcb1-dev libxcb-randr0-dev libxcb-shm0-dev libxcb-xfixes0-dev \
  libxrandr-dev libx11-dev libegl-dev libgl-dev
```

Windows 与 macOS 使用系统原生 API，无需额外安装。

## 开发

- **GPUI 版本已精确锁定**（`gpui = "=0.2.2"`、`gpui-component = "=0.5.1"`）—— 这是唯一配套的一对。GPUI 是 pre-1.0；写 UI 代码前先查 [`vendor-docs/`](vendor-docs)（从锁定版本源码提取），不要凭模型记忆。
- **Definition of Done：** `cargo check && cargo clippy -- -D warnings && cargo test`，外加对应需求的验收标准。
- **CI** 在 Ubuntu / Windows / macOS 上跑质量门禁，并单独在三平台编译 `system` feature，使平台后端在真实目标上得到编译校验。
- 渲染正确性、截图 / DPI 精度、取色、自定义 Element 手感、冷启动耗时只能在真机上验收 —— 见 [ADR-0002](docs/adr/0002-core-before-ui.md)。
- 界面文案通过 `rust-i18n` 多语言（简体中文 + 英文），运行时切换无需重启。

## 文档

- [`docs/spec/requirements.md`](docs/spec/requirements.md) 与 [`docs/spec/design.md`](docs/spec/design.md) —— 唯一真相源。
- [`docs/adr/`](docs/adr) —— 范围与顺序决策（ADR 胜过 spec 里与之冲突的表述）。
- [`CONTEXT.md`](CONTEXT.md) —— 领域词汇表。

## 许可证

采用 [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0) 许可。
