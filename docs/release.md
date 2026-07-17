# v1 发布指南

## 发布前提

发布候选必须先通过：

```shell
cargo check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo bench -p rastery-core --bench v1_local
```

随后完成 [`testing/v1-desktop-acceptance.md`](./testing/v1-desktop-acceptance.md) 的真机验收。
Core 探针只用于发现相对性能回退，不能代替 UI 响应与冷启动测量。

## 应用图标

唯一源文件是 `crates/rastery-app/assets/rastery-logo.svg`。图标生成器继续放在外部 Termior
工具仓库，不复制到 Rastery。修改 SVG 后，在 PowerShell 中生成并校验全部平台资源：

```powershell
$generator = 'D:\Coding\ToolsProjects\termior\scripts\generate-icons.mjs'
$iconArgs = @(
  '--root', 'D:\Coding\DesignProjects\rastery\crates\rastery-app',
  '--source', 'assets\rastery-logo.svg',
  '--output', 'assets\icons',
  '--name', 'rastery',
  '--app-id', 'app.rastery.Rastery'
)
node $generator --write @iconArgs
node $generator --check @iconArgs
```

生成结果包括 Windows ICO、macOS ICNS、16–1024 px PNG，以及 Linux hicolor PNG/SVG。
这些产物需要随源码提交；CI 不依赖本机外部脚本，而是独立检查格式、尺寸和 SVG 源同步状态。

## GitHub Actions 产物

`.github/workflows/release.yml` 可手动运行，也会在推送 `v*` 标签时运行：

- Windows 2022：构建并签名 `rastery.exe`，使用固定版本 `cargo-wix 0.3.9` 生成 MSI，
  再签名和验证 MSI；EXE 内嵌应用图标，快捷方式、文件关联与“应用和功能”使用同一 ICO；
  EXE 或 MSI 超过 30 MiB 时失败。
- Ubuntu 22.04：生成包含 `usr/bin`、`.desktop` 与 hicolor 图标树的 `tar.gz` 安装布局；
  二进制超过 30 MiB 时失败。
- macOS 15 arm64：用固定版本 `cargo-bundle 0.11.0` 生成包含 ICNS 的 `Rastery.app`，再归档为
  `tar.gz`；bundle 内二进制超过 30 MiB 时失败。

Windows job 需要仓库 Actions secrets：

| Secret | 内容 |
| --- | --- |
| `WINDOWS_CERTIFICATE_BASE64` | 代码签名 PFX 文件的 Base64 文本 |
| `WINDOWS_CERTIFICATE_PASSWORD` | PFX 密码 |

在 PowerShell 中可用 `[Convert]::ToBase64String([IO.File]::ReadAllBytes("certificate.pfx"))`
生成第一个值。不要把证书、密码或解码后的 PFX 提交到仓库。

## Windows MSI

WiX 定义位于 `crates/rastery-app/wix/main.wxs`，安装范围包括：

- 内嵌 ICO 的 `rastery.exe` 与带图标的开始菜单快捷方式；
- PNG、JPEG、WebP、BMP、GIF 的「Open with Rastery」文件关联；
- “应用和功能”与文件关联使用 `assets/icons/rastery.ico`；
- Major Upgrade 与卸载清理。

本地生成 MSI 需要 Windows、WiX Toolset v3 和 `cargo-wix 0.3.9`：

```shell
cargo install cargo-wix --version 0.3.9 --locked
cargo build --release -p rastery-app
cargo wix --package rastery-app --no-build
```

安装器必须在 Windows 10/11 x64 真机验证安装、文件关联、升级、卸载、SmartScreen 与
签名状态。没有受信任的代码签名证书时，不得把 MSI 标记为正式发布产物。

## 尚未自动化的正式分发

macOS job 已产出 `.app`，但尚未接入 Developer ID 签名与 Apple 公证；Linux job 已包含桌面文件
和图标安装树，但未制作 AppImage、Flatpak 或发行版原生包，且仍依赖目标系统运行库。因此这些
tar 包仍是构建测试产物，不应称为 portable 正式发布包。平台分发应在运行库和发布渠道确定后
独立完成，不能用 CI 成功替代平台发布验收。
