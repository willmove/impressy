//! 板块与功能注册表（Requirement 34）。
//!
//! 四大板块的划分以 `docs/spec/design.md` §Overview 为准：
//! 「基础图片处理」含编辑 / 拼图 / 批量 / 切图 / 二维码 / EXIF / 截图美化；
//! 「创作输出」含 GIF 制作（v1）与海报设计（FR-11，v2）；
//! 两个 AI 板块整体属 v2。三个视频功能（Requirement 34.5–34.10）作为
//! v2 占位入口挂在「AI 生成与改图」板块下。
//!
//! 每个 [`Feature`] 只携带 i18n 键与 v1/v2 标记，**不含任何 UI 与业务逻辑**——
//! 渲染在 [`crate::pages`]，图像处理在 `rastery-core`。

/// 主界面四大板块（Requirement 34.1–34.4）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    /// 基础图片处理（v1，有实质内容）。
    BasicImage,
    /// AI 生成与改图（v2，占位）。
    AiGeneration,
    /// 行业定制 AI 工具（v2，占位）。
    IndustryTools,
    /// 创作输出（v1 的 GIF + v2 的海报）。
    CreativeOutput,
}

impl Section {
    /// 四大板块，按导航顺序。
    pub const ALL: [Self; 4] = [
        Self::BasicImage,
        Self::AiGeneration,
        Self::IndustryTools,
        Self::CreativeOutput,
    ];

    /// 导航标签的 i18n 键。
    pub fn nav_key(self) -> &'static str {
        match self {
            Self::BasicImage => "nav.basic_image",
            Self::AiGeneration => "nav.ai_generation",
            Self::IndustryTools => "nav.industry_tools",
            Self::CreativeOutput => "nav.creative_output",
        }
    }

    /// 稳定的元素 id 片段（用于 GPUI 的 `.id(...)`）。
    pub fn id(self) -> &'static str {
        match self {
            Self::BasicImage => "sec-basic",
            Self::AiGeneration => "sec-ai",
            Self::IndustryTools => "sec-industry",
            Self::CreativeOutput => "sec-output",
        }
    }

    /// 该板块下的功能入口，按展示顺序。
    pub fn features(self) -> &'static [Feature] {
        match self {
            Self::BasicImage => &[
                Feature::Edit,
                Feature::Collage,
                Feature::Batch,
                Feature::Slice,
                Feature::QrCode,
                Feature::Exif,
                Feature::Beautify,
            ],
            // 三个视频功能是 Requirement 34.5–34.7 要求提供的入口，均为 v2 占位。
            Self::AiGeneration => &[
                Feature::TextToImage,
                Feature::ImageEdit,
                Feature::VideoWatermark,
                Feature::VideoSubtitle,
                Feature::VideoClarity,
            ],
            Self::IndustryTools => &[Feature::Industry],
            Self::CreativeOutput => &[Feature::Gif, Feature::Poster],
        }
    }
}

/// 单个功能入口（Requirement 34）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Feature {
    // —— 基础图片处理（v1）——
    /// 裁剪 / 缩放 / 旋转（FR-01，`rastery_core::transform`）。
    Edit,
    /// 拼图拼接（FR-02，`rastery_core::collage`）。
    Collage,
    /// 批量处理（FR-03，`rastery_core::batch`）。
    Batch,
    /// 切图（FR-04，`rastery_core::slice`）。
    Slice,
    /// 二维码（FR-05，`rastery_core::qr`）。
    QrCode,
    /// EXIF 管理（FR-06，`rastery_core::exif`）。
    Exif,
    /// 截图美化（FR-07，`rastery_core::beautify`）。
    Beautify,
    // —— 创作输出 ——
    /// GIF 制作（FR-10，v1，`rastery_core::animation`）。
    Gif,
    /// 海报设计（FR-11，v2 占位）。
    Poster,
    // —— AI 生成与改图（v2 占位）——
    /// 文生图（v2）。
    TextToImage,
    /// AI 改图（v2）。
    ImageEdit,
    /// 视频去水印（Requirement 34.5，v2 占位）。
    VideoWatermark,
    /// 视频去字幕（Requirement 34.6，v2 占位）。
    VideoSubtitle,
    /// 视频清晰度增强（Requirement 34.7，v2 占位）。
    VideoClarity,
    // —— 行业定制 AI 工具（v2 占位）——
    /// 行业专用 AI 工具合集（v2）。
    Industry,
}

impl Feature {
    /// 是否为 v1 功能（完全离线、无 API Key）。false 表示 v2 占位。
    pub fn is_v1(self) -> bool {
        matches!(
            self,
            Self::Edit
                | Self::Collage
                | Self::Batch
                | Self::Slice
                | Self::QrCode
                | Self::Exif
                | Self::Beautify
                | Self::Gif
        )
    }

    /// 稳定的元素 id 片段。
    pub fn id(self) -> &'static str {
        match self {
            Self::Edit => "feat-edit",
            Self::Collage => "feat-collage",
            Self::Batch => "feat-batch",
            Self::Slice => "feat-slice",
            Self::QrCode => "feat-qrcode",
            Self::Exif => "feat-exif",
            Self::Beautify => "feat-beautify",
            Self::Gif => "feat-gif",
            Self::Poster => "feat-poster",
            Self::TextToImage => "feat-t2i",
            Self::ImageEdit => "feat-img-edit",
            Self::VideoWatermark => "feat-video-wm",
            Self::VideoSubtitle => "feat-video-sub",
            Self::VideoClarity => "feat-video-clarity",
            Self::Industry => "feat-industry",
        }
    }

    /// 功能名称的 i18n 键。
    pub fn name_key(self) -> &'static str {
        match self {
            Self::Edit => "feature.edit.name",
            Self::Collage => "feature.collage.name",
            Self::Batch => "feature.batch.name",
            Self::Slice => "feature.slice.name",
            Self::QrCode => "feature.qrcode.name",
            Self::Exif => "feature.exif.name",
            Self::Beautify => "feature.beautify.name",
            Self::Gif => "feature.gif.name",
            Self::Poster => "feature.poster.name",
            Self::TextToImage => "feature.text_to_image.name",
            Self::ImageEdit => "feature.image_edit.name",
            Self::VideoWatermark => "feature.video_watermark.name",
            Self::VideoSubtitle => "feature.video_subtitle.name",
            Self::VideoClarity => "feature.video_clarity.name",
            Self::Industry => "feature.industry.name",
        }
    }

    /// 功能描述的 i18n 键。
    pub fn desc_key(self) -> &'static str {
        match self {
            Self::Edit => "feature.edit.desc",
            Self::Collage => "feature.collage.desc",
            Self::Batch => "feature.batch.desc",
            Self::Slice => "feature.slice.desc",
            Self::QrCode => "feature.qrcode.desc",
            Self::Exif => "feature.exif.desc",
            Self::Beautify => "feature.beautify.desc",
            Self::Gif => "feature.gif.desc",
            Self::Poster => "feature.poster.desc",
            Self::TextToImage => "feature.text_to_image.desc",
            Self::ImageEdit => "feature.image_edit.desc",
            Self::VideoWatermark => "feature.video_watermark.desc",
            Self::VideoSubtitle => "feature.video_subtitle.desc",
            Self::VideoClarity => "feature.video_clarity.desc",
            Self::Industry => "feature.industry.desc",
        }
    }
}
