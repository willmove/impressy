# Impressy

[![CI](https://github.com/willmove/impressy/actions/workflows/ci.yml/badge.svg)](https://github.com/willmove/impressy/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](#license)

**English** | [简体中文](README.zh-CN.md)

**Impressy** is a cross-platform native desktop image toolbox (Rust + [GPUI](https://www.gpui.rs/)) focused on lightweight, fast **local image processing**. It is **local-first**: every feature in v1 works fully offline, with no account and no API key.

> Windows 10/11 · macOS · Linux (X11 / Wayland)

---

## Status

Impressy was built in two stages:

- **v1 — Local features (implemented).** Completely offline image-processing tools. Screen capture, global hotkeys, and screen color picking remain outside the current scope.
- **v2 — AI image features (implemented; live-provider acceptance open).** Text/image generation, 12 AI editing presets, 13 industry tools, and poster design via BYOK. Seedream, Nano Banana, and OpenAI keys are stored in the operating-system credential manager. The three video entries remain explicitly planned placeholders and are not part of the marked v1/v2 image scope.

See [ADR-0001](docs/adr/0001-v1-scope-local-only.md) for why the scope is split this way.
See [ADR-0003](docs/adr/0003-remove-screen-capture.md) for the screen-capture removal decision.
See [ADR-0004](docs/adr/0004-v2-provider-contract.md) for the researched v2 provider contract.

## Features (v1)

The UI is organized into four **sections**. v1 fills the two local ones:

### Basic Image Processing

| Feature | What it does | Engine |
| --- | --- | --- |
| Crop / Resize / Rotate | Aspect-ratio presets, lossless 90° rotation, high-quality resampling | `impressy_core::transform` |
| Collage | Stack images vertically, horizontally, or in a grid | `impressy_core::collage` |
| Batch Processing | Convert / compress / resize / watermark many images; failures isolated per item | `impressy_core::batch` · `watermark` |
| Slicing | Split an image into an exact N×M grid of tiles | `impressy_core::slice` |
| QR Code | Generate and recognize QR codes with custom colors | `impressy_core::qr` |
| EXIF Manager | View and strip camera / GPS metadata **without changing a pixel** | `impressy_core::exif` |
| Screenshot Beautify | Add rounded corners, padding, gradients, borders, and shadows to an imported image; no screen capture | `impressy_core::beautify` |

### Creative Output

| Feature | What it does | Engine |
| --- | --- | --- |
| GIF Maker | Compose frames into a GIF (delay + forward/reverse/ping-pong) | `impressy_core::animation` |
| Poster Design | Generate a 9:16 AI background, position/resize three native text layers, and export a local offscreen composition | `impressy-ai` · `impressy_core::poster` |

### AI Generation and Industry Tools (v2)

- Text-to-image and reference-image generation with dynamic provider capabilities and multi-result selection.
- 12 AI editing presets, including mask-based removal with an interactive region selector.
- 13 locked-prompt industry tools: restoration, ID photos, avatars, memes, portraits, try-on, recoloring, promotional posters, platform adaptation, covers, illustrations, food enhancement, and interior previews.
- Exact local post-processing for ID photos, 3:4 promotional posters, avatars, and platform output sizes.

## Architecture

A Cargo workspace of four crates:

| Crate | Responsibility | Testable headless? |
| --- | --- | --- |
| [`impressy-core`](crates/impressy-core) | Pure local image engine: crop, compress, collage, slice, GIF, QR, EXIF, watermark, beautify. **No UI, no network, no global state.** | ✅ Fully — property-tested |
| [`impressy-app`](crates/impressy-app) | GPUI UI: four-section navigation, feature pages, and a custom crop Element | ⚠️ Type-check only on headless machines |
| [`impressy-ai`](crates/impressy-ai) | Provider trait, Seedream/Nano Banana/OpenAI adapters, HTTPS transport, capability declarations, and OS credential storage | ✅ Contract-tested with fake transports |
| [`impressy-presets`](crates/impressy-presets) | Compile-time locked prompt templates for editing and all 13 industry tools | ✅ Embedded-resource tests |

Design principles: local-first, privacy by design (no telemetry, no embedded keys), native and lightweight (single binary, **no browser-engine dependency**), GPU-direct rendering.

## Getting started

### Prerequisites

- **Rust** stable ≥ 1.85 (the workspace uses edition 2024).
- A desktop environment to *run* the app (GPUI needs a display). Building/type-checking works headless.

### Build & test

```bash
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`impressy-core` is covered by property tests (invariants such as lossless round-trip pixel-identity, exact crop ratios, N images → N GIF frames, EXIF strip preserving pixels).

### Run the app

```bash
cargo run -p impressy-app
```

## Development

- **GPUI versions are pinned** (`gpui = "=0.2.2"`, `gpui-component = "=0.5.1"`) — the only compatible pair. GPUI is pre-1.0; consult [`vendor-docs/`](vendor-docs) (extracted from the pinned sources) before writing UI code, not model memory.
- **Definition of Done:** `cargo check && cargo clippy -- -D warnings && cargo test`, plus the relevant acceptance criteria.
- **CI** runs the full quality gate on Ubuntu / Windows / macOS.
- Rendering correctness, custom-Element feel, and cold-start time can only be validated on real desktop hardware — see [ADR-0002](docs/adr/0002-core-before-ui.md) and the [v1 desktop acceptance checklist](docs/testing/v1-desktop-acceptance.md). Core regressions can be compared with the [performance probe](docs/testing/performance-probe.md); packaging details are in the [release guide](docs/release.md).
- UI text is localized (Simplified Chinese + English) via `rust-i18n`; switch at runtime, no restart.

## Documentation

- [`docs/spec/requirements.md`](docs/spec/requirements.md) & [`docs/spec/design.md`](docs/spec/design.md) — the single source of truth.
- [`docs/adr/`](docs/adr) — scope and sequencing decisions (ADRs override the spec where they conflict).
- [`CONTEXT.md`](CONTEXT.md) — domain glossary.

## License

Licensed under the [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0).
