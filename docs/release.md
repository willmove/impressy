# GitHub Release Process

本 runbook 是把稳定版 Impressy 发布到 GitHub 的规范流程。步骤编排对齐 Markion
的 `docs/release-process.md`（环境检查 → 变更摘要 → 版本同步 → 验证 → tag →
监控 → 整理 notes → 终验），并保留 Impressy 特有的桌面验收门禁、代码签名与公证要求。

一次发布只有在下列全部成立后才算完成：tag 工作流成功、三平台安装包已挂到 Release、
双语整理后的 release notes 已写入、仓库与 GitHub 状态一致。

目标版本由维护者指定；当前请求为 **v0.1.0**。

## 1. Defaults and prerequisites

- 从 `main` 发布：工作树干净，且本地 `main` 与 `origin/main` 同步。
- 使用维护者明确给出的版本号。未指定时，才从最高稳定 `vMAJOR.MINOR.PATCH` tag 递增 PATCH。
  不要自行推断 major / minor / prerelease。
- 默认发布稳定、非 draft Release；仅在维护者明确要求时才做 prerelease 或 draft。
- Release notes 默认中英双语：英文在前，简体中文在后。
- 保护已公开 tag：未经明确授权，不得删除、强推或重建已发布 tag。
- 必备工具：stable Rust / Cargo、Git、GitHub CLI（`gh`）、OpenSpec CLI、
  以及有 push / 发布权限的已认证 GitHub 账户。

发布前检查：

```bash
gh auth status
gh repo view --json nameWithOwner,defaultBranchRef,url,isPrivate
git fetch --tags origin
git status --short --branch
git tag --sort=-version:refname
git log --oneline --decorate -20
```

确认：

- 仓库是 `willmove/impressy`，默认分支是 `main`。
- 工作树没有未提交的发布相关改动。
- `main` 相对 `origin/main` 既不落后，也不异常超前；干净分支可 fast-forward，发现分叉则停下排查。
- 目标版本尚未作为本地 tag、远程 tag 或 GitHub Release 存在。
- 正式验收文件 `docs/testing/results/v1-desktop-acceptance.json` 已提交，且
  `python scripts/verify_desktop_acceptance.py` 通过（见第 4 节）。

碰撞检查：

```bash
git tag --list v0.1.0
git ls-remote --tags origin v0.1.0
gh release view v0.1.0
```

对新版本，`gh release view` 预期返回 not-found。

### Impressy 相对 Markion 的硬差异

| 项 | Markion | Impressy |
| --- | --- | --- |
| 打包 | `cargo-packager`（NSIS / DMG / DEB / AppImage） | WiX MSI、DEB、`cargo-bundle` + 公证 DMG |
| 签名 | 通常未签名，notes 中说明绕过步骤 | Windows Authenticode + macOS Developer ID / 公证为正式发布前提 |
| 发布门禁 | 原生三平台构建成功即可发 Release | **先**通过桌面验收 JSON 验证器，再构建并上传安装包 |
| 体积 | 按 packager 产物 | EXE / MSI / DEB / DMG / 二进制均 ≤ 30 MiB |

## 2. Build the change summary

以「上一稳定 tag」为对比基线；若尚无 tag（例如首发 `v0.1.0`），用仓库初始有意义的基线或完整 `git log`。
审阅完整提交与 diff，不要只依赖 GitHub 自动生成 notes：

```bash
git log --oneline <previous-tag>..HEAD
git diff --stat <previous-tag>..HEAD
openspec list
```

阅读已完成的 OpenSpec proposal / delta specs / design / tasks。Release notes 写用户可见行为，
不要照抄 commit subject 或罗列内部工具改动。

发布前明确：

- 主要用户可见功能与改进
- 重要缺陷修复
- 兼容性、迁移与已知限制
- 平台支持或安装包变化
- 可如实写进 Verification 的证据（桌面验收 JSON、CI、本地质量门禁）

## 3. Synchronize version metadata

当前 workspace 版本字段集中在根 `Cargo.toml` 的 `[workspace.package].version`，
各 crate 使用 `version.workspace = true`。目标版本为 `0.1.0` 时：

1. 确认 `[workspace.package].version = "0.1.0"`。
2. 运行 `cargo check --workspace`，让 Cargo 刷新 `Cargo.lock`（不要盲目全局替换版本字符串）。
3. 用 metadata 核对每个 Impressy workspace 包都解析为 `0.1.0`：

```bash
cargo metadata --no-deps --format-version 1
```

若版本已是 `0.1.0` 且无其它元数据漂移，则不必为「只改版本号」单独再开 commit；
验收证据与发布说明相关提交仍按第 5 节处理。

WiX / DEB / bundle 均从 Cargo 包版本读取；不要在安装脚本里另写一套漂移版本号。

## 4. Validate before publication

### 4.1 自动质量门禁

```bash
cargo check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo bench -p impressy-core --bench v1_local
```

Core bench 只用于发现相对性能回退，不能代替桌面冷启动与 UI 响应测量。

### 4.2 桌面验收证据（发布硬门禁）

按 [`testing/v1-desktop-acceptance.md`](./testing/v1-desktop-acceptance.md) 在四类桌面环境完成真机验收：

1. 生成确定性资源并记录摘要：

```bash
cargo run --release -p impressy-core --example acceptance_assets -- target/acceptance-assets
```

参考 `generated_sha256`（在相同 `image` / `qrcode` 版本下）应为
`3a436a64acf820756e65739709978cc8f7e74a0cc3be7924bcde8e9478e9050f`。
`exif_photo_sha256` 来自验收人提供的真实 EXIF 照片。

2. 将结果写入 `docs/testing/results/v1-desktop-acceptance.json`
   （结构见 `v1-desktop-acceptance.example.json`）。
3. `source_commit` 必须是完整 40 字符 SHA，且是当前 HEAD 的祖先；其后不得再有
   release-affecting 路径变更（`Cargo.toml` / `Cargo.lock` / `crates` / `packaging` /
   `vendor` / `.github`）。
4. 四个平台 `result` 均为 `pass`，common + platform checks 齐全，每项性能 ≥ 3 个样本且达标，
   `package_checks` 三平台均为 `true`。
5. 提交该 JSON 后运行：

```bash
python scripts/verify_desktop_acceptance.py
```

缺文件、过期、指标不合格或安装包未验收都会让 `.github/workflows/release.yml` 的
`acceptance-gate` job 失败，从而阻止正式 Release。

**禁止臆造或补填未实测的平台结果。** 平台证据来自不同 SHA 时必须先对齐候选再汇总
（见 issue #5）。

### 4.3 发布前 diff 检查

```bash
git diff --check
git status --short --branch
git diff -- Cargo.toml Cargo.lock docs/testing/results/v1-desktop-acceptance.json
```

测试失败、验收验证器失败、diff 含无关改动、或目标 tag/Release 已存在时，停止，不要打 tag。

## 5. Commit, tag, and push

验收 JSON（及必要的版本同步）落在独立提交后，创建 annotated tag 并推送：

```bash
git add -- docs/testing/results/v1-desktop-acceptance.json Cargo.toml Cargo.lock
git commit -m "Release Impressy v0.1.0"
git tag -a v0.1.0 -m "Release Impressy v0.1.0"
git push origin main v0.1.0
```

推送 `v*` tag 会触发 `.github/workflows/release.yml`。只有 `refs/tags/v*` 才会跑
`publish-release` job 并创建 GitHub Release。

## 6. Monitor the tag workflow

找到 branch 为 `v0.1.0` 的 run，并等待结束：

```bash
gh run list --workflow release.yml --limit 10 --json databaseId,headBranch,headSha,event,status,conclusion,createdAt,displayTitle,url
gh run watch <tag-run-id> --exit-status --interval 15
```

下列 job 必须全部成功：

- Verified desktop acceptance evidence
- Signed Windows x64 MSI
- Linux x64 Debian package
- Signed and notarized macOS arm64 DMG
- Publish tagged release

失败时用 `gh run view <tag-run-id> --log-failed` 排查。不要把失败 run 报告为已发布完成。
若公共 tag 已存在，保留它，前向修复或请示维护者，不要擅自 force-move。

### 所需 GitHub Actions secrets

| Secret | 内容 |
| --- | --- |
| `WINDOWS_CERTIFICATE_BASE64` | 代码签名 PFX 的 Base64 |
| `WINDOWS_CERTIFICATE_PASSWORD` | PFX 密码 |
| `MACOS_CERTIFICATE_BASE64` | Developer ID Application `.p12` 的 Base64 |
| `MACOS_CERTIFICATE_PASSWORD` | `.p12` 密码 |
| `MACOS_SIGNING_IDENTITY` | `Developer ID Application: ... (TEAMID)` |
| `APPLE_ID` | 公证用 Apple ID |
| `APPLE_TEAM_ID` | Apple Developer Team ID |
| `APPLE_APP_PASSWORD` | Apple ID app-specific password |

PowerShell 可用 `[Convert]::ToBase64String([IO.File]::ReadAllBytes("certificate.pfx"))`
生成证书 Base64。不要把证书、密码或解码后的材料提交进仓库。

## 7. Curate the release notes

工作流会用 `--generate-notes` 创建 Release。把自动 notes 当种子，再整理为双语终稿：

```bash
gh release edit v0.1.0 --title "Impressy v0.1.0" --notes-file <release-notes-file>
```

推荐结构：

```markdown
# Impressy v0.1.0

One-sentence summary of the release.

## Highlights

### Feature or improvement area

- User-visible change and its practical effect.

### Fixes

- Important reliability or behavior fix.

## Compatibility

- State whether config or output layouts require migration.
- State platform limitations (signed MSI / notarized DMG / Debian amd64 DEB scope).

## Downloads

- Windows x64: signed MSI.
- macOS Apple Silicon: signed and notarized DMG.
- Linux x86_64: Debian/Ubuntu amd64 DEB (not a universal Linux package).

## Verification

- Local workspace quality gate result.
- Committed `v1-desktop-acceptance.json` verifier result.
- Tag workflow native packaging result.

**Full comparison**: https://github.com/willmove/impressy/compare/<previous-tag>...v0.1.0

---

# Impressy v0.1.0（中文说明）

一句话版本总结。

## 主要更新

### 功能或改进领域

- 用户可见的变化及其实际效果。

### 修复

- 重要的可靠性或行为修复。

## 兼容性

- 说明配置或导出布局是否需要迁移。
- 说明平台限制（已签名 MSI / 已公证 DMG / 仅 Debian/Ubuntu amd64 DEB）。

## 下载

- Windows x64：已签名 MSI。
- macOS Apple Silicon：已签名并公证的 DMG。
- Linux x86_64：Debian/Ubuntu amd64 DEB（不是全发行版通用包）。

## 验证

- 本地 workspace 质量门禁结果。
- 已提交的桌面验收 JSON 与验证器结果。
- tag 工作流原生打包结果。

**完整变更对比**: https://github.com/willmove/impressy/compare/<previous-tag>...v0.1.0
```

规则：

- 声明必须来自真实 tag-to-tag diff、提交与已完成 OpenSpec change。
- 优先写用户结果，技术细节只在解释兼容性、安全、性能或保真度时出现。
- 未验证过的测试、平台构建、安装包或功能不得写入 Verification。

## 8. Final verification

```bash
gh release view v0.1.0 --json name,tagName,isDraft,isPrerelease,publishedAt,url,body,assets
git status --short --branch
git log -1 --oneline --decorate
git ls-remote --heads --tags origin main v0.1.0
```

确认：

- 标题为 `Impressy v0.1.0`，tag 为 `v0.1.0`。
- Release 已发布（非 draft），且不是误开的 prerelease。
- 双语整理 notes 与完整对比链接存在。
- Assets 包含 Windows MSI、Linux DEB、macOS DMG。
- tag 工作流三平台与 publish job 均成功。
- 本地 `main`、`origin/main`、发布提交与 annotated tag 指向预期状态。
- 工作树干净。

全部通过后，才可把该版本报告为发布完成，并附上 Release 与 workflow 链接。

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

## Windows MSI（本地）

WiX 定义位于 `crates/impressy-app/wix/main.wxs`。本地生成需要 Windows、WiX Toolset v3 和
`cargo-wix 0.3.9`：

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
