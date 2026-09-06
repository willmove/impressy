//! 板块与功能注册表（Requirement 34）。
//!
//! 四大板块的划分以 `docs/spec/design.md` §Overview 为准：
//! 「基础图片处理」含编辑 / 拼图 / 批量 / 切图 / 二维码 / EXIF / 截图美化；
//! 「创作输出」含 GIF 制作（v1）与海报设计（FR-11，v2）；
//! 两个 AI 板块在 v2 提供完整图片工具。三个视频功能（Requirement 34.5–34.10）仍作为
//! 独立的开发中入口挂在「AI 生成与改图」板块下。
//!
//! 每个 [`Feature`] 只携带 i18n 键与 v1/v2 标记，**不含任何 UI 与业务逻辑**——
//! 渲染与工作区编排在 [`crate::shell`]，图像处理在 `impressy-core`。

/// 主界面四大板块（Requirement 34.1–34.4）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    /// 基础图片处理（v1，有实质内容）。
    BasicImage,
    /// AI 生成与改图（v2）。
    AiGeneration,
    /// 行业定制 AI 工具（v2）。
    IndustryTools,
    /// 创作输出（v1 的 GIF + v2 的海报）。
    CreativeOutput,
}

impl Section {
    /// 导航标签的 i18n 键。
    pub fn nav_key(self) -> &'static str {
        match self {
            Self::BasicImage => "nav.basic_image",
            Self::AiGeneration => "nav.ai_generation",
            Self::IndustryTools => "nav.industry_tools",
            Self::CreativeOutput => "nav.creative_output",
        }
    }
}

/// 单个功能入口（Requirement 34）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Feature {
    // —— 基础图片处理（v1）——
    /// 裁剪 / 缩放 / 旋转（FR-01，`impressy_core::transform`）。
    Edit,
    /// 拼图拼接（FR-02，`impressy_core::collage`）。
    Collage,
    /// 批量处理（FR-03，`impressy_core::batch`）。
    Batch,
    /// 切图（FR-04，`impressy_core::slice`）。
    Slice,
    /// 二维码（FR-05，`impressy_core::qr`）。
    QrCode,
    /// EXIF 管理（FR-06，`impressy_core::exif`）。
    Exif,
    /// 截图美化（FR-07，`impressy_core::beautify`）。
    Beautify,
    // —— 创作输出 ——
    /// GIF 制作（FR-10，v1，`impressy_core::animation`）。
    Gif,
    /// 海报设计（FR-11，v2）。
    Poster,
    // —— AI 生成与改图（v2）——
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
    // —— 行业定制 AI 工具（v2）——
    OldPhotoRestoration,
    IdPhoto,
    AvatarStudio,
    MemeGenerator,
    AiPortrait,
    ModelTryOn,
    ProductRecolor,
    PromotionalPoster,
    PlatformAdaptation,
    CoverFactory,
    ArticleIllustration,
    FoodEnhancement,
    InteriorPreview,
}

impl Feature {
    /// Canonical section assignment shared by navigation, search, menus, and active highlighting.
    pub fn section(self) -> Section {
        match self {
            Self::Edit
            | Self::Collage
            | Self::Batch
            | Self::Slice
            | Self::QrCode
            | Self::Exif
            | Self::Beautify => Section::BasicImage,
            Self::TextToImage
            | Self::ImageEdit
            | Self::VideoWatermark
            | Self::VideoSubtitle
            | Self::VideoClarity => Section::AiGeneration,
            Self::Gif | Self::Poster => Section::CreativeOutput,
            _ => Section::IndustryTools,
        }
    }

    /// 是否为 v1 功能（完全离线、无 API Key）。false 表示联网的 v2 功能或视频占位。
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

    /// v2 AI 图片功能。三个视频入口不在本次图片工具范围内。
    pub fn is_ai(self) -> bool {
        !self.is_v1()
            && !matches!(
                self,
                Self::VideoWatermark | Self::VideoSubtitle | Self::VideoClarity
            )
    }

    pub fn ai_spec(self) -> Option<crate::ai_state::AiFeatureSpec> {
        use crate::ai_state::*;
        use impressy_ai::AspectRatio;
        let spec = match self {
            Self::TextToImage => AiFeatureSpec {
                tiers: &[],
                max_selected: 1,
                needs_image: false,
                default_ratio: AspectRatio::Square,
            },
            Self::ImageEdit => AiFeatureSpec {
                tiers: &[],
                max_selected: 1,
                needs_image: true,
                default_ratio: AspectRatio::Square,
            },
            Self::Poster => AiFeatureSpec {
                tiers: &["tech-launch", "minimal", "luxury", "playful"],
                max_selected: 1,
                needs_image: false,
                default_ratio: AspectRatio::PortraitNineSixteen,
            },
            Self::OldPhotoRestoration => AiFeatureSpec {
                tiers: RESTORE_TIERS,
                max_selected: 3,
                needs_image: true,
                default_ratio: AspectRatio::Square,
            },
            Self::IdPhoto => AiFeatureSpec {
                tiers: &[],
                max_selected: 1,
                needs_image: true,
                default_ratio: AspectRatio::PortraitThreeFour,
            },
            Self::AvatarStudio => AiFeatureSpec {
                tiers: AVATAR_TIERS,
                max_selected: 6,
                needs_image: true,
                default_ratio: AspectRatio::Square,
            },
            Self::MemeGenerator => AiFeatureSpec {
                tiers: MEME_TIERS,
                max_selected: 12,
                needs_image: true,
                default_ratio: AspectRatio::Square,
            },
            Self::AiPortrait => AiFeatureSpec {
                tiers: PORTRAIT_TIERS,
                max_selected: 8,
                needs_image: true,
                default_ratio: AspectRatio::PortraitThreeFour,
            },
            Self::ModelTryOn => AiFeatureSpec {
                tiers: &[],
                max_selected: 1,
                needs_image: true,
                default_ratio: AspectRatio::PortraitThreeFour,
            },
            Self::ProductRecolor => AiFeatureSpec {
                tiers: COLOR_TIERS,
                max_selected: 4,
                needs_image: true,
                default_ratio: AspectRatio::Square,
            },
            Self::PromotionalPoster => AiFeatureSpec {
                tiers: PROMO_TIERS,
                max_selected: 1,
                needs_image: true,
                default_ratio: AspectRatio::PortraitThreeFour,
            },
            Self::PlatformAdaptation => AiFeatureSpec {
                tiers: PLATFORM_TIERS,
                max_selected: 4,
                needs_image: true,
                default_ratio: AspectRatio::Square,
            },
            Self::CoverFactory => AiFeatureSpec {
                tiers: &[],
                max_selected: 1,
                needs_image: false,
                default_ratio: AspectRatio::LandscapeSixteenNine,
            },
            Self::ArticleIllustration => AiFeatureSpec {
                tiers: &[],
                max_selected: 1,
                needs_image: false,
                default_ratio: AspectRatio::LandscapeSixteenNine,
            },
            Self::FoodEnhancement => AiFeatureSpec {
                tiers: FOOD_TIERS,
                max_selected: 1,
                needs_image: true,
                default_ratio: AspectRatio::LandscapeFourThree,
            },
            Self::InteriorPreview => AiFeatureSpec {
                tiers: INTERIOR_TIERS,
                max_selected: 4,
                needs_image: true,
                default_ratio: AspectRatio::LandscapeFourThree,
            },
            _ => return None,
        };
        Some(spec)
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
            Self::OldPhotoRestoration => "feat-old-photo",
            Self::IdPhoto => "feat-id-photo",
            Self::AvatarStudio => "feat-avatar",
            Self::MemeGenerator => "feat-meme",
            Self::AiPortrait => "feat-ai-portrait",
            Self::ModelTryOn => "feat-try-on",
            Self::ProductRecolor => "feat-recolor",
            Self::PromotionalPoster => "feat-promo-poster",
            Self::PlatformAdaptation => "feat-platform-adapt",
            Self::CoverFactory => "feat-cover",
            Self::ArticleIllustration => "feat-article",
            Self::FoodEnhancement => "feat-food",
            Self::InteriorPreview => "feat-interior",
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
            Self::OldPhotoRestoration => "feature.old_photo.name",
            Self::IdPhoto => "feature.id_photo.name",
            Self::AvatarStudio => "feature.avatar.name",
            Self::MemeGenerator => "feature.meme.name",
            Self::AiPortrait => "feature.ai_portrait.name",
            Self::ModelTryOn => "feature.try_on.name",
            Self::ProductRecolor => "feature.recolor.name",
            Self::PromotionalPoster => "feature.promo_poster.name",
            Self::PlatformAdaptation => "feature.platform_adapt.name",
            Self::CoverFactory => "feature.cover_factory.name",
            Self::ArticleIllustration => "feature.article_illustration.name",
            Self::FoodEnhancement => "feature.food.name",
            Self::InteriorPreview => "feature.interior.name",
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
            Self::OldPhotoRestoration => "feature.old_photo.desc",
            Self::IdPhoto => "feature.id_photo.desc",
            Self::AvatarStudio => "feature.avatar.desc",
            Self::MemeGenerator => "feature.meme.desc",
            Self::AiPortrait => "feature.ai_portrait.desc",
            Self::ModelTryOn => "feature.try_on.desc",
            Self::ProductRecolor => "feature.recolor.desc",
            Self::PromotionalPoster => "feature.promo_poster.desc",
            Self::PlatformAdaptation => "feature.platform_adapt.desc",
            Self::CoverFactory => "feature.cover_factory.desc",
            Self::ArticleIllustration => "feature.article_illustration.desc",
            Self::FoodEnhancement => "feature.food.desc",
            Self::InteriorPreview => "feature.interior.desc",
        }
    }
}
