# v1 发布指南

## 发布前提

发布候选必须先通过：

```shell
cargo check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo bench -p impressy-core --bench v1_local
```

随后完成 [`testing/v1-desktop-acceptance.md`](./testing/v1-desktop-acceptance.md) 的真机验收。
Core 探针只用于发现相对性能回退，不能代替 UI 响应与冷启动测量。
验收结果必须写入 `testing/results/v1-desktop-acceptance.json` 并通过
`python scripts/verify_desktop_acceptance.py`；tag 发布会先执行同一门禁。

## 应用图标

唯一源文件是 `crates/impressy-app/assets/impressy-logo.svg`。图标生成器继续放在外部 Termior
工具仓库，不复制到 Impressy。修改 SVG 后，在 PowerShell 中生成并校验全部平台资源：

```powershell
$generator = 'D:\Coding\ToolsProjects\termior\scripts\generate-icons.mjs'
$iconArgs = @(
  '--root', 'D:\Coding\DesignProjects\impressy\crates\impressy-app',
  '--source', 'assets\impressy-logo.svg',
  '--output', 'assets\icons',
  '--name', 'impressy',
  '--app-id', 'app.impressy.Impressy'
)
node $generator --write @iconArgs
node $generator --check @iconArgs
```

生成结果包括 Windows ICO、macOS ICNS、16–1024 px PNG，以及 Linux hicolor PNG/SVG。
这些产物需要随源码提交；CI 不依赖本机外部脚本，而是独立检查格式、尺寸和 SVG 源同步状态。

## GitHub Actions 产物

`.github/workflows/release.yml` 可手动运行，也会在推送 `v*` 标签时运行：

- Windows 2022：构建并签名 `impressy.exe`，使用固定版本 `cargo-wix 0.3.9` 生成 MSI，
  再签名和验证 MSI；EXE 内嵌应用图标，快捷方式、文件关联与“应用和功能”使用同一 ICO；
  EXE 或 MSI 超过 30 MiB 时失败。
- Ubuntu 22.04：生成包含运行库依赖声明、`.desktop` 与 hicolor 图标树的 amd64 `.deb`；
  二进制或安装包超过 30 MiB 时失败。
- macOS 15 arm64：用固定版本 `cargo-bundle 0.11.0` 生成 `.app`，以 Developer ID 签名，
  通过 Apple 公证并装订票据，最终生成已签名、公证的 DMG；二进制或 DMG 超过 30 MiB 时失败。
- `v*` tag 的三平台任务全部通过后，工作流创建 GitHub Release 并上传 MSI、DEB 与 DMG。

普通 CI 还会对未签名 MSI 与 `.deb` 执行安装包冒烟：Windows 校验安装目录、六种文件关联、
开始菜单快捷方式及卸载清理；Ubuntu 校验 desktop entry、包内容、实际安装和卸载清理。
这些自动检查用于提前发现打包回归，不能替代下文的签名状态、SmartScreen 或桌面真机验收。

Windows job 需要仓库 Actions secrets：

| Secret | 内容 |
| --- | --- |
| `WINDOWS_CERTIFICATE_BASE64` | 代码签名 PFX 文件的 Base64 文本 |
| `WINDOWS_CERTIFICATE_PASSWORD` | PFX 密码 |
| `MACOS_CERTIFICATE_BASE64` | Developer ID Application `.p12` 的 Base64 文本 |
| `MACOS_CERTIFICATE_PASSWORD` | `.p12` 密码 |
| `MACOS_SIGNING_IDENTITY` | `Developer ID Application: ... (TEAMID)` 完整身份 |
| `APPLE_ID` | 公证使用的 Apple ID |
| `APPLE_TEAM_ID` | Apple Developer Team ID |
| `APPLE_APP_PASSWORD` | Apple ID app-specific password |

在 PowerShell 中可用 `[Convert]::ToBase64String([IO.File]::ReadAllBytes("certificate.pfx"))`
生成第一个值。不要把证书、密码或解码后的 PFX 提交到仓库。

## Windows MSI

WiX 定义位于 `crates/impressy-app/wix/main.wxs`，安装范围包括：

- 内嵌 ICO 的 `impressy.exe` 与带图标的开始菜单快捷方式；
- PNG、JPEG、WebP、BMP、GIF 的「Open with Impressy」文件关联；
- “应用和功能”与文件关联使用 `assets/icons/impressy.ico`；
- Major Upgrade 与卸载清理。

本地生成 MSI 需要 Windows、WiX Toolset v3 和 `cargo-wix 0.3.9`：

```shell
cargo install cargo-wix --version 0.3.9 --locked
cargo build --release -p impressy-app
cargo wix --package impressy-app --no-build
```

安装器必须在 Windows 10/11 x64 真机验证安装、文件关联、升级、卸载、SmartScreen 与
签名状态。没有受信任的代码签名证书时，不得把 MSI 标记为正式发布产物。

## 正式分发边界

当前 Linux 正式产物是 Debian/Ubuntu amd64 的 `.deb`；其他发行版仍需后续增加对应原生包，
不得把 `.deb` 描述为全发行版通用包。CI 只证明构建、签名、公证和结构检查成功，不能替代
`testing/v1-desktop-acceptance.md` 中的 Windows、macOS、Linux X11/Wayland 真机安装与运行验收。
