# Technical Design Document - Impressy 图像工具箱

> **本文档与 [`requirements.md`](./requirements.md) 是本项目的唯一真相源。** 领域词汇见 [`CONTEXT.md`](../../CONTEXT.md)，范围与顺序决策见 [`docs/adr/`](../adr/)。
> 早期的 `impressy-spec.md` 已冻结至 [`archive/impressy-spec-v0.3.md`](./archive/impressy-spec-v0.3.md)，**不要据此写代码**。
>
> **v1 / v2 范围**：v1 只做本地功能；v2 已按 [ADR-0004](../adr/0004-v2-provider-contract.md) 启动并建立 `impressy-ai` 与 `impressy-presets`。v2 不得削弱任何 v1 离线承诺。
>
> **截图能力已剥离**：`impressy-capture`、屏幕截图、全局热键、屏幕取色、覆盖层与截图标注不属于当前范围。截图美化仍是导入现有图片后的纯处理能力。见 [ADR-0003](../adr/0003-remove-screen-capture.md)。
>
> **建设顺序已经按 ADR-0002 执行。** 已冻结的 spec §8 曾主张 M1 骨架 → M2 core（UI 优先）；实际项目先完成 `impressy-core` 的可自动验证闭环，再接入 GPUI UI。当前不再按旧里程碑推进，而按「自动验证 → 桌面真机验收 → 发布」收敛。
>
> **UX 稳定化是当前 v2 发布门禁。** ADR-0006 统一了当前文档、临时预览、编辑历史、输入集合与导出语义；预览与实际处理输入不一致、静默覆盖旧文件、无法撤销和不完整导出流程必须在发布前修复。

## Overview

Impressy is a cross-platform desktop image processing application built with Rust and GPUI framework, targeting Windows, macOS, and Linux. The system focuses on lightweight local image processing and is organized around four product **板块**:

1. **Basic Image Processing**: Offline-first operations including editing, collage, batch processing, slicing, QR codes, EXIF management, and screenshot beautification
2. **AI Generation and Editing**: Text-to-image, image-to-image, and 12 preset AI editing scenarios
3. **Industry-Specific AI Tools**: 13 specialized tools including photo restoration, ID photos, avatars, meme generation, portrait photography, model try-on, product recoloring, promotional posters, platform adaptation, cover images, article illustrations, food photography, and interior design preview
4. **Creative Output**: GIF creation and poster design with local text rendering

### v1 Implementation Baseline Before v2 (2026-07-18)

The current candidate implementation baseline is `fffdb2c`, one local commit ahead of `origin/main@f88a939`. It includes the complete asynchronous native path-prompt change for image open, multi-image open, result save, output-directory selection, and image-watermark selection, plus direct regression coverage through the production `resolve_path_prompt` seam for cancellation, platform failure, channel closure, non-busy recovery, and immediate reuse. The local quality gate executes 60 tests and passes. Remote `f88a939` passed the Windows, macOS, and Linux quality jobs plus Windows MSI and Linux DEB smoke tests; because `fffdb2c` has not been pushed, exact-candidate CI evidence remains open. This is **not a release-ready declaration**:

| Layer | Baseline state | Verification boundary |
| --- | --- | --- |
| `impressy-core` | Implemented as pure module functions over `image::RgbaImage`; no UI, network, or global state | Fully headless-testable; local quality gate and property tests pass |
| `impressy-app` | Four-section GPUI shell, eight v1 pages, background jobs, config, i18n, drag/drop, clipboard, file export, and crop Element are wired | Type checks and headless state tests pass; Windows 11 release rendering, runtime locale switching, the native picker matrix, and all five performance metrics have partial real-machine evidence; the complete desktop matrix remains open |
| Packaging | Windows MSI, Linux DEB, and macOS DMG pipelines are defined; Windows/Linux package smoke tests exist in CI; the exact local candidate produced a 5.10MiB unsigned MSI whose administratively extracted EXE matched the release SHA-256 | Elevated Windows install/association/upgrade/uninstall, signing, notarization, and SmartScreen/Gatekeeper behavior remain release evidence |
| Acceptance evidence | Deterministic test-asset generator and JSON verifier exist; Windows 11 build 26200 partial evidence is recorded on issue #2 | `docs/testing/results/v1-desktop-acceptance.json` is not yet committed, Windows 10/macOS/Linux and package-security checks remain open, so v1 is not release-ready |

Status words used here have the meanings defined in `requirements.md` §“v1 当前验收状态”. A future change must not promote a UI item from “implemented” to “verified” using compilation alone.

The 2026-07-19 v2 implementation supersedes this source baseline: the workspace now contains four crates and all marked v2 image features are wired. Automated contract/state tests pass, while real-provider visual acceptance and a new exact-candidate desktop matrix remain open.

### Core Design Principles

- **Local-First Architecture**: All v1 本地功能 work offline; AI 功能 use BYOK (Bring Your Own Key)
- **Privacy by Design**: No telemetry or embedded API keys; credentials are stored in OS-native secure storage
- **Async-First Processing**: Heavy operations run in background executors to maintain UI responsiveness
- **Preview-Execution Identity**: The image shown as an operation's input is the exact version consumed by that operation
- **Recoverable Editing**: Parameter changes remain transient until applied; applied single-image edits participate in bounded session undo/redo
- **Safe Export**: Single-image export creates a copy by default, and multi-file output never overwrites silently
- **Provider Abstraction**: Unified AI provider interface supporting Seedream, Google Nano Banana, and OpenAI GPT-Image. Agnes is deferred — the trait reserves extensibility only.
- **Multi-Crate Workspace**: Four crates separate UI, pure local processing, provider networking, and locked prompts

### Technology Stack

- **Framework**: GPUI for native cross-platform UI
- **Language**: Rust with strict quality gates (cargo check, clippy deny, all tests pass)
- **Rendering**: DirectX 11 + DirectWrite (Windows), Metal (macOS), GPUI backend (Linux)
- **Image Processing**: Rust image processing libraries (offline)
- **Credential Storage**: Windows Credential Manager, macOS Keychain, Linux Secret Service
- **Configuration**: TOML format in standard app data directory
- **Build**: Cargo workspace with `impressy-app`, `impressy-core`, `impressy-ai`, and `impressy-presets`

## Architecture

### High-Level Architecture Diagram

```mermaid
graph TB
    subgraph "current workspace"
        A["GPUI AppShell<br/>navigation + pages + settings"]
        B["Workspace<br/>main-thread state"]
        C["WorkspaceJob<br/>Send + background execution"]
        D["impressy-core<br/>pure image modules"]
        E["ConfigStore<br/>TOML in platform config dir"]
        F["File system / clipboard / native dialogs"]
    end

    subgraph "v2 AI layer"
        G["impressy-ai<br/>Provider abstraction"]
        H["impressy-presets<br/>locked prompt templates"]
        I["OS credential store + AI providers"]
    end

    A --> B
    B --> C
    C --> D
    D --> C
    C --> B
    A --> E
    A --> F
    A --> G
    G --> I
    H --> A

    style D fill:#e1f5ff
    style G fill:#fff4e1,stroke-dasharray: 5 5
    style H fill:#f3e5f5,stroke-dasharray: 5 5
```

### Crate Organization

The current system is organized as a Cargo workspace with **four crates**:

#### 1. **impressy-app** (Main Application)
- GPUI application initialization, window identity, and four-section navigation
- `AppShell` orchestration, feature parameters, preview state, and settings
- `Workspace` command validation and `WorkspaceJob` / `WorkspaceOutcome` lifecycle
- Background execution for file I/O, decoding, encoding, and image processing
- Runtime Chinese / English switching through `rust-i18n` and `gpui_component::set_locale`
- Platform config persistence, drag/drop, clipboard, path prompts, and output-directory reveal
- Custom three-phase crop selection Element

#### 2. **impressy-core** (Image Processing Engine)
- Pure Rust module functions over `image::RgbaImage`
- Content-sniffed PNG/JPEG/WebP/GIF/BMP decoding and PNG/JPEG/WebP still-image encoding
- Transformations: crop, resize, rotate, collage, slice
- QR code generation and recognition
- EXIF parsing and cleaning
- Screenshot beautification effects
- GIF composition
- Watermarking
- Config serialization and deterministic output naming
- **No façade trait**: the public API is the set of typed module functions; async scheduling belongs to `impressy-app`

The following packages implement the v2 AI layer:

#### 3. **impressy-ai** (AI Service Abstraction)
- Provider trait abstraction
- Unified request/response models
- HTTPS-only Rustls transport with certificate validation
- Error mapping from provider-specific to app-level errors
- Capability declaration system
- Provider implementations: Seedream, Google Nano Banana, OpenAI GPT-Image（Agnes 暂不实现）
- **Key Trait**: `Provider` with capability declarations

#### 4. **impressy-presets** (Industry Tool Templates)
- Locked prompt template storage
- Compile-time embedding of prompts into binary
- Template organization by industry tool category
- Version-controlled prompt iteration
- **Key API**: typed edit/industry prompt renderers backed by separate compile-time template files

### Concurrency Model

```mermaid
sequenceDiagram
    participant U as UI Thread
    participant M as AppShell / Workspace
    participant B as Background Executor
    participant C as Core Engine

    U->>M: User action
    M->>M: Validate and build WorkspaceJob
    M->>B: Spawn Send task
    B->>C: Decode / transform / encode / export
    C-->>B: Result or CoreError
    B-->>M: Progress and WorkspaceOutcome channel
    M->>M: Apply state on main thread
    M-->>U: Notify and re-render
    U->>U: Remains responsive
```

### Data Flow Patterns

#### Pattern 1: Offline Image Processing
```
User Input → AppShell → Workspace::prepare → WorkspaceJob → Background Executor
                                                           ↓
UI refresh ← Workspace::apply ← WorkspaceOutcome / progress ← impressy-core + file system
```

#### Pattern 2: Native Path Prompt
```
GPUI listener → request asynchronous platform prompt → listener returns and entity borrow ends
             → await selected paths → update AppShell → prepare WorkspaceJob
```

On Windows, a synchronous native dialog can start a nested message loop and re-enter GPUI while the entity is already mutably borrowed. Therefore all file and directory pickers MUST either use GPUI's asynchronous prompt API or be scheduled only after the listener borrow has been released. This is an architectural constraint, not an optional implementation detail.

#### Pattern 3: Current Document Editing

```
CurrentDocument.applied(version N) → tool parameters → TransientPreview(base N)
                                      ↓ Apply
EditHistory append ← CurrentDocument.applied(version N+1) ← background result(base N)
```

Every background preview or apply job carries its base document version. A result is accepted only when that version still matches the state that requested it; stale async results are discarded. Crop, resize, and beautification previews never become inputs to another tool until the user applies them. Rotation is a discrete action and commits immediately.

#### Pattern 4: Multi-Image Batch Export

```
InputSet → ordered [Resize?, Watermark?] → OutputSettings → CollisionPolicy → files
                                                                  ↓
                                         per-item result + retryable failures
```

Format and compression/quality are output settings, not batch steps. Processing steps run in displayed order for every input. The selected item preview uses the same full pipeline. Cancellation is observed between items so already completed files remain valid.

#### Pattern 5: AI Reference Request

```
CurrentDocument.applied(version N) → preview + optional region → inline Provider disclosure
                                                                  ↓ Generate
                                         immutable snapshot(version N) → Provider
```

The request snapshot, displayed reference, and optional selection coordinates share the same version and geometry. Clicking Generate while the inline disclosure is visible is the explicit user initiation required by the privacy contract. Text-to-image requests contain no reference image.

## Components and Interfaces

### Core Engine Contract (`impressy-core`)

The core contract is a typed module API over `image::RgbaImage`. There is no `ImageProcessor` façade, no async runtime, and no file-system-owning service object. Callers select a module function and pass explicit values; errors use `impressy_core::Result<T>` / `CoreError`.

| Module | Public responsibility | Principal types / functions |
| --- | --- | --- |
| `format` | Content-sniffed PNG/JPEG/WebP/GIF/BMP decode; PNG/JPEG/WebP encode | `decode`, `encode`, `OutputFormat`, `EncodeSettings`, `Quality`, `PngCompression` |
| `transform` | Exact-ratio crop, resize, fit, lossless right-angle rotation | `AspectRatio`, `CropRect`, `ResizeFilter`, `Rotation` |
| `collage` | Vertical, horizontal, and grid composition | `CollageLayout`, `CollageOptions`, `compose` |
| `slice` | Exact row × column slicing with remainder pixels preserved | `SliceGrid`, `Tile`, `slice` |
| `batch` | Per-item transform with count/order preservation and isolated failures | `BatchOp`, `BatchResult`, `process` |
| `watermark` | Image-overlay watermark at bottom-right or tiled | `Position`, `apply` |
| `beautify` | Rounded corners, padding, background, border, and shadow | `BeautifyParams`, `Background`, `Border`, `Shadow`, `beautify` |
| `animation` | Forward, reverse, and ping-pong GIF encoding | `GifParams`, `Playback`, `compose` |
| `qr` | QR generation and recognition | `QrOptions`, `ErrorCorrection`, `generate`, `decode` |
| `exif` | Human-readable EXIF view and container-level metadata stripping | `ExifData`, `GpsCoordinates`, `read`, `strip`, `has_exif` |
| `config` | v1 TOML data model and round-trip serialization | `AppConfig`, `Language`, `to_toml`, `from_toml` |
| `naming` | Sanitized deterministic batch and tile filenames | `sanitize_stem`, `batch_filename`, `tile_filename` |

`impressy-core` MUST remain synchronous and pure. Requirement 3 async behavior is fulfilled by the application layer scheduling these functions on GPUI's background executor; adding an async executor or UI text to the core would violate the crate boundary.

### Application Contract (`impressy-app`)

| Component | Responsibility | Threading rule |
| --- | --- | --- |
| `AppShell` | Own active section/page, settings, controls, tool selection, path prompts, clipboard, and locale changes | GPUI main thread only |
| `Workspace` | Orchestrate an optional `CurrentDocument`, independent multi-image `InputSet`, task status, and background jobs | Mutated on main thread only |
| `CurrentDocument` | Own immutable original, latest applied result, transient preview, version, dirty/export state, and bounded edit history | Mutated on main thread; image snapshots may be shared immutably with jobs |
| `InputSet` | Own ordered multi-image inputs, focus, pipeline preview identity, and per-item results for batch/collage/GIF workflows | Mutated on main thread; immutable snapshots sent to jobs |
| `EditHistory` | Record applied single-image operations and bounded checkpoints for session undo/redo | Main thread metadata; checkpoint pixels are immutable shared values |
| `ExportPlan` | Bind output format, dimensions, format-specific settings, destination, and collision policy before file I/O | Constructed on main thread, executed in background |
| `WorkspaceCommand` | Express user intent such as transform, collage, slice, QR, EXIF, GIF, batch, export, or open | Constructed on main thread |
| `WorkspaceJob` | Own all data needed for file I/O, decode, transform, encode, and export | Must be safe to move to the background executor |
| `WorkspaceOutcome` | Return result data, errors, clipboard payload, and output-directory effects | Applied to `Workspace` on main thread |
| `Selection` / crop Element | Store normalized crop rectangle; implement GPUI `request_layout`, `prepaint`, and `paint`; support move and eight resize grips | Events and rendering on main thread |
| `FeatureParams` | Hold bounded page parameters and translate them to core types | Main thread; pure conversion helpers are unit-tested |
| `ConfigStore` | Resolve the platform config directory; atomically load/save TOML with corruption fallback | Short config operations may run during app lifecycle; image work never routes through it |
| `UiMessage` / `ErrorKind` | Map internal outcomes to localized user-facing state | User text is resolved through i18n keys |

The job lifecycle is:

1. A listener converts UI input into a `WorkspaceCommand`.
2. `Workspace::prepare` validates state and produces a self-contained `WorkspaceJob`.
3. `AppShell` runs the job on `cx.background_executor()` and receives progress / completion through an async channel.
4. `Workspace::apply` validates the document/input-set version carried by the outcome. It applies a matching result or discards a stale result, then asks GPUI to re-render.

Native path prompts are outside `WorkspaceJob`: selection happens asynchronously, after the listener's mutable entity borrow is released. File reads and image decoding begin only after concrete paths have been returned.

### v2 Provider and Preset Interfaces

The implemented boundary is:

- a `Provider` abstraction with capability declarations, text-to-image, image-to-image, edit, and normalized errors;
- provider credentials stored only in OS credential stores;
- locked prompt templates compiled into `impressy-presets` and selected by industry-tool **档位**;
- `impressy-core` remains independent of both network crates; only `impressy-app` orchestrates them.

Provider request schemas and models are frozen by ADR-0004 and contract-tested through injected fake transports. A future model or endpoint upgrade is an explicit research/ADR task.

## Data Models

### Image and Encoding Model

- Internal decoded pixels use `image::RgbaImage` (`u8` RGBA).
- `OutputFormat` contains `Png`, `Jpeg`, and `Webp`; GIF is produced by `animation::compose`, while BMP/GIF remain readable inputs.
- `EncodeSettings` binds legal settings to the target format: PNG compression, JPEG quality, lossy WebP quality, or lossless WebP.
- `Quality` is validated in the inclusive range 1–100 and passed to the encoder without remapping.
- JPEG export flattens alpha onto opaque white; PNG and lossless WebP preserve alpha.

### Geometry and Effect Model

- `AspectRatio` stores a reduced integer pair. The five v1 presets are 1:1, 4:5, 9:16, 16:9, and 21:9.
- `CropRect` is a checked pixel-space rectangle; the crop Element stores a normalized rectangle and converts it against current image dimensions.
- `SliceGrid` and `CollageOptions` reject zero or invalid dimensions instead of silently changing user input.
- Beautification and watermark parameters are explicit value types; all effects return a new image and do not mutate the source.

### Workspace State Model

`Workspace` separates single-image state from multi-image state. It may own one `CurrentDocument` and one independent `InputSet`; switching pages never makes one silently impersonate the other.

`CurrentDocument` contains:

- immutable original decoded pixels and original container bytes where available;
- the latest applied image result and a monotonically increasing document version;
- an exported-state marker on the current state and every retained history checkpoint, independent of the monotonic job-generation `version`, so Undo/Redo restores the exact dirty/export state without reusing old versions;
- at most one `TransientPreview` tagged with the base document version and active tool;
- `EditHistory`, with applied operations, undo/redo position, and optional immutable pixel checkpoints.

The edit-history checkpoint cache has a 256 MiB soft budget; original and current pixels do not count against it. Operation records are retained for the open session, while the oldest pixel checkpoints are evicted first. The UI exposes only the range that can actually be restored without guessing or silently recomputing an unavailable state. Closing the document destroys this history.

`InputSet` contains ordered decoded inputs, source names, selected/focused item, and a monotonically increasing set version. Batch, collage, and GIF jobs consume immutable snapshots of this state. Batch output retains input index, success/failure partition, original source stem, final dimensions, encoded size, and collision outcome.

Every preview, processing, AI, and export job carries the relevant document or input-set version. An outcome for an older version cannot replace a newer preview, current result, selection, or task status.

### Batch Pipeline and Export Model

`BatchPipeline` is an ordered list whose initial supported step types are `Resize` and `Watermark`. Resize includes proportional dimensions, percentage or longest-side modes, and an optional do-not-enlarge rule. Watermark retains text/image source, opacity, placement, and spacing. Output format plus PNG compression or JPEG/WebP quality belong to `ExportPlan`, after all steps.

`CollisionPolicy` contains `PreserveBoth` (default), `Skip`, and `Replace`. Preserve-both resolves the complete destination set before writing and adds stable suffixes until every path is unique against both existing files and other items in the task. Replace is legal only after an explicit user choice.

Each output is encoded to a uniquely named temporary file in the destination directory, flushed, closed, and decoded or structurally validated before publication. New destinations use an atomic rename where available. Replace uses the platform's atomic replacement primitive where available; if the platform cannot provide it, the app must preserve the previous destination or report that safe replacement is unavailable. Temporary artifacts are never reported as successful outputs.

Single-image `ExportPlan` presents the current document preview, format, dimensions, format-specific settings, estimated file size, filename, and destination in one flow. One-time choices do not mutate defaults unless the user explicitly saves them as defaults. Export defaults remain non-secret configuration.

### Configuration Model

`AppConfig` contains only:

- `default_export_format`;
- `default_export_quality`;
- `default_png_compression`;
- `language` (`zh-CN` or `en` at runtime);
- `last_output_dir: Option<PathBuf>`.
- `default_provider`;
- non-secret local `generation_history` records.

It intentionally contains no API key or prompt text. `ConfigStore` writes through a temporary file and backup/rename sequence, and a missing or corrupt config falls back to defaults with a localized warning.

### History and Credentials

Current-document `EditHistory` is bounded session state and is never serialized. Generation history is separate non-secret TOML data and can be cleared in Settings. Provider credentials use `keyring` and are stored only by Windows Credential Manager, macOS Keychain, or Linux Secret Service; secret wrappers are redacted, zeroized on drop, and never serialized or logged.

## Error Handling

### Error Layers

`impressy-core` exposes one structured `CoreError` with variants for image decode/encode, WebP, disk space, I/O, QR encode/decode, EXIF, GIF encode, config parse/serialize, and invalid arguments. Public core operations return `Result` and do not panic for user input.

`impressy-app` does not expose a speculative application-wide error enum. Background jobs convert concrete failures into semantic `ErrorKind` categories:

- unsupported format or corrupted image;
- encode or image operation failure;
- EXIF or QR failure;
- disk full, permission, or general I/O;
- config or clipboard failure.

`UiMessage` stores semantic state and data, never translated strings. Rendering resolves the i18n key at display time, so an existing error changes language immediately when the locale changes.

### Recovery Rules

1. An individual batch item failure is recorded with its source name and does not stop remaining items.
2. A failed `WorkspaceJob` clears the busy state and leaves the workspace usable for the next command.
3. Missing config silently uses defaults; unreadable or invalid config uses defaults and displays a reset warning.
4. Export failures preserve the in-memory result and dirty state so the user can choose another destination.
5. Invalid numeric or empty text input is rejected before a background job is spawned.
6. AI/provider/credential failures map into localized semantic categories without exposing provider bodies or credentials.
7. Stale preview, processing, or AI outcomes are discarded by version and cannot overwrite a newer current document, input set, selection, or task status.
8. A failed apply action preserves the prior applied result and its edit-history position.
9. Cancelling a batch stops before the next item; completed outputs remain valid and unstarted items are reported as cancelled rather than failed.
10. A multi-file name collision follows the chosen collision policy; no existing file is replaced under the default preserve-both policy.
11. Closing or replacing a current document with unexported applied changes requires export, discard, or cancel; no default choice destroys the changes.

## Testing Strategy

### Automated Baseline

The 2026-07-19 workspace baseline executes 80 tests: `impressy-ai` provider/security/transport contracts, `impressy-presets` embedded-template coverage, the complete v1 core property suite, and app state/orchestration tests including AI masks, exact business dimensions, config secret exclusion, platform local cropping, and poster composition.

That baseline predates ADR-0006. The 2026-09-06 implementation adds automated coverage for document/input-set version identity, cross-document instance identity, transient-preview commit rules, undo/redo round trips and memory budgeting, proportional resizing, batch pipeline ordering, collision resolution, temporary-file publication, AI reference snapshot identity, stale-outcome rejection, cross-tool preview decisions, and stale AI task settlement. The workspace now passes 132 tests, including property tests for arbitrary edit histories and per-input relative batch resizing plus explicit pipeline-order verification, along with `cargo check` and zero-warning clippy; desktop, packaging, cross-platform, and real-provider acceptance remain open.

Property-based testing is required for pure transformations and invariants. Example-based unit tests are preferred for state transitions, platform-independent orchestration, validation, and known regressions. UI pixels, interaction feel, native dialogs, clipboard interoperability, signing, and timing are desktop acceptance concerns.

### CI and Package Verification

Normal CI performs:

- icon-source and generated-asset consistency checks;
- `cargo check`, Clippy with denied warnings, and tests on Windows, macOS, and Linux;
- unsigned Windows MSI build plus installation, association, shortcut, and uninstall smoke;
- Linux amd64 DEB build plus content, install, and uninstall smoke.

The release workflow additionally builds signed Windows artifacts, a signed/notarized macOS DMG, and the Linux package, enforces the 30 MiB limit, and refuses to start without current desktop acceptance evidence.

### Desktop Acceptance

`docs/testing/v1-desktop-acceptance.md` is the normative procedure for work that headless tests cannot prove. The committed result must include:

- Windows, macOS, Linux X11, and Linux Wayland environment details and pass results;
- the complete common-feature and platform-specific check sets;
- at least three samples for every performance metric;
- deterministic generated-resource and EXIF-photo SHA-256 values;
- package size, signing, notarization, install, uninstall, and platform security conclusions.

`scripts/verify_desktop_acceptance.py` rejects missing, stale, incomplete, or out-of-budget evidence. Windows 11 build 26200 has partial release-build evidence for DirectWrite Chinese/English rendering, the complete native picker path matrix, cold start, 50MP preview, beautify preview, and 100-image responsiveness/progress. The exact-candidate unsigned MSI also builds within budget and passes administrative extraction with a byte-identical release EXE; a non-elevated per-machine install was intentionally stopped by Windows Error 1925 without leaving installed state. The baseline still has only an example JSON, and Windows 10, macOS, Linux, elevated package integration, and signed package checks remain open, so desktop acceptance is not complete.

### Quality Gates

Every task must pass the repository Definition of Done:

1. `cargo check`
2. `cargo clippy -- -D warnings`
3. `cargo test`

CI may add `--workspace` / `--all-targets`, but v1 MUST NOT enable `webview` or `inspector` features because they introduce a browser engine and violate the dependency boundary.

## Correctness Properties

*A property is a characteristic or behavior that should hold true across all valid executions of a system—essentially, a formal statement about what the system should do. Properties serve as the bridge between human-readable specifications and machine-verifiable correctness guarantees.*

### Property Reflection

After analyzing the 517 acceptance criteria, I identified properties suitable for property-based testing in the core engine and deterministic application state. The following reflection eliminates redundancy:

**Redundancy Analysis:**
1. **EXIF Cleaning Properties**: Requirements 2.7-2.8, 11.7-11.10, and 60.5-60.6 all specify EXIF GPS/device removal. These can be combined into a single comprehensive property.
2. **Round-Trip Properties**: Requirements 10 (QR codes), 38 (lossless formats), 52 (explicit round-trip), and 53 (config) all specify encode-decode consistency. Each is distinct (QR vs PNG vs config) so all are retained.
3. **Batch Invariants**: Requirements 8 and 54 both specify batch processing count/order invariants. These are overlapping and can be combined.
4. **Removed Capture Coordinates**: Requirements 13 and 48 were removed by ADR-0003, so no DPI capture property remains in v1.
5. **Idempotence**: Requirement 56 lists multiple idempotent operations (EXIF clean, PNG-to-PNG, config reload). While conceptually similar, each tests different subsystems, so all are retained.

### Property 1: Lossless Image Format Round-Trip Preserves Pixels

*For any* valid image with pixel data, encoding in lossless PNG format and then decoding SHALL produce pixel data identical to the original input.

*For any* valid image with pixel data, encoding in lossless WebP format and then decoding SHALL produce pixel data identical to the original input.

**Validates: Requirements 38.7, 38.8, 52.7, 52.8**

### Property 2: QR Code Round-Trip Preserves Content

*For any* text string, encoding as a QR code in PNG format and then parsing the generated QR code image SHALL decode to the original text string exactly.

*For any* URL string, the round-trip operation (generate QR code → save as PNG → parse) SHALL preserve the original URL exactly.

**Validates: Requirements 10.9, 10.10, 52.11, 52.12**

### Property 3: Configuration Serialization Round-Trip Preserves Data

*For any* valid AppConfig object, serializing to TOML format and then parsing back SHALL produce an equivalent configuration object.

**Validates: Requirements 32.1, 53.3**

### Property 4: EXIF Cleaning Removes GPS and Device Metadata

*For any* image with EXIF metadata, after EXIF cleaning, the output image SHALL contain zero GPS-related EXIF tags.

*For any* image with EXIF metadata, after EXIF cleaning, the output image SHALL contain zero device identification EXIF tags.

*For any* image, EXIF cleaning SHALL preserve the pixel data exactly while removing only metadata.

**Validates: Requirements 2.7, 2.8, 11.7, 11.8, 55.5, 60.5, 60.6**

### Property 5: Image Slicing Produces Correct Tile Count

*For any* image and grid configuration with M rows and N columns, slicing SHALL produce exactly M × N output tiles.

**Validates: Requirements 9.5, 9.6, 55.4**

### Property 6: Collage Composition Contains All Input Images

*For any* list of N images and any collage layout mode (vertical, horizontal, grid), the output collage SHALL contain exactly N visible image regions.

**Validates: Requirements 7.8**

### Property 7: Grid Collage Column Count Matches Configuration

*For any* list of N images composed in grid mode with C columns, the resulting grid SHALL have ⌈N/C⌉ rows and the specified C columns (except possibly the last row).

**Validates: Requirements 7.5**

### Property 8: Batch Processing Preserves Input Count

*For any* batch of N images processed with any batch operation, the sum of successful outputs and failed items SHALL equal N.

**Validates: Requirements 8.7, 8.8, 54.6**

### Property 9: Batch Processing Preserves Input Order

*For any* ordered list of images in a batch operation, the output files SHALL maintain the same order as the input list.

**Validates: Requirements 54.5**

### Property 10: Batch Format Conversion Preserves Dimensions

*For any* batch of images undergoing format conversion, each output image SHALL have dimensions identical to its corresponding input image.

**Validates: Requirements 54.1**

### Property 11: Batch Compression Preserves Dimensions

*For any* batch of images undergoing compression, each output image SHALL have dimensions identical to its corresponding input image.

**Validates: Requirements 54.2**

### Property 12: Batch Resize Preserves Image Count

*For any* batch of N images undergoing resize with a scale factor, the output SHALL contain exactly N images.

**Validates: Requirements 54.3**

### Property 13: Batch Watermark Preserves Image Count

*For any* batch of N images undergoing watermark application, the output SHALL contain exactly N images.

**Validates: Requirements 54.4**

### Property 14: Cropped Image Matches Selected Aspect Ratio

*For any* image cropped with a selected aspect ratio preset, the output image aspect ratio SHALL match the preset within 0.1% tolerance.

**Validates: Requirements 55.1**

### Property 15: 360-Degree Rotation Is Identity

*For any* image rotated by 360 degrees, the output image SHALL be visually identical to the input image.

**Validates: Requirements 55.2**

### Property 16: Removed with Screen Capture

The DPI/capture property is no longer part of the current scope (ADR-0003).

### Property 17: GIF Frame Count Matches Input

*For any* non-empty list of N images composed into a GIF, forward and reverse playback SHALL contain exactly N frames; ping-pong playback SHALL contain one frame when N=1 and 2N−1 frames when N>1.

**Validates: Requirements 15.6**

### Property 18: Beautification with Transparent Background Produces PNG with Transparency

*For any* image beautified with transparent background selected, the output SHALL be a PNG file with true transparency outside the content area (e.g., outside rounded corners).

**Validates: Requirements 12.10**

### Property 19: Batch Filename Generation Is Sequential and Unique

*For any* v1 batch or slicing operation producing multiple output files from N inputs, the filenames SHALL contain sanitized source stems plus sequential identifiers, and slicing names SHALL additionally contain grid positions; every generated path SHALL remain inside the selected output directory.

**Validates: Requirements 41.3, 41.4**

### Property 20: EXIF Cleaning Is Idempotent

*For any* image, applying EXIF cleaning twice SHALL produce output identical to applying it once.

**Validates: Requirements 56.1**

### Property 21: PNG-to-PNG Conversion Is Idempotent

*For any* PNG image, converting from PNG to PNG format SHALL produce output equivalent to the input.

**Validates: Requirements 56.2**

### Property 22: QR Code Re-Encoding Is Idempotent

*For any* text string, encoding as QR code, decoding, and re-encoding SHALL produce a QR code that decodes to the same text.

**Validates: Requirements 56.3**

### Property 23: Configuration Save-Reload Is Idempotent

*For any* configuration, saving to file and immediately reloading SHALL produce a configuration equal to the original.

**Validates: Requirements 56.4**

### Property 24: Invalid Image File Returns Error Without Crash

*For any* invalid or corrupted image file, attempting to open SHALL return an error without crashing the application.

**Validates: Requirements 57.1**

### Property 25: Corrupted QR Code Returns Error Without Crash

*For any* corrupted QR code image, attempting to parse SHALL return a decoding error without crashing the application.

**Validates: Requirements 57.2**

### Property 26: Invalid TOML Returns Error Without Crash

*For any* invalid TOML syntax in a configuration file, attempting to parse SHALL return a parse error without crashing the application.

**Validates: Requirements 57.6**

### Property 27: Error Conditions Maintain Valid Application State

*For any* v1 error condition (file I/O, parsing, encoding, decoding, or image-operation errors), the application SHALL remain in a valid state allowing continued operation after error handling.

**Validates: Requirements 57.7, 57.8**

### Property 28: Screenshot Beautification Padding Increases Dimensions Predictably

*For any* image beautified with inner padding P, the output width and height SHALL each increase by exactly 2P pixels. Corner radius changes the alpha mask, not the outer dimensions.

**Validates: Requirements 55.6**

### [v2] Property 29: Credential Storage Never Writes API Keys to Plaintext Files

*For any* API key storage operation, the Impressy_System SHALL NOT write the API key to the configuration file or log files.

**Validates: Requirements 60.2, 60.3, 60.4**

### [v2] Property 30: Network Requests to AI Providers Use HTTPS

*For all* network requests to AI providers, the AI_Engine SHALL use HTTPS protocol.

**Validates: Requirements 60.7**

### Property 31: Preview and Execution Use the Same Version

*For any* current-document or input-set version V, an accepted preview or processing outcome SHALL carry V and SHALL be rejected if the owning state has advanced to any version other than V.

**Validates: Requirements 5.9, 5.10, 8.14, 61.2, 61.3**

### Property 32: Undo and Redo Restore Applied States

*For any* sequence of applied operations whose history entries remain available, undoing N entries and redoing the same N entries SHALL restore the same pixels, dimensions, document version position, and dirty/export state as before the undo sequence.

**Validates: Requirements 61.7–61.13**

### Property 33: Preserve-Both Collision Resolution Is Unique and Non-Destructive

*For any* set of candidate output names and existing destination names, preserve-both resolution SHALL produce paths that are unique within the task, distinct from every existing path under platform filename comparison rules, and contained by the selected output directory.

**Validates: Requirements 41.10, 41.11, 41.14**

### Property 34: Batch Pipeline Preserves Displayed Step Order

*For any* valid ordered list of resize and watermark steps, every successful batch item SHALL be processed by each enabled step exactly once in displayed order before output encoding.

**Validates: Requirements 8.11, 8.12, 8.13**

## Implementation Approach

The original sprint list described a greenfield build. The codebase has moved past that stage, so the execution plan is now organized by verification state rather than by hypothetical implementation order.

### [v1] Stage A — Core Engine: Implemented and Automatically Verified

Complete scope:

- workspace and two-crate boundary;
- PNG/JPEG/WebP encoding and PNG/JPEG/WebP/GIF/BMP decoding;
- crop, resize, rotation, collage, slice, QR, EXIF, beautification, GIF, watermark, batch, config, and naming;
- v1 correctness properties and regression tests;
- cross-platform CI quality jobs.

Exit criterion: `impressy-core` remains pure, synchronous, network-free, and green under all three quality gates on Windows, macOS, and Linux.

### [v1] Stage B — Desktop Application: Implemented, Stabilization and Real-Machine Acceptance Open

Implemented scope:

- four **板块** and v2 placeholders;
- eight v1 pages connected to the core;
- runtime Chinese / English switching;
- background image jobs with progress and reusable error state;
- config persistence and recovery;
- file open, drag/drop, clipboard paste/copy, export, and output-directory reveal;
- all native open/save/directory flows use GPUI asynchronous path prompts, with selected paths passed to `Workspace` before file I/O or codec work begins;
- crop selection custom Element with normalized geometry;
- deterministic acceptance assets and result verifier.

Remaining stabilization work:

1. Repeat the now-passing Windows 11 picker regression matrix on Windows 10; the Windows 11 release build covered open, multi-open, save, output-directory, and image-watermark flows without `RefCell already borrowed`.
2. Run the complete feature matrix on Windows 10/11 x64, macOS, Linux X11, and Linux Wayland. The current Windows 11 evidence is partial and does not replace the full matrix.
3. Record any rendering, focus, DPI, clipboard, dialog, drag/drop, or file-manager defects as implementation work; compilation and partial Windows evidence are not proof for untested behaviors.

Exit criterion: all common and platform-specific checks in `v1-desktop-acceptance.md` pass and are represented in the committed JSON evidence.

### [v1] Stage C — Performance and Release: Open

1. Generate the deterministic acceptance assets and record their SHA-256. Current Impressy reference: `3a436a64acf820756e65739709978cc8f7e74a0cc3be7924bcde8e9478e9050f` (Rastery-era `b3f14709…` is obsolete after the acceptance QR URL rename).
2. Measure at least three release-build samples for cold start, 50MP preview, beautify preview latency, 100-image click response, and progress frequency on the required platforms. Windows 11 is complete and within budget; Windows 10, macOS, and Linux remain open.
3. Optimize any metric outside the requirement budget, then repeat the affected platform evidence.
4. Build the Windows MSI, macOS DMG, and Linux DEB; verify size, install/uninstall, associations, shortcuts, runtime dependencies, signing, and notarization. The exact-candidate Windows MSI build, size, and extracted payload integrity pass; elevated install integration and signing remain open.
5. Commit `docs/testing/results/v1-desktop-acceptance.json` for the exact release-affecting source baseline and run its verifier.
6. Only after all gates pass may a `v*` tag create a release.

Exit criterion: the release workflow accepts the evidence and all platform artifacts pass their jobs. Until then the product status remains pre-release even if every automated test is green.

### [v2] Stage D — AI 功能: Implemented, Real-Provider Acceptance Open

ADR-0004 and fresh official-provider research established the contract. The four-crate implementation includes secure credentials, three providers, capability-driven UI, 12 edit presets, 13 industry tools, poster composition, exact output post-processing, history, and semantic failures.

Exit criterion: execute [`v2-ai-acceptance.md`](../testing/v2-ai-acceptance.md) with funded real-provider accounts, verify every **保持不变项** and content-filter/error path, then repeat the exact-candidate desktop and package matrix. Automated fake-transport tests cannot promote subjective generated output to “verified”.

v2 MUST NOT weaken the v1 guarantees: every 本地功能 remains usable without network connectivity, API keys, provider initialization, or telemetry.

### [v2 release gate] Stage E — Unified Document and Workflow Stabilization: Implemented, acceptance open

ADR-0006 and Requirements 5, 6, 8, 15, 34, 36, 37, 39, 41, 42, 47, and 61 define the required stabilization. Implement in dependency order:

1. Introduce `CurrentDocument` and `InputSet` version identity; make preview, local processing, AI snapshots, and status updates reject stale outcomes.
2. Add transient preview, explicit apply, bounded session undo/redo, original restore, dirty/export state, and close/replace guards.
3. Correct crop/resize semantics with source dimensions, aspect-ratio lock, percentage/longest-side modes, and non-enlarge behavior.
4. Build the unified export flow and safe multi-file publication with preserve-both as default.
5. Replace mutually exclusive batch modes with the ordered resize/watermark pipeline, result list, cancellation, failed-item retry, and parameter-only presets.
6. Recompose the workspace so the canvas remains primary, active parameters scroll independently, and the current primary action remains visible.
7. Bind AI disclosure, reference preview, region selection, and request payload to one immutable current-document snapshot; add result comparison and handoff to local processing.
8. Add real GIF playback preview and keyboard/accessibility paths for the core workflow.

Exit criterion: automated properties and app-state tests for Stage E pass; the UX task scripts below pass in Chinese and English on the Windows desktop candidate; required cross-platform CI/package evidence is refreshed for the same source commit; real-provider tests continue to satisfy every **保持不变项**.

Normative desktop task scripts:

- Open a 1920×1080 image, resize it to width 960 with proportional height 540, and export JPEG without opening Settings.
- Apply rotation, crop, and beautification; undo two operations, redo them, and prove preview, current result, and exported pixels agree.
- Process 100 images through longest-side resize, watermark, and JPEG output; cancel between items, preserve completed files, and retry only failures.
- Repeat slicing and batch export into the same directory without changing or destroying outputs from the first run under the default policy.
- Use a rotated current document for AI region editing and prove through fake transport that displayed reference pixels, transmitted pixels, and mask coordinates agree before any funded-provider run.
- Complete open, proportional resize, undo, redo, and export by keyboard with visible focus; inspect the same controls with the platform screen reader.

Navigation favorites/recent items, task-synonym search, default collapsing of long AI groups, and purely visual styling refinements are follow-up enhancements. They may proceed after Stage E and do not block its exit criterion.

## Maintenance and Evolution

### Dependency and Version Strategy

- Keep `gpui = "=0.2.2"` and `gpui-component = "=0.5.1"` exactly locked.
- Treat a synchronized GPUI / gpui-component upgrade as its own change with regenerated `vendor-docs/`, full CI, and repeated desktop acceptance.
- Do not enable `webview` or `inspector` features.
- Keep the local `proc-macro-error2` compatibility patch documented and remove it only when a verified synchronized upgrade makes it unnecessary.
- Use `PathBuf` for platform paths and preserve the four-crate dependency boundary.

### Code Quality Standards

- All tasks pass `cargo check`, `cargo clippy -- -D warnings`, and `cargo test`.
- CI runs the workspace and all targets on Windows, macOS, and Linux.
- `impressy-core` public operations return structured errors for invalid user input and contain no user-visible localized text.
- Property tests run at least 100 cases, keep shrinking enabled, and preserve generated regression cases for deterministic replay.
- Every property test identifies the corresponding property number in a nearby comment.
- Existing user changes in a dirty worktree are never overwritten as part of unrelated work.

### Property-Based Testing Configuration

- **Library**: `proptest` (currently 1.11.x).
- **Scope**: pure core transformations, serialization, naming, and state invariants that can be expressed without a desktop.
- **Cases**: at least 100 per property; the library default may be higher.
- **Shrinking**: enabled.
- **Replay**: committed `*.proptest-regressions` cases and explicit seeds when diagnosing CI-only failures.

The 2026-07-19 baseline implements Properties 1–15 and 17–30. Property 16 was removed with screen capture; Properties 29–30 are covered by credential/config and HTTPS transport tests. The 2026-09-06 implementation adds passing coverage for Properties 31–34. Desktop and cross-platform evidence is tracked separately from these automated properties.

### Verification Layers

| Layer | Purpose | Current mechanism |
| --- | --- | --- |
| Pure behavior | Pixel, count, order, round-trip, idempotence, and error invariants | `impressy-core` unit and property tests |
| Headless app state | Document/input-set versions, transient apply, undo/redo, pipeline ordering, collision resolution, parameters, config, job outcomes, and crop math | `impressy-app` unit and property tests |
| Cross-platform build | API use, cfg paths, linking, package structure | GitHub Actions on three operating systems |
| Desktop behavior | Rendering, interaction, dialogs, clipboard, drag/drop, file manager, performance | Four-environment desktop acceptance JSON |
| Release trust | Signing, notarization, installation, uninstall, artifact size | release workflow plus real-machine checks |

### Future Extensions (Out of Current Scope)

- the three video placeholders;
- optional screen capture only after a new ADR re-evaluates package size and platform dependencies;
- AI providers beyond the three initially planned providers;
- cloud sync or a plugin system.
- navigation favorites and recently used tools;
- task-synonym search and default collapsing of long AI navigation groups;
- purely visual styling refinements that do not change workflow semantics.

---

## Document Metadata

- **Feature Name**: impressy
- **Spec Type**: Feature
- **Workflow Type**: Requirements-First
- **Document Version**: 1.4
- **Total Requirements Covered**: 61
- **Total Acceptance Criteria Addressed**: 517
- **Correctness Properties Defined**: 33 active (31 v1/cross-cutting + 2 v2), plus removed Property 16 placeholder
- **Automated Baseline**: 132 passing workspace tests on the 2026-09-06 local v2 implementation, including ADR-0006 Stage E and property coverage; `cargo check` and zero-warning clippy pass
- **Last Updated**: 2026-09-06
- **Status**: v1/v2 image features and Stage E UX stabilization implemented; real-provider, desktop, package, and release acceptance open
