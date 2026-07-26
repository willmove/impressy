# macOS / Linux 发布证据改走 GitHub Actions

## 状态

已接受（2026-07-26）。

## 背景

[`v1-desktop-acceptance.md`](../testing/v1-desktop-acceptance.md) 原先要求 Windows、
macOS、Linux X11、Linux Wayland 四类**桌面真机**都完成交互与性能矩阵，再写入正式
验收 JSON。主开发机是 Windows 真机；macOS 与 Linux 桌面并不总是可用。

与此同时，仓库已有：

- `.github/workflows/ci.yml`：`ubuntu` / `windows` / `macos` 上的
  `cargo check` + clippy + test，以及 Windows MSI、Linux DEB 安装/卸载冒烟；
- `.github/workflows/release.yml`：tag 上构建签名 MSI、DEB、签名并公证的 DMG。

这与 Markion 的发布模型一致：跨平台打包与冒烟由 GitHub 托管 runner 承担，不阻塞在
维护者自备第二、第三台桌面真机上。继续把「有 macOS/Linux 真机」写成发布前置，会让
v0.1.0 无法按 CI 路径收敛。

## 决策

发布与正式验收 JSON 采用**双轨证据**：

1. **Windows（桌面真机）**  
   继续按 `v1-desktop-acceptance.md` 做完整交互、性能与安装包验收；`platforms.windows`
   使用 `evidence: "desktop"`，必须包含全部 common checks、Windows 平台 check，以及
   `performance.windows` 的达标样本。

2. **macOS / Linux（GitHub Actions）**  
   `platforms.macos`、`platforms.linux-x11`、`platforms.linux-wayland` 使用
   `evidence: "github-actions"`。证据来自针对同一 `source_commit`（或以其为祖先且无
   release-affecting 漂移的 HEAD）的成功 CI / release runner 结果，而不是维护者本机
   macOS / Linux 桌面。

   - macOS：至少记录成功的 `CI` quality job（`macos-latest`）。签名与公证由
     `release.yml` 的 `macos-dmg` job 在打 `v*` tag 时强制执行；预 tag 的
     `package_checks.macos` 表示「该提交已在 Actions macOS runner 上通过质量门禁」，
     正式签名产物仍以 tag 工作流为准。
   - Linux：至少记录成功的 `CI` quality job 与 `linux-package` DEB 安装/卸载冒烟。
     X11 与 Wayland 交互矩阵不再作为打 tag 的硬前置；两者在 JSON 中可共享同一 CI
     证据，checks 使用 CI 专用集合（见验证器）。

3. **性能样本**  
   发布门禁只强制 `performance.windows`。macOS / Linux 的性能数组可为空；若填写则仍须
   满足既有预算（便于日后自愿补充真机数据）。

4. **验证器**  
   `scripts/verify_desktop_acceptance.py` 按 `evidence` 字段分支校验；缺失或非法
   evidence 一律失败。ADR 胜过此前「四平台桌面真机缺一不可」的表述。

## 后果

- 打 `v0.1.0` 不再等待自备 macOS / Linux 桌面；跨平台编译与安装包冒烟走 GitHub Actions。
- Windows 仍是交互与 NFR 性能的唯一硬真机门禁。
- CI 不能证明 Metal/X11/Wayland 下的渲染手感；这些仍可作为可选后续真机补充，但不阻塞
  已签名/已公证安装包的发布路径。
- Issue #3 / #4 的「桌面真机全矩阵」降级为可选增强，或改为「确认 Actions 证据已写入
  正式 JSON」类文档任务。
