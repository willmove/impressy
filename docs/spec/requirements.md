# Requirements Document - Impressy 图像工具箱

> **本文档与 [`design.md`](./design.md) 是本项目的唯一真相源。** 领域词汇见 [`CONTEXT.md`](../../CONTEXT.md)，范围与顺序决策见 [`docs/adr/`](../adr/)。
> 早期的 `impressy-spec.md` 已冻结至 [`archive/impressy-spec-v0.3.md`](./archive/impressy-spec-v0.3.md)，仅作历史参考，**不要据此写代码**。

## Introduction

### 为什么存在

现有图片处理软件（如美图秀秀）**启动慢、功能臃肿**，常用功能缺失、不需要的功能冗余。Impressy 的目标是用一款轻量、快启动的原生工具，覆盖个人用户的常用本地图片处理需求。

这段问题陈述是 [ADR-0001](../adr/0001-v1-scope-local-only.md) 的基石——它解释了为什么本地功能是产品的本体，而 AI 功能是后加的一层。

### 是什么

Impressy 是一款跨平台（Windows、macOS、Linux）的原生桌面图像处理工具，基于 Rust + GPUI 框架构建，围绕四大**板块**组织：基础图片处理、AI 生成与改图、行业定制 AI 工具、创作输出。系统采用本地优先架构，本地功能完全离线可用，AI 功能通过用户自备 API Key（BYOK）实现，确保隐私安全与零成本运营。屏幕截图、全局热键与屏幕取色已按 [ADR-0003](../adr/0003-remove-screen-capture.md) 从当前范围剥离。

### v1 / v2 范围（重要）

**v1 只交付本地功能**——完全离线、不需要 API Key 的功能集合；**v2 在不削弱该承诺的前提下叠加 AI 功能**。每条需求的标题都标了 `[v1]` 或 `[v2]`：

- **`[v1]` 36 条**：本地图片处理功能及其支撑需求（架构、导航、i18n、格式、质量、剪贴板、性能、property test，以及当前文档与编辑历史）。
- **`[v2]` 21 条**：已进入实现基线的 AI 图片功能——Requirement 4、5、16、17、18、19–31、33、50、59。Provider 契约、四 crate 边界与安全约束见 [ADR-0004](../adr/0004-v2-provider-contract.md)。
- **`[removed]` 4 条**：Requirement 13、14、48、49，以及其他需求中的截图专用 AC。它们不属于当前交付范围，见 ADR-0003。

注意「本地功能 / AI 功能」与「板块」是**正交的两根轴**：v1 的历史基线只填充本地功能；当前 v2 基线已填充两个 AI 板块并在「创作输出」提供海报设计。三个视频入口继续作为「开发中」占位。详见 [CONTEXT.md](../../CONTEXT.md)、[ADR-0001](../adr/0001-v1-scope-local-only.md) 与 [ADR-0004](../adr/0004-v2-provider-contract.md)。

**建设顺序**：先 `impressy-core` 到全绿，GPUI 骨架推后（与已冻结 spec 的 M1→M7 顺序相反）。原因见 [ADR-0002](../adr/0002-core-before-ui.md)。

### v1 当前验收状态（2026-07-19）

本节区分四种经常被混用的状态。后续进度说明必须使用这些词，不得把「代码已写」表述为「需求已验收」：

- **已实现**：对应代码路径已经存在并接入应用。
- **自动验证通过**：对应行为可由 headless 测试、编译器或 CI 证明，并已通过质量门禁。
- **待真机验收**：代码已实现且能类型检查，但渲染、交互手感、系统集成或性能只能在桌面真机上判断。
- **可发布**：自动验证与 [`v1-desktop-acceptance.md`](../testing/v1-desktop-acceptance.md) 通过，正式结果文件 `docs/testing/results/v1-desktop-acceptance.json` 存在且通过验证器（Windows 桌面真机 + macOS/Linux GitHub Actions 证据，见 [ADR-0005](../adr/0005-ci-macos-linux-release-evidence.md)），安装包签名 / 公证及平台专项均通过。

截至 2026-07-19，当前 Windows 11 真机验收的 release 源码候选为 `fffdb2c`，已推送到
`origin/feature/ongoing`。其后的提交只更新验收文档，没有改变 release 源码。`377d62c`
已完成全部原生路径 prompt 的异步化，`fffdb2c` 又把取消、平台错误和 channel 关闭统一
收敛到生产 `resolve_path_prompt` seam，让取消 / 失败后的非忙碌状态与立即复用能够直接
回归。本地质量门禁执行 60 个测试（`impressy-core` 28 个、`impressy-app` 32 个）并通过。
远端 `origin/main@f88a939` 的 Windows、macOS、Linux 质量任务以及 Windows MSI、Linux
DEB 冒烟均通过；`fffdb2c` 尚未触发精确候选的跨平台 CI：

| 范围 | 当前状态 | 证据 / 剩余门禁 |
| --- | --- | --- |
| `impressy-core` 本地引擎 | 自动验证通过 | 本地质量门禁全绿；Requirement 52–57 的 v1 property 覆盖已落地 |
| `impressy-app` 八个 v1 功能页 | 已实现；待真机验收 | Windows 11 build 26200 已完成主流程和扩展矩阵：文件拖放、跨应用 PNG 剪贴板、输出目录、配置持久化 / 损坏恢复、格式不支持 / 图片损坏 / 权限不足分类均通过；编辑与拼图均直出 PNG/JPEG/WebP，拼图还覆盖网格间距和透明背景；批量首项写出失败会显示名称且不中断后项；切图覆盖 3×3 与自定义 2×4 的逐像素重组；二维码覆盖高纠错、尺寸与自定义双色并可回读；美化透明背景的外角 alpha 为 0；GIF 覆盖 100/800ms 边界、三种播放顺序和 Windows Photos 实播。磁盘空间不足、常用消息应用播放、Windows 10 和其他桌面仍待验收 |
| 三平台编译与安装包结构 | 已实现 | `origin/main@f88a939` 的 Windows、macOS、Linux 质量任务及 Windows MSI、Linux DEB 冒烟均通过；`fffdb2c` 的本地未签名 MSI 为 5.10MiB，administrative extraction 成功且 payload EXE 摘要与 release 构建一致；精确候选跨平台 CI、管理员权限下的安装 / 关联 / 升级 / 卸载、签名与公证仍待完成 |
| 性能指标 | Windows 11 真机验证通过；其他平台待验收 | release 构建三次样本均达标：冷启动最大 539.36ms、50MP 预览最大 1047.66ms、美化预览最大 198.56ms、100 张批处理点击响应最大 33.59ms、进度频率最小 1.97Hz；Windows 10、macOS 与 Linux 仍须按同一清单测量 |
| v1 发布状态 | **不可标记为可发布** | 正式桌面验收结果文件尚未提交；缺失或过期会被发布工作流阻止 |

这张表是状态快照，不替代下方 Acceptance Criteria。任何代码变更若影响 `Cargo.toml`、`Cargo.lock`、`crates/`、`packaging/`、`vendor/` 或 `.github/`，都必须让既有真机验收证据失效并重新验收。

### v2 当前验收状态（2026-07-19）

v2 图片功能代码已接入四 crate workspace：三家 Provider、操作系统凭据存储、动态能力声明、12 个 AI 改图场景、13 个行业工具、海报文字图层、本地精确尺寸后处理和本地生成历史均已实现。假传输 Provider 契约测试、提示词内嵌测试、安全配置测试与全部 workspace 质量门禁通过。

该状态仍是「已实现；自动验证通过；待真实服务商与人眼验收」，不是「可发布」：真实 Seedream / Nano Banana / OpenAI 请求需要验收人提供各自 API Key 和额度；所有**保持不变项**、内容过滤、服务商账户错误与生成质量必须按 [`v2-ai-acceptance.md`](../testing/v2-ai-acceptance.md) 在真实服务上验收。v2 改动也已使此前 v1 候选 SHA 的桌面证据过期，四平台桌面与安装包门禁须以新的精确候选重新执行。

### 当前 v2 UX 稳定化门禁（2026-09-06）

[`2026-09-06 UX 审查`](../reviews/2026-09-06-ux/report.md) 发现预览与实际处理输入可能不一致、批量/切图可能静默覆盖旧文件、图像编辑缺少撤销，以及导出与批量流程不完整。ADR-0006 已冻结统一的**当前文档**、**输入集合**、**临时预览**、**编辑历史**与**导出**语义。

Requirement 5、6、8、15、34、36、37、39、41、42、47、51、58、59、60 与新增 Requirement 61 的相关 AC 是当前 v2 发布门禁。它们仍按“本地功能 / AI 功能”标记归属，不把本地功能误标成 AI 功能。导航的最近使用/固定工具、任务同义词搜索以及纯视觉精修属于本次稳定化之后的增强，不阻塞当前门禁。

截至 2026-09-06，这些 UX 稳定化 AC 已完成编码实现，并通过工作区 132 项自动测试（含编辑历史与批量相对尺寸 property test、步骤顺序、跨编辑工具预览决策、文档实例身份、历史预算与过期 AI 任务收敛验证）、`cargo check` 与零警告 clippy。Windows release 构建与启动冒烟通过；中英文桌面任务脚本、同一候选提交的跨平台 CI/打包证据和真实 Provider 验收仍待完成，因此尚不能宣称本门禁全部通过。

## Glossary

> 这是**组件清单**（crate 与技术构件的命名），不是领域词汇表。领域词汇（板块、档位、保持不变项、本地功能 / AI 功能）见 [`CONTEXT.md`](../../CONTEXT.md)。

- **Impressy_System**: Impressy 图像工具箱应用程序整体
- **UI_Layer**: 基于 GPUI 的用户界面层，负责所有可视化交互
- **Core_Engine**: 本地图像处理引擎（impressy-core crate），执行裁剪、压缩、拼接等离线操作
- **AI_Engine**: AI 服务提供商抽象层（impressy-ai crate），统一管理多个 AI 服务商接口
- **Preset_Library**: 行业工具锁定提示词模板库（impressy-presets crate）
- **Provider**: AI 服务提供商，包括 Seedream（默认）、Google Nano Banana、OpenAI GPT-Image。〔v2〕Agnes 暂不实现，仅预留接口扩展能力。
- **API_Key**: 用户自备的 AI 服务商访问密钥
- **Credential_Store**: 操作系统凭据管理器（Windows Credential Manager / macOS Keychain）
- **Background_Executor**: GPUI 异步执行器，用于处理耗时任务而不阻塞 UI
- **BYOK**: Bring Your Own Key，用户自备 API Key 模式
- **EXIF**: Exchangeable Image File Format，图像文件元数据标准
- **Canvas_Element**: GPUI 自定义 UI 元素，用于交互式画布功能
- **Text_Layer**: 海报设计中的本地渲染文字图层
- **Batch_Queue**: 批量处理任务队列
- **Current_Document**: 单图工作流中的**当前文档**，包含原图、最新已应用结果、未导出状态与会话编辑历史；领域定义见 [`CONTEXT.md`](../../CONTEXT.md)
- **Input_Set**: 多图工作流中的**输入集合**，与 Current_Document 的单图临时预览相互独立
- **Transient_Preview**: 参数调整期间的**临时预览**，尚未成为 Current_Document 的已应用结果
- **Tier**: 行业工具的**档位**——用户唯一需要做的选择，一组预置的、语义化的参数选项（如证件照的「一寸/二寸」、头像工坊的「日漫风/赛博朋克」）。档位背后的提示词对用户不可见。定义见 [`CONTEXT.md`](../../CONTEXT.md)。
  _注：本条此前误作 `Archive_Mode` —— 「档位」的「档」被当成了「档案 archive」，实为 gear/tier 之意。请勿在代码中使用 `ArchiveMode` 一名。_


## Requirements

### [v1] Requirement 1: 平台支持与运行环境

**User Story:** 作为用户，我希望在 Windows、macOS、Linux 平台上运行 Impressy，以便在不同操作系统环境下使用图像处理功能。

#### Acceptance Criteria

1. THE Impressy_System SHALL run on Windows 10 x64 and Windows 11 x64
2. THE Impressy_System SHALL run on macOS using Metal rendering backend
3. THE Impressy_System SHALL run on Linux using appropriate GPUI backend
4. THE Impressy_System SHALL use DirectX 11 for rendering on Windows platform
5. THE Impressy_System SHALL use DirectWrite for text shaping on Windows platform
6. THE Core_Engine SHALL execute all v1 local image processing functions without network connectivity
7. WHEN the system starts from a cold state, THE Impressy_System SHALL display the main interface within 1500 milliseconds
8. EACH platform release artifact of THE Impressy_System SHALL NOT exceed 30 MiB

### [v1] Requirement 2: 隐私与安全

**User Story:** 作为隐私关注的用户,我希望我的 API Key 和图像数据完全本地化存储和处理,以确保数据不被泄露。

#### Acceptance Criteria

1. 〔v2〕WHEN a user configures an API_Key for a Provider, THE Impressy_System SHALL store the API_Key in the Credential_Store
2. 〔v2〕THE Impressy_System SHALL NOT include any API_Key in the installation package
3. 〔v2〕THE Impressy_System SHALL NOT upload any API_Key to any remote server
4. 〔v2〕IF a user initiates an AI request, THEN THE AI_Engine SHALL send image data to the configured Provider endpoint
5. THE Impressy_System SHALL NOT send user image data to any server without explicit user-initiated AI requests
6. THE Impressy_System SHALL NOT send telemetry data to any remote server
7. WHEN EXIF_Cleaner exports an image file, THE Core_Engine SHALL remove all GPS metadata fields
8. WHEN EXIF_Cleaner exports an image file, THE Core_Engine SHALL remove all device identification metadata fields
9. 〔v2〕THE Impressy_System SHALL use Windows Credential Manager for API_Key storage on Windows platform
10. 〔v2〕THE Impressy_System SHALL use macOS Keychain for API_Key storage on macOS platform


### [v1] Requirement 3: 架构与并发处理

**User Story:** 作为用户,我希望在批量处理大量图片时界面仍保持流畅响应,以便同时进行其他操作。

#### Acceptance Criteria

1. THE Impressy_System SHALL organize code into a Cargo workspace. 〔v1 只建两个 crate：**impressy-app、impressy-core**。`impressy-capture` 按 ADR-0003 剥离；`impressy-ai` 与 `impressy-presets` 属 AI 功能，v1 不建 —— 见 ADR-0001。〕
2. 〔v2〕THE AI_Engine SHALL implement a unified Provider trait for all AI service providers
3. 〔v2〕WHEN a new Provider is added, THE AI_Engine SHALL integrate it without modifying upper-layer business logic
4. THE Core_Engine SHALL execute all image encoding operations in Background_Executor
5. THE Core_Engine SHALL execute all image decoding operations in Background_Executor
6. 〔v2〕THE AI_Engine SHALL execute all network requests in Background_Executor
7. WHEN processing 100 images in batch mode, THE UI_Layer SHALL remain responsive to user interactions
8. THE UI_Layer SHALL update only rendering and state on the main thread
9. THE Batch_Queue SHALL execute batch processing tasks asynchronously
10. WHEN a native file or directory picker is opened, THE UI_Layer SHALL release mutable entity borrows before the platform dialog starts its event loop; on Windows it SHALL NOT use a synchronous nested dialog from inside a GPUI entity listener

### [v2] Requirement 4: AI 服务商管理

**User Story:** 作为用户,我希望配置和切换不同的 AI 服务商,以便根据需求和成本选择合适的服务。

#### Acceptance Criteria

1. THE Impressy_System SHALL support Seedream as a Provider
2. THE Impressy_System SHALL support Google Nano Banana as a Provider
3. THE Impressy_System SHALL support OpenAI GPT-Image as a Provider
4. 〔v2・暂缓〕Agnes **暂不实现**（spec v0.3 已确认），仅要求 Provider trait 预留扩展能力，不得为其编写适配器。
5. WHEN a user opens settings, THE UI_Layer SHALL display API_Key configuration fields for all supported Providers
6. WHEN a user selects a default Provider, THE Impressy_System SHALL save the selection in local configuration
7. WHEN a user switches Provider, THE UI_Layer SHALL adjust available generation parameters according to the Provider capabilities
8. THE AI_Engine SHALL declare capability constraints for each Provider including maximum reference image count
9. THE AI_Engine SHALL declare capability constraints for each Provider including supported aspect ratios
10. THE AI_Engine SHALL declare capability constraints for each Provider including maximum generation count per request


### [v2] Requirement 5: AI 生成体验

**User Story:** 作为用户,我希望在 AI 生成过程中看到进度反馈和清晰的错误信息,以便了解生成状态和问题原因。

#### Acceptance Criteria

1. WHEN an AI generation request is in progress, THE UI_Layer SHALL display a progress indicator
2. WHEN an AI generation request is in progress, THE UI_Layer SHALL display a waiting state message
3. IF an AI generation request fails due to invalid API_Key, THEN THE UI_Layer SHALL display an invalid key error message
4. IF an AI generation request fails due to insufficient account balance, THEN THE UI_Layer SHALL display an insufficient balance error message
5. IF an AI generation request fails due to network connectivity issues, THEN THE UI_Layer SHALL display a network failure error message
6. IF an AI generation request fails due to content policy violation, THEN THE UI_Layer SHALL display a content blocked error message
7. WHEN an AI generation completes with multiple results, THE UI_Layer SHALL display all generated images
8. WHEN multiple generated images are displayed, THE UI_Layer SHALL allow the user to select any subset for download
9. WHEN an AI feature uses reference images, THE UI_Layer SHALL display the exact image version that will be transmitted
10. WHEN region editing is selected, THE displayed reference image, selection coordinates, and transmitted image SHALL belong to the same Current_Document version
11. BEFORE the user starts an AI request with a reference image, THE UI_Layer SHALL display an inline disclosure naming the selected Provider and stating that the displayed image will be transmitted to it
12. WHEN the user clicks the generate action while the inline disclosure is visible, THE action SHALL constitute explicit initiation of that request without an additional recurring confirmation dialog
13. WHEN the user switches Provider, THE inline disclosure and available parameters SHALL update before generation can start
14. WHEN a user returns to an AI feature during the same application session, THE UI_Layer SHALL preserve that feature's unsubmitted selections unless they are invalid under current Provider capabilities
15. THE UI_Layer SHALL keep the primary generate action visible while its parameter panel scrolls independently
16. THE UI_Layer SHALL place Provider, quality, and count controls in an expandable advanced section while displaying a summary of their current values
17. WHEN AI results are available, THE UI_Layer SHALL allow comparison with the submitted reference image and SHALL allow a selected result to become a Current_Document for subsequent local processing

### [v1] Requirement 6: 图片编辑

**User Story:** 作为用户,我希望打开图片并进行裁剪、格式转换和质量调整,以便获得符合需求的输出文件。

#### Acceptance Criteria

1. WHEN a user opens an image file, THE UI_Layer SHALL display the image in the editor
2. THE UI_Layer SHALL provide aspect ratio presets including 1:1, 4:5, 9:16, 16:9, and ultra-wide banner
3. WHEN a user selects an aspect ratio preset, THE UI_Layer SHALL display a crop frame with the selected ratio
4. THE UI_Layer SHALL allow the user to drag and adjust the crop frame position
5. THE UI_Layer SHALL allow the user to drag and adjust the crop frame size while maintaining aspect ratio
6. THE Core_Engine SHALL support PNG output format with adjustable compression level
7. THE Core_Engine SHALL support JPEG output format with adjustable quality
8. THE Core_Engine SHALL support WebP output format with adjustable quality
9. WHEN a user exports a cropped image, THE Core_Engine SHALL apply the crop region to produce the output file
10. WHEN a user exports an image, THE Core_Engine SHALL apply the selected format and quality settings
11. WHEN a Current_Document is opened, THE resize width and height SHALL initially equal the Current_Document dimensions
12. WHEN the user changes one resize dimension while aspect-ratio lock is enabled, THE UI_Layer SHALL update the other dimension proportionally
13. THE UI_Layer SHALL enable aspect-ratio lock by default and SHALL require an explicit user action before allowing non-proportional stretching
14. THE UI_Layer SHALL distinguish crop parameters, resize parameters, and rotation actions as separate editing tools
15. THE UI_Layer SHALL provide resize modes for percentage and longest side
16. THE UI_Layer SHALL provide an option that prevents enlargement when the source already fits within the requested dimensions
17. WHEN a crop ratio is selected, the selected ratio indicator, crop frame, displayed crop dimensions, and exported crop SHALL remain synchronized


### [v1] Requirement 7: 拼图拼接

**User Story:** 作为用户,我希望将多张图片拼接成一张长图或网格图,以便创建图片集合展示。

#### Acceptance Criteria

1. WHEN a user imports multiple images, THE UI_Layer SHALL accept the images for collage composition
2. THE UI_Layer SHALL provide vertical layout mode for collage
3. THE UI_Layer SHALL provide horizontal layout mode for collage
4. THE UI_Layer SHALL provide grid layout mode for collage
5. WHERE grid layout mode is selected, THE UI_Layer SHALL allow the user to adjust column count
6. WHERE grid layout mode is selected, THE UI_Layer SHALL allow the user to adjust spacing between images
7. WHERE grid layout mode is selected, THE UI_Layer SHALL allow the user to adjust background color
8. WHEN a user exports a collage, THE Core_Engine SHALL compose all images into a single output file according to the selected layout
9. THE Core_Engine SHALL support PNG format export for collages
10. THE Core_Engine SHALL support JPEG format export for collages
11. THE Core_Engine SHALL support WebP format export for collages

### [v1] Requirement 8: 批量处理

**User Story:** 作为用户,我希望一次性处理几十到上百张图片,包括格式转换、压缩、调整尺寸和加水印,以提高工作效率。

#### Acceptance Criteria

1. THE UI_Layer SHALL organize the batch processing page in the order Input_Set → ordered processing steps → output settings → results
2. WHEN a user drags 50 or more images into the batch processor, THE UI_Layer SHALL accept all images for processing
3. WHERE watermark mode is selected, THE UI_Layer SHALL support text watermark
4. WHERE watermark mode is selected, THE UI_Layer SHALL support image watermark
5. WHERE watermark mode is selected, THE UI_Layer SHALL provide position options including bottom-right corner and tiled pattern
6. WHEN batch processing executes, THE UI_Layer SHALL display progress feedback for the entire batch
7. IF an individual item in the batch fails, THEN THE Batch_Queue SHALL mark the failed item separately
8. IF an individual item in the batch fails, THEN THE Batch_Queue SHALL continue processing remaining items without interruption
9. WHEN a batch export completes, THE Core_Engine SHALL produce output files with the selected watermark applied at the chosen position
10. WHILE batch processing executes, THE UI_Layer SHALL remain responsive to user interactions
11. THE UI_Layer SHALL allow resize and watermark steps to be added, removed, and reordered in one batch pipeline
12. THE UI_Layer SHALL treat output format and compression or quality as output settings rather than mutually exclusive processing modes
13. FOR EACH batch item, THE Batch_Queue SHALL apply every enabled processing step in the displayed order before encoding the selected output format
14. THE UI_Layer SHALL preview the selected input after the complete displayed pipeline and SHALL NOT reuse an unrelated Current_Document preview
15. THE UI_Layer SHALL distinguish appending files to an Input_Set from replacing the entire Input_Set
16. THE UI_Layer SHALL allow cancellation between batch items without corrupting files already completed
17. WHEN batch processing finishes or is cancelled, THE UI_Layer SHALL display per-item input name, output dimensions, output size, and success or failure reason
18. WHEN one or more items fail, THE UI_Layer SHALL allow retrying only failed items without rewriting successful outputs
19. THE UI_Layer SHALL allow saving the ordered steps and their parameters as a reusable batch preset
20. A batch preset SHALL NOT store input files or an output directory


### [v1] Requirement 9: 切图

**User Story:** 作为用户,我希望将长图或整图按网格切分成多个小图,以便用于社交媒体九宫格发布。

#### Acceptance Criteria

1. WHEN a user imports an image for slicing, THE UI_Layer SHALL accept the image
2. THE UI_Layer SHALL provide a 3x3 grid preset for slicing
3. THE UI_Layer SHALL allow the user to specify custom row count for slicing
4. THE UI_Layer SHALL allow the user to specify custom column count for slicing
5. WHEN a user exports sliced images, THE Core_Engine SHALL divide the source image according to the grid configuration
6. WHEN a user exports sliced images, THE Core_Engine SHALL produce sequentially numbered output files for each grid cell

### [v1] Requirement 10: 二维码生成与识别

**User Story:** 作为用户,我希望生成自定义样式的二维码并识别图片中的二维码内容,以便进行信息编码和解码。

#### Acceptance Criteria

1. WHEN a user inputs text for QR code generation, THE Core_Engine SHALL generate a QR code image encoding the input text
2. WHEN a user inputs a URL for QR code generation, THE Core_Engine SHALL generate a QR code image encoding the URL
3. THE UI_Layer SHALL allow the user to adjust QR code output size
4. THE UI_Layer SHALL allow the user to adjust QR code error correction level
5. THE UI_Layer SHALL allow the user to adjust QR code foreground color
6. THE UI_Layer SHALL allow the user to adjust QR code background color
7. WHEN a user drags an image containing a QR code, THE Core_Engine SHALL decode the QR code content
8. WHEN QR code decoding succeeds, THE UI_Layer SHALL display the decoded content
9. FOR ALL QR codes generated by the Core_Engine, scanning the code with mainstream QR readers SHALL decode the original input correctly
10. WHEN a screenshot-quality QR code image is provided, THE Core_Engine SHALL successfully decode it


### [v1] Requirement 11: EXIF 信息查看与清除

**User Story:** 作为用户,我希望查看照片的拍摄参数并一键清除隐私元数据,以保护位置和设备信息。

#### Acceptance Criteria

1. WHEN a user opens an image with EXIF data, THE UI_Layer SHALL display camera model information
2. WHEN a user opens an image with EXIF data, THE UI_Layer SHALL display focal length information
3. WHEN a user opens an image with EXIF data, THE UI_Layer SHALL display aperture information
4. WHEN a user opens an image with EXIF data, THE UI_Layer SHALL display shutter speed information
5. WHEN a user opens an image with EXIF data, THE UI_Layer SHALL display ISO information
6. WHEN a user opens an image with EXIF data, THE UI_Layer SHALL display GPS coordinates if present
7. WHEN a user triggers EXIF cleaning, THE Core_Engine SHALL export an image file without any GPS metadata fields
8. WHEN a user triggers EXIF cleaning, THE Core_Engine SHALL export an image file without any device identification metadata fields
9. FOR ALL images exported by EXIF_Cleaner, inspection with third-party EXIF tools SHALL reveal no GPS fields
10. FOR ALL images exported by EXIF_Cleaner, inspection with third-party EXIF tools SHALL reveal no device identification fields

### [v1] Requirement 12: 截图美化

**User Story:** 作为用户,我希望为现有截图添加圆角、边距、背景和阴影效果,以美化演示图片。

#### Acceptance Criteria

1. WHEN a user drags a screenshot into the beautifier, THE UI_Layer SHALL accept the image
2. THE UI_Layer SHALL allow the user to adjust corner radius for rounded corners
3. THE UI_Layer SHALL allow the user to adjust inner padding
4. THE UI_Layer SHALL allow the user to select gradient background
5. THE UI_Layer SHALL allow the user to select solid color background
6. THE UI_Layer SHALL allow the user to select transparent background
7. THE UI_Layer SHALL allow the user to adjust border stroke
8. THE UI_Layer SHALL allow the user to adjust shadow effect
9. WHILE a user adjusts beautification parameters, THE UI_Layer SHALL update the preview in real-time
10. WHEN a user exports with transparent background selected, THE Core_Engine SHALL produce a PNG file with true transparency outside rounded corners

> 本需求只处理用户导入的现有图片，不包含屏幕捕获，也不依赖 `impressy-capture`。


### [removed] Requirement 13: 屏幕截图

已从当前产品范围剥离，不实现、不占位。见 [ADR-0003](../adr/0003-remove-screen-capture.md)。

### [removed] Requirement 14: 屏幕取色

已随屏幕截图能力一并剥离，不实现、不占位。见 [ADR-0003](../adr/0003-remove-screen-capture.md)。


### [v1] Requirement 15: GIF 制作

**User Story:** 作为内容创作者,我希望将连续图片合成为动图并控制播放效果,以创作动态内容。

#### Acceptance Criteria

1. WHEN a user imports multiple images for GIF creation, THE UI_Layer SHALL accept the images as animation frames
2. THE UI_Layer SHALL allow the user to set frame delay between 100 milliseconds and 800 milliseconds
3. THE UI_Layer SHALL allow the user to adjust output dimensions
4. THE UI_Layer SHALL allow the user to enable reverse playback mode
5. THE UI_Layer SHALL allow the user to enable ping-pong playback mode (forward then reverse loop)
6. WHEN a user exports a GIF, THE Core_Engine SHALL encode the frames with the configured delay and playback mode
7. FOR ALL GIFs exported by the Core_Engine, playback in system image viewers SHALL display correctly
8. FOR ALL GIFs exported by the Core_Engine, playback in messaging applications SHALL display correctly
9. THE UI_Layer SHALL render an animated preview of the configured frame order, delay, dimensions, and playback mode before export
10. THE UI_Layer SHALL provide play and pause controls and SHALL display the current frame index and total duration
11. WHEN the user changes frame order, delay, dimensions, or playback mode, THE animated preview SHALL update to match the next exported GIF
12. THE UI_Layer SHALL NOT represent a static first frame as a completed GIF preview

### [v2] Requirement 16: 海报设计

**User Story:** 作为营销人员,我希望使用 AI 生成的背景结合本地文字排版创作海报,确保文字清晰无误。

#### Acceptance Criteria

1. THE UI_Layer SHALL provide canvas presets including 9:16 mobile poster format
2. THE UI_Layer SHALL allow the user to generate AI background with style selection
3. WHERE the user selects a style such as tech launch, THE AI_Engine SHALL generate an appropriate background image
4. THE UI_Layer SHALL allow the user to add a main title Text_Layer
5. THE UI_Layer SHALL allow the user to add a subtitle Text_Layer
6. THE UI_Layer SHALL allow the user to add a corner label Text_Layer
7. THE UI_Layer SHALL render all Text_Layer elements using GPUI TextSystem
8. THE UI_Layer SHALL render all Text_Layer elements using DirectWrite on Windows platform
9. THE UI_Layer SHALL allow the user to drag Text_Layer elements to adjust position
10. THE UI_Layer SHALL allow the user to resize Text_Layer elements
11. WHEN a user exports a poster, THE Core_Engine SHALL perform offscreen composition combining AI background and Text_Layer elements
12. WHEN a user exports a poster, THE Core_Engine SHALL produce output where text rendering matches the canvas preview exactly
13. FOR ALL exported posters, text SHALL be rendered clearly without character errors


### [v2] Requirement 17: AI 文生图与图生图

**User Story:** 作为创意工作者,我希望通过文字描述或参考图片生成新图像,以快速获得创意素材。

#### Acceptance Criteria

1. WHEN a user enters a text prompt, THE AI_Engine SHALL send a text-to-image generation request to the configured Provider
2. WHEN a user uploads a reference image with a prompt, THE AI_Engine SHALL send an image-to-image generation request to the configured Provider
3. THE UI_Layer SHALL allow the user to select output aspect ratio
4. THE UI_Layer SHALL allow the user to specify the number of images to generate
5. THE UI_Layer SHALL allow the user to switch between configured Providers
6. WHEN a user changes the selected Provider, THE UI_Layer SHALL adjust available aspect ratio options according to Provider capabilities
7. WHEN a user changes the selected Provider, THE UI_Layer SHALL adjust maximum generation count according to Provider capabilities
8. WHEN generation completes with multiple results, THE UI_Layer SHALL display all generated images
9. THE UI_Layer SHALL allow the user to select any subset of generated images for download

### [v2] Requirement 18: AI 改图 - 12 个场景预设

**User Story:** 作为普通用户,我希望无需编写提示词即可使用 AI 改图功能,通过选择预设场景快速完成图像修改。

#### Acceptance Criteria

1. THE UI_Layer SHALL provide AI removal tool with region selection capability
2. THE UI_Layer SHALL provide background replacement tool
3. THE UI_Layer SHALL provide image expansion tool
4. THE UI_Layer SHALL provide clarity enhancement tool
5. THE UI_Layer SHALL provide background removal tool
6. THE UI_Layer SHALL provide e-commerce white background tool
7. THE UI_Layer SHALL provide portrait enhancement tool
8. THE UI_Layer SHALL provide text replacement tool
9. THE UI_Layer SHALL provide sticker application tool
10. THE UI_Layer SHALL provide product photo set tool
11. THE UI_Layer SHALL provide professional portrait photo tool
12. THE UI_Layer SHALL provide freeform editing tool for advanced users
13. WHERE AI removal is selected, THE UI_Layer SHALL allow the user to select regions to remove
14. WHERE AI removal is selected, THE AI_Engine SHALL generate output with selected regions cleanly removed
15. WHERE e-commerce white background is selected, THE AI_Engine SHALL generate output with product extracted and placed on pure white background
16. FOR ALL AI editing tools except freeform editing, THE Preset_Library SHALL provide locked prompts invisible to users


### [v2] Requirement 19: 行业工具 - 老照片修复

**User Story:** 作为用户,我希望修复老旧照片的破损、上色并增强清晰度,同时保持人物特征不变。

#### Acceptance Criteria

1. WHEN a user uploads an old photo, THE UI_Layer SHALL accept the image
2. THE UI_Layer SHALL provide a damage repair option
3. THE UI_Layer SHALL provide a colorization option for black-and-white photos
4. THE UI_Layer SHALL provide a clarity enhancement option
5. WHEN a user triggers restoration, THE AI_Engine SHALL send the image with locked prompt from Preset_Library to the configured Provider
6. WHEN restoration completes, THE AI_Engine SHALL return the restored photo
7. THE UI_Layer SHALL provide a before-after comparison view with press-and-hold interaction
8. THE Preset_Library SHALL store the locked restoration prompt invisible to users

### [v2] Requirement 20: 行业工具 - AI 证件照

**User Story:** 作为用户,我希望将生活照转换为符合标准规格的证件照,包括指定背景色和着装。

#### Acceptance Criteria

1. WHEN a user uploads a clear frontal face photo, THE UI_Layer SHALL accept the image
2. THE UI_Layer SHALL provide size specification options including one-inch, two-inch, and visa photo standard pixel dimensions
3. THE UI_Layer SHALL provide background color options including blue, red, and white
4. THE UI_Layer SHALL provide attire options including formal suit for males
5. WHEN a user triggers generation, THE AI_Engine SHALL send the image with locked prompt from Preset_Library to the configured Provider
6. WHEN generation completes, THE AI_Engine SHALL return an ID photo conforming to the selected pixel dimensions specification
7. THE Preset_Library SHALL store the locked ID photo prompt invisible to users


### [v2] Requirement 21: 行业工具 - 头像工坊

**User Story:** 作为社交媒体用户,我希望将自拍照转换为多种风格的 1:1 头像,以丰富个人形象展示。

#### Acceptance Criteria

1. WHEN a user uploads a selfie, THE UI_Layer SHALL accept the image
2. THE UI_Layer SHALL provide 9 style options for selection
3. THE UI_Layer SHALL allow the user to select up to 6 styles simultaneously
4. THE UI_Layer SHALL include Japanese anime style as a selectable option
5. THE UI_Layer SHALL include cyberpunk style as a selectable option
6. THE UI_Layer SHALL include Chinese ink painting style as a selectable option
7. WHEN a user triggers generation, THE AI_Engine SHALL send requests for each selected style with corresponding locked prompts from Preset_Library
8. WHEN generation completes, THE AI_Engine SHALL return one 1:1 avatar image for each selected style
9. THE Preset_Library SHALL store locked prompts for all 9 avatar styles invisible to users

### [v2] Requirement 22: 行业工具 - 表情包生成器

**User Story:** 作为聊天爱好者,我希望将照片批量转换为 Q 版表情包,包含多种情绪表情。

#### Acceptance Criteria

1. WHEN a user uploads a photo with clear facial features, THE UI_Layer SHALL accept the image
2. THE UI_Layer SHALL provide 6 to 12 emotion Tier（档位）choices for selection
3. THE UI_Layer SHALL include laughing emotion as a selectable option
4. THE UI_Layer SHALL include shocked emotion as a selectable option
5. THE UI_Layer SHALL include eye-rolling emotion as a selectable option
6. WHEN a user triggers generation, THE AI_Engine SHALL send requests for each selected emotion with corresponding locked prompts from Preset_Library
7. WHEN generation completes, THE AI_Engine SHALL return Q-style sticker images in batch
8. THE Preset_Library SHALL store locked prompts for all emotion variations invisible to users


### [v2] Requirement 23: 行业工具 - AI 写真

**User Story:** 作为用户,我希望将人像照转换为不同主题的写真照片,保持人物相貌特征的同时更换场景和服装。

#### Acceptance Criteria

1. WHEN a user uploads a clear portrait photo, THE UI_Layer SHALL accept the image
2. THE UI_Layer SHALL provide 8 theme options including wedding, traditional Hanfu, graduation, travel, and professional
3. WHEN a user selects a theme and triggers generation, THE AI_Engine SHALL send the image with corresponding locked prompt from Preset_Library to the configured Provider
4. WHEN generation completes, THE AI_Engine SHALL return a themed portrait photo
5. THE Preset_Library SHALL store locked prompts for all 8 portrait themes invisible to users

### [v2] Requirement 24: 行业工具 - 模特试穿

**User Story:** 作为服装商家,我希望将平铺或挂拍的服装图生成模特上身效果,保持服装细节的准确性。

#### Acceptance Criteria

1. WHEN a user uploads a flat-lay or hanging garment photo, THE UI_Layer SHALL accept the image
2. THE UI_Layer SHALL provide model selection options including Asian female model
3. THE UI_Layer SHALL provide scene selection options including cafe setting
4. WHEN a user triggers generation, THE AI_Engine SHALL send the image with locked prompt from Preset_Library to the configured Provider
5. WHEN generation completes, THE AI_Engine SHALL return a model wearing photo
6. THE Preset_Library SHALL store locked model try-on prompts invisible to users

### [v2] Requirement 25: 行业工具 - 商品改色

**User Story:** 作为电商卖家,我希望批量生成商品的不同配色版本,保持商品外形和材质不变。

#### Acceptance Criteria

1. WHEN a user uploads a product image, THE UI_Layer SHALL accept the image
2. THE UI_Layer SHALL provide multiple color palette options including burgundy, navy, and haze blue
3. THE UI_Layer SHALL allow the user to select custom colors using a color picker
4. THE UI_Layer SHALL allow the user to select multiple colors simultaneously
5. WHEN a user triggers generation, THE AI_Engine SHALL send requests for each selected color with corresponding locked prompts from Preset_Library
6. WHEN generation completes, THE AI_Engine SHALL return product images in each selected color variation
7. THE Preset_Library SHALL store locked color variation prompts invisible to users


### [v2] Requirement 26: 行业工具 - 促销海报

**User Story:** 作为商家,我希望基于商品图快速生成促销海报,包含活动信息和吸引人的视觉设计。

#### Acceptance Criteria

1. WHEN a user uploads a product image, THE UI_Layer SHALL accept the image
2. THE UI_Layer SHALL provide activity type options including limited-time discount, new arrival, clearance, and member day
3. THE UI_Layer SHALL allow the user to enter custom main title text
4. THE UI_Layer SHALL allow the user to leave title text blank for AI-generated copy
5. WHEN a user triggers generation, THE AI_Engine SHALL send the image with locked prompt from Preset_Library to the configured Provider
6. WHEN generation completes, THE AI_Engine SHALL return a vertical 3:4 promotional poster
7. THE Preset_Library SHALL store locked promotional poster prompts invisible to users

### [v2] Requirement 27: 行业工具 - 平台尺寸适配

**User Story:** 作为多平台运营者,我希望将一张图片适配到不同平台的规格要求,避免变形和裁切主体。

#### Acceptance Criteria

1. WHEN a user uploads an image, THE UI_Layer SHALL accept the image
2. THE UI_Layer SHALL provide target platform specification options including Taobao main image, Xiaohongshu note 1242x1656, WeChat article cover, and Douyin vertical format
3. WHEN the source image aspect ratio is close to the target specification, THE Core_Engine SHALL perform precise local cropping without consuming AI quota
4. WHEN the source image aspect ratio significantly differs from the target specification, THE AI_Engine SHALL perform AI expansion first
5. WHEN AI expansion completes, THE Core_Engine SHALL perform cropping to exact target dimensions
6. THE Core_Engine SHALL ensure no distortion occurs during adaptation
7. THE Core_Engine SHALL ensure main subject is not cropped during adaptation
8. THE Preset_Library SHALL store locked platform adaptation prompts invisible to users


### [v2] Requirement 28: 行业工具 - 封面图工厂

**User Story:** 作为内容创作者,我希望基于标题文本生成适配不同平台的封面图,包含合适的排版和视觉风格。

#### Acceptance Criteria

1. WHEN a user enters title text, THE UI_Layer SHALL accept the input
2. THE UI_Layer SHALL provide platform options including WeChat Official Account, Xiaohongshu, Bilibili, and Douyin
3. THE UI_Layer SHALL provide style options including bold typography style
4. WHEN a user triggers generation, THE AI_Engine SHALL send the title text with locked prompt from Preset_Library to the configured Provider
5. WHEN generation completes, THE AI_Engine SHALL return a cover image with title layout
6. THE Preset_Library SHALL store locked cover image prompts invisible to users

### [v2] Requirement 29: 行业工具 - 文章配图

**User Story:** 作为文章作者,我希望根据主题描述生成系列风格一致的插图,用于文章配图和演示。

#### Acceptance Criteria

1. WHEN a user enters a theme description, THE UI_Layer SHALL accept the input
2. THE UI_Layer SHALL provide usage options including PPT illustration
3. THE UI_Layer SHALL provide style options including business gradient style
4. WHEN a user triggers generation, THE AI_Engine SHALL send the description with locked prompt from Preset_Library to the configured Provider
5. WHEN generation completes, THE AI_Engine SHALL return an illustration without text overlay
6. THE UI_Layer SHALL allow the user to repeatedly generate while maintaining consistent series style
7. THE Preset_Library SHALL store locked article illustration prompts invisible to users

### [v2] Requirement 30: 行业工具 - 菜品图美化

**User Story:** 作为餐饮商家,我希望将手机拍摄的菜品照提升为专业美食图,保持菜品真实性的同时优化光影和色泽。

#### Acceptance Criteria

1. WHEN a user uploads a phone-captured dish photo, THE UI_Layer SHALL accept the image
2. THE UI_Layer SHALL provide style options including delivery standard style
3. WHEN a user triggers enhancement, THE AI_Engine SHALL send the image with locked prompt from Preset_Library to the configured Provider
4. WHEN enhancement completes, THE AI_Engine SHALL return a food photo with improved lighting, color, and background
5. THE Preset_Library SHALL store locked food enhancement prompts invisible to users


### [v2] Requirement 31: 行业工具 - 房间软装预览

**User Story:** 作为装修用户,我希望预览房间在不同软装风格下的效果,保持房间结构不变的同时更换装饰风格。

#### Acceptance Criteria

1. WHEN a user uploads a room photo (rough or furnished), THE UI_Layer SHALL accept the image
2. THE UI_Layer SHALL provide style options including Scandinavian, Modern Chinese, and Light Luxury
3. THE UI_Layer SHALL allow the user to select up to 4 styles simultaneously
4. WHEN a user triggers generation, THE AI_Engine SHALL send requests for each selected style with corresponding locked prompts from Preset_Library
5. WHEN generation completes, THE AI_Engine SHALL return style comparison renderings
6. THE Preset_Library SHALL store locked interior design prompts invisible to users

### [v1] Requirement 32: 配置管理

**User Story:** 作为用户,我希望自定义默认导出设置,以提高工作效率。

#### Acceptance Criteria

1. THE Impressy_System SHALL store configuration in TOML format
2. THE Impressy_System SHALL store configuration file in the standard application data directory defined by the directories crate
3. 〔v2〕THE Impressy_System SHALL store default Provider selection in configuration
4. THE Impressy_System SHALL store default export format in configuration
5. THE Impressy_System SHALL store default export quality in configuration
6. 〔v2〕THE Impressy_System SHALL NOT store API_Key in the configuration file
7. 〔v2〕THE Impressy_System SHALL store generation history records locally only
8. THE Impressy_System SHALL store the last output directory path locally only
9. 〔v2〕THE UI_Layer SHALL provide a clear all history function


### [v2] Requirement 33: 提示词模板管理

**User Story:** 作为开发者,我希望以可维护的方式管理所有行业工具的锁定提示词,便于版本迭代和优化。

#### Acceptance Criteria

1. THE Preset_Library SHALL store all industry tool locked prompts as built-in resources
2. THE Preset_Library SHALL compile all prompts into the application binary
3. THE UI_Layer SHALL NOT display locked prompts to users
4. THE Preset_Library SHALL organize prompts as separate template files in the impressy-presets crate within the code repository
5. THE Preset_Library SHALL allow prompt iteration and optimization through code repository updates

### [v1] Requirement 34: 主界面导航

**User Story:** 作为用户,我希望通过清晰的界面导航访问所有功能,包括开发中的功能预留。

#### Acceptance Criteria

1. THE UI_Layer SHALL provide navigation to the basic image processing section
2. THE UI_Layer SHALL provide navigation to the AI generation and editing section
3. THE UI_Layer SHALL provide navigation to the industry-specific AI tools section
4. THE UI_Layer SHALL provide navigation to the creative output section
5. THE UI_Layer SHALL provide entry points for video watermark removal feature
6. THE UI_Layer SHALL provide entry points for video subtitle removal feature
7. THE UI_Layer SHALL provide entry points for video clarity enhancement feature
8. WHEN a user clicks on video watermark removal entry, THE UI_Layer SHALL display an under development placeholder
9. WHEN a user clicks on video subtitle removal entry, THE UI_Layer SHALL display an under development placeholder
10. WHEN a user clicks on video clarity enhancement entry, THE UI_Layer SHALL display an under development placeholder
11. DURING v1, WHEN a user opens the AI generation and editing section, THE UI_Layer SHALL display an under development placeholder and SHALL NOT request network access or an API_Key
12. DURING v1, WHEN a user opens the industry-specific AI tools section, THE UI_Layer SHALL display an under development placeholder and SHALL NOT request network access or an API_Key
13. FOR EACH feature, the sidebar, search results, platform menus, and active-section indicator SHALL use one canonical section assignment
14. AFTER a Current_Document is opened, THE UI_Layer SHALL keep document identity, undo, redo, original comparison, and export actions visible above the workspace
15. THE UI_Layer SHALL place parameters for only the active tool in a side panel that scrolls independently from the image canvas
16. THE UI_Layer SHALL keep the active tool's primary apply, generate, or export action visible without requiring the user to scroll the canvas
17. THE image canvas SHALL provide fit-to-window, 100%, zoom, and pan controls and SHALL distinguish transparent pixels from an opaque background


### [v1] Requirement 35: 多语言支持

**User Story:** 作为国际用户,我希望使用中文或英文界面,以便更好地理解和使用软件功能。

#### Acceptance Criteria

1. THE UI_Layer SHALL support Simplified Chinese language
2. THE UI_Layer SHALL support English language
3. THE UI_Layer SHALL allow the user to switch between supported languages in settings
4. WHEN the user switches language, THE UI_Layer and gpui-component SHALL update existing visible text without restarting the application
5. ALL user-visible v1 UI text SHALL use i18n keys with both `en` and `zh-CN` values; user-visible strings SHALL NOT be hard-coded in Rust UI code

### [v1] Requirement 36: 错误处理与用户反馈

**User Story:** 作为用户,我希望在操作失败时获得清晰的错误信息和恢复建议,以便解决问题继续工作。

#### Acceptance Criteria

1. IF an image file cannot be opened due to unsupported format, THEN THE UI_Layer SHALL display an unsupported format error message
2. IF an image file cannot be opened due to file corruption, THEN THE UI_Layer SHALL display a corrupted file error message
3. IF a batch processing item fails, THEN THE UI_Layer SHALL display the specific failure reason for that item
4. IF an export operation fails due to insufficient disk space, THEN THE UI_Layer SHALL display a disk space error message
5. IF an export operation fails due to permission issues, THEN THE UI_Layer SHALL display a permission error message
6. THE UI_Layer SHALL provide user-friendly error messages in the selected interface language
7. THE UI_Layer SHALL categorize errors to help users understand the root cause
8. THE UI_Layer SHALL associate progress, success, and failure messages with the task that produced them and SHALL clear or relabel them when the user changes to an unrelated task
9. WHEN source and current-result properties differ, THE UI_Layer SHALL label their dimensions, format, and file size separately
10. WHEN an operation fails and a recovery action is available, THE UI_Layer SHALL present that action next to the failure message
11. THE UI_Layer SHALL keep implementation names such as crate, executor, and internal state identifiers out of ordinary user guidance
12. THE UI_Layer SHALL communicate selected, disabled, warning, success, and failure states with text or icon semantics in addition to color

### [v1] Requirement 37: 性能与响应式

**User Story:** 作为用户,我希望软件启动快速、操作流畅,即使在处理大量图片时界面也保持响应。

#### Acceptance Criteria

1. WHEN the application cold starts, THE Impressy_System SHALL display the main interface within 1500 milliseconds
2. WHEN opening an image file up to 50 megapixels, THE UI_Layer SHALL display the image within 2000 milliseconds
3. WHILE batch processing 100 images, THE UI_Layer SHALL update progress indicators smoothly
4. WHILE batch processing 100 images, THE UI_Layer SHALL respond to user clicks within 100 milliseconds
5. WHEN applying real-time preview effects, THE UI_Layer SHALL update the preview within 200 milliseconds of parameter adjustment
6. AT application content sizes corresponding to 1280×800 and 1366×768 desktop windows, THE UI_Layer SHALL keep the active primary action visible and SHALL NOT clip the Current_Document canvas or active parameter controls
7. WHEN parameter controls exceed available vertical space, THE parameter panel SHALL scroll independently while the Current_Document canvas remains visible


### [v1] Requirement 38: 图像格式支持

**User Story:** 作为用户,我希望打开和导出多种常见图像格式,以满足不同场景需求。

#### Acceptance Criteria

1. THE Core_Engine SHALL support reading PNG format images
2. THE Core_Engine SHALL support reading JPEG format images
3. THE Core_Engine SHALL support reading WebP format images
4. THE Core_Engine SHALL support reading GIF format images
5. THE Core_Engine SHALL support reading BMP format images
6. THE Core_Engine SHALL support writing PNG format images with adjustable compression level
7. THE Core_Engine SHALL support writing JPEG format images with adjustable quality from 1 to 100
8. THE Core_Engine SHALL support writing WebP format images with adjustable quality from 1 to 100
9. THE Core_Engine SHALL support writing GIF format images with frame delay control
10. THE Core_Engine SHALL preserve transparency when writing PNG format images
11. THE Core_Engine SHALL preserve transparency when writing WebP format images in lossless mode

### [v1] Requirement 39: 图像质量控制

**User Story:** 作为注重文件大小的用户,我希望精确控制输出图像的质量和压缩率,平衡文件大小与视觉效果。

#### Acceptance Criteria

1. WHERE JPEG export is selected, THE UI_Layer SHALL provide a quality slider ranging from 1 to 100
2. WHERE WebP export is selected, THE UI_Layer SHALL provide a quality slider ranging from 1 to 100
3. WHERE PNG export is selected, THE UI_Layer SHALL provide compression level options
4. WHEN a user adjusts quality settings, THE UI_Layer SHALL display estimated file size
5. WHEN a user exports with quality setting 85, THE Core_Engine SHALL encode the image with quality parameter 85
6. FOR ALL quality settings, THE Core_Engine SHALL apply the exact specified quality value to the encoder
7. WHEN the user invokes export, THE UI_Layer SHALL display format, dimensions, format-appropriate quality or compression, estimated file size, filename, and destination in one export flow
8. THE settings page SHALL store export defaults, while a one-time export change SHALL apply only to that export unless the user explicitly saves it as a default
9. WHERE PNG export is selected, THE UI_Layer SHALL describe compression as a file-size and encoding-speed choice and SHALL NOT present it as lossy visual quality
10. WHERE JPEG export is selected for a Current_Document containing transparency, THE UI_Layer SHALL display the opaque background treatment in the export preview


### [v1] Requirement 40: 剪贴板集成

**User Story:** 作为高效用户,我希望通过剪贴板快速粘贴图像进行编辑和复制输出结果。

#### Acceptance Criteria

1. WHEN the system clipboard contains image data, THE UI_Layer SHALL allow pasting the image into any image input field
2. WHEN a user copies an edited image, THE Impressy_System SHALL place the image data in the system clipboard
3. THE Impressy_System SHALL support pasting images from clipboard across different applications

### [v1] Requirement 41: 文件输出与命名

**User Story:** 作为用户,我希望输出文件具有清晰的命名规则和可选的保存位置,便于文件管理。

#### Acceptance Criteria

1. WHEN a user exports a single image, THE UI_Layer SHALL prompt the user to select save location and filename
2. WHEN a user exports batch processed images, THE UI_Layer SHALL prompt the user to select output directory
3. WHEN batch processing outputs multiple files, THE Core_Engine SHALL append sequential numbers to filenames
4. WHEN slicing outputs multiple tiles, THE Core_Engine SHALL append grid position identifiers to filenames
5. 〔v2〕WHEN multiple AI images are generated, THE Core_Engine SHALL use timestamp-based filenames to prevent overwrites
6. THE Impressy_System SHALL remember the last used output directory in local configuration
7. THE UI_Layer SHALL provide an option to open the output directory after export completes
8. THE UI_Layer SHALL use the term export for writing ordinary image files and SHALL NOT imply that Impressy saves a reopenable project file
9. WHEN exporting a single Current_Document, THE UI_Layer SHALL default to creating a copy rather than overwriting the source file
10. WHEN batch or slicing output collides with an existing filename, THE default policy SHALL preserve both files by generating a stable non-conflicting filename
11. THE UI_Layer SHALL allow the user to select preserve-both, skip, or replace as the collision policy for a multi-file export
12. THE Impressy_System SHALL overwrite an existing output only after the user explicitly selects the replace policy
13. WHEN replacing an existing output, THE Impressy_System SHALL finish writing and validating a temporary file before atomically replacing the destination where the platform supports atomic replacement
14. IF a multi-file export is interrupted, every reported successful output SHALL remain a complete decodable file and no partial file SHALL be reported as successful


### [v1] Requirement 42: 输入验证与用户引导

**User Story:** 作为新用户,我希望软件能够验证我的输入并提供清晰的提示,避免无效操作。

#### Acceptance Criteria

1. IF a user attempts to export without selecting an image, THEN THE UI_Layer SHALL display a no image selected warning
2. 〔v2〕IF a user attempts to generate AI image without configuring API_Key, THEN THE UI_Layer SHALL display an API key required message with link to settings
3. 〔v2〕IF a user uploads an image exceeding size limits for AI processing, THEN THE UI_Layer SHALL display an image too large warning
4. IF a user enters invalid text in a numeric field, THEN THE UI_Layer SHALL highlight the field and display an invalid input message
5. WHERE a field has specific format requirements, THE UI_Layer SHALL display format hints below the input field
6. WHEN a user hovers over an unfamiliar option, THE UI_Layer SHALL display a tooltip explanation
7. IF a user attempts batch processing with no files selected, THEN THE UI_Layer SHALL display a no files selected warning
8. WHEN the user changes tools or starts export while a Transient_Preview exists, THE UI_Layer SHALL require a choice among apply, discard, or return before continuing
9. WHEN the user closes or replaces a Current_Document that contains unexported applied changes, THE UI_Layer SHALL require a choice among export, discard, or cancel
10. THE UI_Layer SHALL distinguish resetting the active tool from restoring the Current_Document to its original image

### [v1] Requirement 43: 版本锁定与依赖管理

**User Story:** 作为开发者,我希望项目依赖版本精确锁定,确保构建稳定性和可重现性。

#### Acceptance Criteria

1. THE Impressy_System SHALL lock the gpui crate to a specific version in Cargo workspace
2. THE Impressy_System SHALL lock the gpui-component crate to a specific version in Cargo workspace
3. THE Impressy_System SHALL document the locked versions in project documentation
4. WHEN upgrading gpui or gpui-component versions, THE development team SHALL treat the upgrade as an independent task
5. THE Impressy_System SHALL ensure gpui and gpui-component versions are compatible with each other
6. THE Cargo workspace configuration SHALL specify exact versions for all critical dependencies


### [v1] Requirement 44: 代码质量与测试

**User Story:** 作为开发团队成员,我希望代码通过自动化质量检查,确保代码库的稳定性和可维护性。

#### Acceptance Criteria

1. THE Impressy_System SHALL pass cargo check without errors
2. THE Impressy_System SHALL pass cargo clippy with warning level set to deny without any warnings
3. THE Impressy_System SHALL pass all unit tests executed by cargo test
4. WHEN a developer completes a task, THE code changes SHALL pass all three quality checks before task completion
5. THE project repository SHALL include automated quality check scripts
6. THE project documentation SHALL define quality gates as part of Definition of Done

### [v1] Requirement 45: 参考文档与代码生成约束

**User Story:** 作为 AI 辅助开发项目,我希望代码生成基于准确的 API 参考而非过时的模型记忆。

#### Acceptance Criteria

1. THE project repository SHALL include a vendor-docs directory containing GPUI API references
2. THE vendor-docs directory SHALL include gpui-component official examples for the locked version
3. THE vendor-docs directory SHALL include key API documentation excerpts for the locked version
4. THE project repository SHALL include a CLAUDE.md file declaring Rust toolchain version
5. THE CLAUDE.md file SHALL declare locked gpui and gpui-component versions
6. THE CLAUDE.md file SHALL declare workspace crate responsibilities
7. THE CLAUDE.md file SHALL declare quality gate commands


### [v1] Requirement 46: 安装与分发

**User Story:** 作为最终用户,我希望通过简单的安装包快速安装软件,并在未来获得更新通知。

#### Acceptance Criteria

1. THE Impressy_System SHALL provide a Windows x64 MSI installer
2. THE Windows executable and MSI installer SHALL be digitally signed for security verification
3. THE Impressy_System SHALL provide a signed and notarized macOS application inside a DMG
4. THE Impressy_System SHALL provide a Debian / Ubuntu amd64 DEB package with a desktop entry, application icons, and explicit native runtime dependencies
5. THE Impressy_System SHALL ship a native compiled executable without a browser engine or bundled web runtime
6. EACH executable and installer artifact SHALL NOT exceed 30 MiB
7. THE Windows installer SHALL create an application shortcut in the Start Menu
8. THE Windows installer SHALL register file type associations for PNG, JPEG, WebP, BMP, and GIF
9. THE CI pipeline SHALL smoke-test installation structure and uninstall cleanup for every package format that its runner can install non-interactively
10. THE release workflow SHALL reject a release when the committed desktop acceptance evidence is missing, stale, or fails any required platform, feature, performance, signing, or package check

### [v1] Requirement 47: 自定义 UI 元素

**User Story:** 作为用户,我希望使用流畅的交互式画布进行裁剪和图层操作。

#### Acceptance Criteria

1. THE UI_Layer SHALL implement crop frame as a custom Canvas_Element
2. THE crop frame Canvas_Element SHALL support drag gestures to adjust position
3. THE crop frame Canvas_Element SHALL support drag gestures on corner handles to adjust size
4. 〔v2〕THE UI_Layer SHALL implement text layers in poster design as draggable Canvas_Element components
5. 〔v2〕THE text layer Canvas_Element SHALL support position adjustment through drag gestures
6. 〔v2〕THE text layer Canvas_Element SHALL support size adjustment through corner handle dragging
7. FOR ALL Canvas_Element implementations, THE UI_Layer SHALL follow GPUI three-phase rendering pipeline: request_layout, prepaint, and paint
8. THE UI_Layer SHALL expose an accessible name, role, enabled state, and selected state for every interactive control when the platform accessibility API supports those semantics
9. THE UI_Layer SHALL provide a visible keyboard focus indicator and a logical Tab order for all core workflow controls
10. THE UI_Layer SHALL allow keyboard-only completion of image open, proportional resize, undo, redo, export, and export confirmation
11. THE crop frame Canvas_Element SHALL provide keyboard operations for moving and resizing the selection and SHALL expose the current selection bounds to assistive technology where supported
12. WHEN a text input owns focus, text-field undo and redo SHALL take precedence over Current_Document undo and redo


### [removed] Requirement 48: 多显示器与 DPI 支持

截图专用的多显示器与 DPI 捕获要求已从当前产品范围剥离。见 [ADR-0003](../adr/0003-remove-screen-capture.md)。

### [removed] Requirement 49: 热键冲突处理

截图全局热键已从当前产品范围剥离。见 [ADR-0003](../adr/0003-remove-screen-capture.md)。


### [v2] Requirement 50: AI 提供商能力声明

**User Story:** 作为系统架构师,我希望每个 AI 提供商明确声明其能力限制,使上层业务逻辑能够动态适配。

#### Acceptance Criteria

1. THE AI_Engine Provider trait SHALL declare maximum reference image count capability
2. THE AI_Engine Provider trait SHALL declare supported aspect ratio list capability
3. THE AI_Engine Provider trait SHALL declare maximum generation count per request capability
4. THE AI_Engine Provider trait SHALL declare whether text-to-image generation is supported
5. THE AI_Engine Provider trait SHALL declare whether image-to-image generation is supported
6. THE AI_Engine Provider trait SHALL declare whether region-based editing is supported
7. WHEN the user switches Provider in UI, THE UI_Layer SHALL query the current Provider capabilities
8. WHEN the user switches Provider in UI, THE UI_Layer SHALL disable options not supported by the selected Provider
9. WHEN the user switches Provider in UI, THE UI_Layer SHALL adjust input field constraints according to Provider capability declarations

### [v1] Requirement 51: 离线功能隔离

**User Story:** 作为用户,我希望在没有网络连接时仍能使用所有基础图像处理功能。

#### Acceptance Criteria

1. THE Core_Engine SHALL execute image editing functions without requiring network connectivity
2. THE Core_Engine SHALL execute collage functions without requiring network connectivity
3. THE Core_Engine SHALL execute batch processing functions without requiring network connectivity
4. THE Core_Engine SHALL execute slicing functions without requiring network connectivity
5. THE Core_Engine SHALL execute QR code generation without requiring network connectivity
6. THE Core_Engine SHALL execute QR code recognition without requiring network connectivity
7. THE Core_Engine SHALL execute EXIF viewing without requiring network connectivity
8. THE Core_Engine SHALL execute EXIF cleaning without requiring network connectivity
9. THE Core_Engine SHALL execute screenshot beautification without requiring network connectivity
10. THE Core_Engine SHALL execute GIF creation without requiring network connectivity
11. 〔v2〕WHEN network connectivity is unavailable, THE UI_Layer SHALL display AI-dependent features as unavailable with clear indication
12. WHEN network connectivity is unavailable, THE UI_Layer SHALL allow full access to all Core_Engine features


### [v1] Requirement 52: 图像解析与打印的往返一致性

**User Story:** 作为开发者,我希望确保图像编解码的正确性,验证编码后解码能恢复原始数据。

#### Acceptance Criteria

1. THE Core_Engine SHALL implement an image encoder for PNG format
2. THE Core_Engine SHALL implement an image decoder for PNG format
3. THE Core_Engine SHALL implement an image encoder for JPEG format
4. THE Core_Engine SHALL implement an image decoder for JPEG format
5. THE Core_Engine SHALL implement an image encoder for WebP format
6. THE Core_Engine SHALL implement an image decoder for WebP format
7. FOR ALL images encoded in lossless PNG format, decoding the encoded output SHALL produce pixel data identical to the original input
8. FOR ALL images encoded in lossless WebP format, decoding the encoded output SHALL produce pixel data identical to the original input
9. THE Core_Engine SHALL implement QR code generator
10. THE Core_Engine SHALL implement QR code parser
11. FOR ALL text strings encoded as QR codes in PNG format, parsing the generated QR code image SHALL decode to the original text string
12. FOR ALL URLs encoded as QR codes, the round-trip property (generate → save as PNG → parse) SHALL preserve the original URL exactly

### [v1] Requirement 53: 配置解析器往返一致性

**User Story:** 作为开发者,我希望配置文件的读写保持一致性,避免配置数据丢失或损坏。

#### Acceptance Criteria

1. THE Impressy_System SHALL implement a configuration serializer to TOML format
2. THE Impressy_System SHALL implement a configuration parser from TOML format
3. FOR ALL valid configuration objects, serializing to TOML then parsing back SHALL produce an equivalent configuration object
4. THE configuration parser SHALL validate TOML syntax before parsing
5. IF configuration file is corrupted, THEN THE Impressy_System SHALL load default configuration and display a configuration reset warning


### [v1] Requirement 54: 批量处理不变性属性

**User Story:** 作为质量保证工程师,我希望验证批量处理操作保持关键属性不变。

#### Acceptance Criteria

1. WHEN batch processing applies format conversion, THE Core_Engine SHALL preserve image dimensions for each processed image
2. WHEN batch processing applies compression, THE Core_Engine SHALL preserve image dimensions for each processed image
3. WHEN batch processing applies resize with scale factor, THE output image count SHALL equal the input image count
4. WHEN batch processing applies watermark, THE output image count SHALL equal the input image count
5. FOR ALL batch operations, THE Core_Engine SHALL preserve the processing order matching input order
6. FOR ALL batch operations where individual items fail, THE number of successful outputs plus failed items SHALL equal the total input count

### [v1] Requirement 55: 图像变换不变性属性

**User Story:** 作为质量保证工程师,我希望验证图像变换操作保持应有的不变性。

#### Acceptance Criteria

1. WHEN an image is cropped, THE output image aspect ratio SHALL match the selected preset aspect ratio within 0.1% tolerance
2. WHEN an image is rotated by 360 degrees, THE output image SHALL be visually identical to the input image
3. WHEN a collage is composed in grid mode with N images, THE output collage SHALL contain exactly N visible image regions
4. WHEN an image is sliced into M rows and N columns, THE Core_Engine SHALL produce exactly M × N output tiles
5. WHEN EXIF data is cleaned from an image, THE output image pixel data SHALL remain identical to the input pixel data
6. WHEN screenshot beautification applies inner padding P, THE output image width and height SHALL each increase by exactly 2P pixels; corner radius SHALL affect the alpha mask rather than the outer dimensions


### [v1] Requirement 56: 幂等性属性验证

**User Story:** 作为质量保证工程师,我希望验证某些操作具有幂等性,重复执行不会产生额外效果。

#### Acceptance Criteria

1. WHEN EXIF cleaning is applied to an already-cleaned image, THE output SHALL be identical to the input
2. WHEN format conversion from PNG to PNG is applied, THE output SHALL be equivalent to the input
3. WHEN a QR code recognition result is re-encoded and re-decoded, THE decoded text SHALL remain identical
4. WHEN configuration is saved and immediately reloaded, THE loaded configuration SHALL equal the saved configuration

### [v1] Requirement 57: 错误条件属性验证

**User Story:** 作为质量保证工程师,我希望验证系统对无效输入正确报错而不崩溃。

#### Acceptance Criteria

1. WHEN an invalid image file is opened, THE Core_Engine SHALL return an error without crashing
2. WHEN a corrupted QR code image is parsed, THE Core_Engine SHALL return a decoding error without crashing
3. 〔v2〕WHEN an API request fails with HTTP 401, THE AI_Engine SHALL return an invalid credentials error without crashing
4. 〔v2〕WHEN an API request fails with HTTP 429, THE AI_Engine SHALL return a rate limit error without crashing
5. WHEN disk space is insufficient for export, THE Core_Engine SHALL return a disk space error without crashing
6. WHEN invalid TOML syntax is encountered, THE configuration parser SHALL return a parse error without crashing
7. FOR ALL error conditions, THE Impressy_System SHALL log the error details
8. FOR ALL error conditions, THE Impressy_System SHALL remain in a valid state allowing continued operation


### [v1] Requirement 58: 性能不变性属性

**User Story:** 作为性能工程师,我希望验证性能关键操作满足时间约束。

#### Acceptance Criteria

1. FOR ALL cold start executions under normal system load, THE startup time SHALL NOT exceed 1500 milliseconds
2. FOR ALL batch processing operations with 100 images, THE UI_Layer response time to user clicks SHALL NOT exceed 100 milliseconds
3. FOR ALL real-time preview updates, THE preview refresh time SHALL NOT exceed 200 milliseconds
4. FOR ALL image file open operations with files up to 50 megapixels, THE display time SHALL NOT exceed 2000 milliseconds
5. WHEN batch processing 100 images, THE progress indicator update frequency SHALL be at least 1 update per second

### [v2] Requirement 59: AI 生成输出验证

**User Story:** 作为产品经理,我希望 AI 生成的输出符合业务约束和格式要求。

#### Acceptance Criteria

1. WHEN AI generation produces multiple images, THE output count SHALL match the user-requested count or be fewer due to content filtering
2. WHEN AI generation for ID photo completes, THE output image dimensions SHALL match the selected standard specification exactly
3. WHEN AI generation for promotional poster completes, THE output aspect ratio SHALL be 3:4 within 1% tolerance
4. WHEN platform size adaptation uses local cropping, THE output dimensions SHALL match the target platform specification exactly
5. WHEN AI avatar generation completes for N selected styles, THE output SHALL contain at most N images
6. FOR ALL AI generation requests, IF content policy violation occurs, THEN THE AI_Engine SHALL return zero images with a content blocked error


### [v1] Requirement 60: 安全属性验证

**User Story:** 作为安全工程师,我希望验证系统正确保护敏感数据。

#### Acceptance Criteria

1. 〔v2〕FOR ALL API_Key storage operations, THE Impressy_System SHALL use the Credential_Store
2. 〔v2〕FOR ALL API_Key storage operations, THE Impressy_System SHALL NOT write API_Key to plain text configuration files
3. 〔v2〕FOR ALL API_Key storage operations, THE Impressy_System SHALL NOT write API_Key to log files
4. 〔v2〕WHEN the configuration file is inspected, THE file SHALL NOT contain any API_Key values
5. WHEN EXIF cleaning completes, THE output file SHALL contain zero GPS-related EXIF tags
6. WHEN EXIF cleaning completes, THE output file SHALL contain zero device identification EXIF tags
7. 〔v2〕FOR ALL network requests to AI providers, THE AI_Engine SHALL use HTTPS protocol
8. 〔v2〕FOR ALL network requests to AI providers, THE AI_Engine SHALL validate TLS certificates

### [v1] Requirement 61: 当前文档、临时预览与编辑历史

**User Story:** 作为用户，我希望界面展示的图片就是实际处理的图片，并能放心尝试、应用和撤销编辑，以免得到与预期不一致的结果。

#### Acceptance Criteria

1. THE Current_Document SHALL retain an immutable original image and exactly one latest applied result
2. FOR EVERY single-image local operation, THE image shown as its input preview SHALL be the exact Current_Document version submitted to the Core_Engine
3. 〔v2〕FOR EVERY AI operation using a reference image, THE image shown as its input preview SHALL be the exact Current_Document version submitted to the selected Provider
4. THE UI_Layer SHALL use the latest applied Current_Document result as the default input for subsequent single-image tools
5. THE UI_Layer SHALL keep Input_Set state for batch processing, collage, and GIF separate from Current_Document preview state
6. WHEN crop, resize, or beautification parameters change, THE UI_Layer SHALL update a Transient_Preview without changing the latest applied result or edit history
7. WHEN the user applies a Transient_Preview, THE Current_Document SHALL replace its latest applied result and append exactly one edit-history entry
8. WHEN the user invokes a discrete rotation action, THE Current_Document SHALL apply it immediately and append exactly one edit-history entry
9. WHEN a slider or drag gesture produces multiple preview updates before one apply action, THE edit history SHALL contain no more than one entry for that apply action
10. THE UI_Layer SHALL support undo and redo of applied crop, resize, rotation, and beautification operations using platform-standard shortcuts and visible controls
11. FOR ANY sequence of applied edits, undoing every available entry SHALL restore the corresponding earlier Current_Document states and redoing them SHALL restore the same later states
12. THE edit history SHALL exist only for the open Current_Document session and SHALL NOT be serialized as AI generation history or application configuration
13. THE Impressy_System SHALL enforce a documented memory budget for edit history and SHALL expose the actually available undo range when older entries are evicted
14. WHEN beautification parameters are adjusted repeatedly before apply, every Transient_Preview SHALL use the Current_Document state captured when the tool was entered, so the same effect is not compounded by preview refreshes

---

## Document Metadata

- **Feature Name**: impressy
- **Spec Type**: Feature
- **Document Version**: 1.4
- **Total Requirements**: 61（v1: 36 / v2: 21 / removed: 4，见各条标题、ADR-0001、ADR-0003 与 ADR-0006）
- **Total Acceptance Criteria**: 517
- **Last Updated**: 2026-09-06
- **Language**: 简体中文 + English
- **Status**: 真相源（Source of Truth）；ADR-0006 UX 稳定化已实现并通过自动门禁，桌面、跨平台打包与真实 Provider 验收仍开放。范围与顺序决策见 docs/adr/。
