use std::collections::BTreeSet;
use std::sync::Arc;

use gpui::RenderImage;
use impressy_ai::{
    AspectRatio, GeneratedImage, GenerationQuality, ProviderCapabilities, ProviderId,
};

use crate::section::Feature;
use impressy_presets::AiEditPreset;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiErrorKind {
    KeyRequired,
    InvalidKey,
    InsufficientBalance,
    RateLimited,
    Network,
    ContentBlocked,
    ImageTooLarge,
    Unsupported,
    InvalidRequest,
    Provider,
    Credential,
    Io,
}

impl AiErrorKind {
    pub(crate) fn i18n_key(self) -> &'static str {
        match self {
            Self::KeyRequired => "ai.error.key_required",
            Self::InvalidKey => "ai.error.invalid_key",
            Self::InsufficientBalance => "ai.error.insufficient_balance",
            Self::RateLimited => "ai.error.rate_limited",
            Self::Network => "ai.error.network",
            Self::ContentBlocked => "ai.error.content_blocked",
            Self::ImageTooLarge => "ai.error.image_too_large",
            Self::Unsupported => "ai.error.unsupported",
            Self::InvalidRequest => "ai.error.invalid_request",
            Self::Provider => "ai.error.provider",
            Self::Credential => "ai.error.credential",
            Self::Io => "ai.error.io",
        }
    }
}

impl From<impressy_ai::AiError> for AiErrorKind {
    fn from(error: impressy_ai::AiError) -> Self {
        match error {
            impressy_ai::AiError::ApiKeyRequired | impressy_ai::AiError::CredentialMissing => {
                Self::KeyRequired
            }
            impressy_ai::AiError::InvalidCredentials => Self::InvalidKey,
            impressy_ai::AiError::InsufficientBalance => Self::InsufficientBalance,
            impressy_ai::AiError::RateLimited => Self::RateLimited,
            impressy_ai::AiError::Network => Self::Network,
            impressy_ai::AiError::ContentBlocked => Self::ContentBlocked,
            impressy_ai::AiError::ImageTooLarge { .. } => Self::ImageTooLarge,
            impressy_ai::AiError::UnsupportedCapability(_)
            | impressy_ai::AiError::TooManyReferenceImages { .. } => Self::Unsupported,
            impressy_ai::AiError::InvalidRequest(_) => Self::InvalidRequest,
            impressy_ai::AiError::CredentialStore => Self::Credential,
            impressy_ai::AiError::ProviderUnavailable | impressy_ai::AiError::InvalidResponse => {
                Self::Provider
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum AiStatus {
    #[default]
    Idle,
    Generating,
    Ready(usize),
    Saved(usize),
    Error(AiErrorKind),
}

pub(crate) struct RenderedAiImage {
    pub(crate) generated: GeneratedImage,
    pub(crate) preview: Arc<RenderImage>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) tier_id: String,
}

pub(crate) struct AiUiState {
    pub(crate) provider: ProviderId,
    pub(crate) aspect_ratio: AspectRatio,
    pub(crate) count: u8,
    pub(crate) quality: GenerationQuality,
    pub(crate) selected_tiers: BTreeSet<usize>,
    pub(crate) selected_results: BTreeSet<usize>,
    pub(crate) edit_preset_index: usize,
    pub(crate) id_size: usize,
    pub(crate) id_background: usize,
    pub(crate) id_attire: usize,
    pub(crate) try_on_model: usize,
    pub(crate) try_on_scene: usize,
    pub(crate) cover_platform: usize,
    pub(crate) cover_style: usize,
    pub(crate) article_usage: usize,
    pub(crate) article_style: usize,
    pub(crate) compare_original: bool,
    pub(crate) results: Vec<RenderedAiImage>,
    pub(crate) status: AiStatus,
}

impl Default for AiUiState {
    fn default() -> Self {
        Self::new(ProviderId::default())
    }
}

impl AiUiState {
    pub(crate) fn new(provider: ProviderId) -> Self {
        Self {
            provider,
            aspect_ratio: AspectRatio::Square,
            count: 1,
            quality: GenerationQuality::Medium,
            selected_tiers: BTreeSet::from([0]),
            selected_results: BTreeSet::new(),
            edit_preset_index: 0,
            id_size: 0,
            id_background: 0,
            id_attire: 0,
            try_on_model: 0,
            try_on_scene: 0,
            cover_platform: 0,
            cover_style: 0,
            article_usage: 0,
            article_style: 0,
            compare_original: false,
            results: Vec::new(),
            status: AiStatus::Idle,
        }
    }

    pub(crate) fn apply_capabilities(&mut self, capabilities: ProviderCapabilities) {
        if !capabilities
            .supported_aspect_ratios
            .contains(&self.aspect_ratio)
        {
            self.aspect_ratio = capabilities.supported_aspect_ratios[0];
        }
        self.count = self.count.clamp(1, capabilities.max_generation_count);
    }

    pub(crate) fn cycle_provider(&mut self) {
        self.provider = match self.provider {
            ProviderId::Seedream => ProviderId::NanoBanana,
            ProviderId::NanoBanana => ProviderId::OpenAi,
            ProviderId::OpenAi => ProviderId::Seedream,
        };
    }

    pub(crate) fn toggle_tier(&mut self, feature: Feature, index: usize) {
        let maximum = feature.ai_spec().map_or(1, |spec| spec.max_selected);
        if self.selected_tiers.contains(&index) {
            if self.selected_tiers.len() > 1 {
                self.selected_tiers.remove(&index);
            }
        } else if self.selected_tiers.len() < maximum {
            self.selected_tiers.insert(index);
        }
    }

    pub(crate) fn set_edit_preset(&mut self, index: usize) {
        if index < AiEditPreset::ALL.len() {
            self.edit_preset_index = index;
        }
    }

    pub(crate) fn composed_tier(&self, feature: Feature) -> Option<String> {
        match feature {
            Feature::IdPhoto => Some(compose_id_photo_tier(
                self.id_size,
                self.id_background,
                self.id_attire,
            )),
            Feature::ModelTryOn => Some(compose_try_on_tier(self.try_on_model, self.try_on_scene)),
            Feature::CoverFactory => {
                Some(compose_cover_tier(self.cover_platform, self.cover_style))
            }
            Feature::ArticleIllustration => {
                Some(compose_article_tier(self.article_usage, self.article_style))
            }
            _ => None,
        }
    }

    pub(crate) fn toggle_result(&mut self, index: usize) {
        if !self.selected_results.remove(&index) {
            self.selected_results.insert(index);
        }
    }

    pub(crate) fn set_results(&mut self, results: Vec<RenderedAiImage>) {
        self.selected_results = (0..results.len()).collect();
        self.status = AiStatus::Ready(results.len());
        self.results = results;
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AiFeatureSpec {
    pub(crate) tiers: &'static [&'static str],
    pub(crate) max_selected: usize,
    pub(crate) needs_image: bool,
    pub(crate) default_ratio: AspectRatio,
}

pub(crate) const RESTORE_TIERS: &[&str] = &["repair", "colorize", "clarity"];
pub(crate) const ID_SIZES: &[&str] = &["one-inch", "two-inch", "visa"];
pub(crate) const ID_BACKGROUNDS: &[&str] = &["blue", "red", "white"];
pub(crate) const ID_ATTIRES: &[&str] = &["suit", "female-suit"];
pub(crate) const AVATAR_TIERS: &[&str] = &[
    "japanese-anime",
    "cyberpunk",
    "chinese-ink",
    "clay-3d",
    "pixel-art",
    "watercolor",
    "comic",
    "minimal-flat",
    "photorealistic-studio",
];
pub(crate) const MEME_TIERS: &[&str] = &[
    "laughing",
    "shocked",
    "eye-rolling",
    "crying",
    "angry",
    "cheering",
    "confused",
    "sleepy",
    "proud",
    "awkward",
    "love",
    "speechless",
];
pub(crate) const PORTRAIT_TIERS: &[&str] = &[
    "wedding",
    "hanfu",
    "graduation",
    "travel",
    "professional",
    "cinematic",
    "retro",
    "fashion",
];
pub(crate) const TRY_ON_MODELS: &[&str] = &["asian-female", "asian-male"];
pub(crate) const TRY_ON_SCENES: &[&str] = &["cafe", "studio"];
pub(crate) const COLOR_TIERS: &[&str] = &["burgundy", "navy", "haze-blue", "custom"];
pub(crate) const PROMO_TIERS: &[&str] = &[
    "limited-time-discount",
    "new-arrival",
    "clearance",
    "member-day",
];
pub(crate) const PLATFORM_TIERS: &[&str] = &[
    "taobao-main-800x800",
    "xiaohongshu-1242x1656",
    "wechat-cover-900x383",
    "douyin-1080x1920",
];
pub(crate) const COVER_PLATFORMS: &[&str] = &["wechat", "xiaohongshu", "bilibili", "douyin"];
pub(crate) const COVER_STYLES: &[&str] = &["bold", "editorial"];
pub(crate) const ARTICLE_USAGES: &[&str] = &["ppt", "editorial"];
pub(crate) const ARTICLE_STYLES: &[&str] = &["business-gradient", "flat"];
pub(crate) const FOOD_TIERS: &[&str] = &["delivery-standard", "menu-premium"];
pub(crate) const INTERIOR_TIERS: &[&str] = &[
    "scandinavian",
    "modern-chinese",
    "light-luxury",
    "warm-minimal",
];

pub(crate) fn compose_id_photo_tier(size: usize, background: usize, attire: usize) -> String {
    format!(
        "{}-{}-{}",
        pick(ID_SIZES, size),
        pick(ID_BACKGROUNDS, background),
        pick(ID_ATTIRES, attire)
    )
}

pub(crate) fn compose_try_on_tier(model: usize, scene: usize) -> String {
    format!("{}-{}", pick(TRY_ON_MODELS, model), pick(TRY_ON_SCENES, scene))
}

pub(crate) fn compose_cover_tier(platform: usize, style: usize) -> String {
    format!(
        "{}-{}",
        pick(COVER_PLATFORMS, platform),
        pick(COVER_STYLES, style)
    )
}

pub(crate) fn compose_article_tier(usage: usize, style: usize) -> String {
    format!(
        "{}-{}",
        pick(ARTICLE_USAGES, usage),
        pick(ARTICLE_STYLES, style)
    )
}

fn pick<'a>(items: &[&'a str], index: usize) -> &'a str {
    items.get(index).copied().unwrap_or(items[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_cycle_and_capabilities_clamp_parameters() {
        let mut state = AiUiState::new(ProviderId::Seedream);
        state.count = 9;
        state.aspect_ratio = AspectRatio::UltrawideTwentyOneNine;
        state.cycle_provider();
        assert_eq!(state.provider, ProviderId::NanoBanana);
        state.apply_capabilities(ProviderCapabilities {
            max_reference_images: 1,
            supported_aspect_ratios: &[AspectRatio::Square],
            max_generation_count: 1,
            text_to_image: true,
            image_to_image: true,
            region_edit: false,
            max_input_image_bytes: 1024,
        });
        assert_eq!(state.count, 1);
        assert_eq!(state.aspect_ratio, AspectRatio::Square);
    }

    #[test]
    fn composed_industry_tiers_join_independent_axes() {
        assert_eq!(
            compose_id_photo_tier(0, 0, 0),
            "one-inch-blue-suit"
        );
        assert_eq!(
            compose_id_photo_tier(2, 2, 1),
            "visa-white-female-suit"
        );
        assert_eq!(compose_try_on_tier(0, 1), "asian-female-studio");
        assert_eq!(compose_cover_tier(0, 1), "wechat-editorial");
        assert_eq!(compose_article_tier(0, 1), "ppt-flat");
    }

    #[test]
    fn multi_tier_tools_enforce_selection_limit() {
        let mut state = AiUiState::default();
        for index in 0..AVATAR_TIERS.len() {
            state.toggle_tier(Feature::AvatarStudio, index);
        }
        assert_eq!(state.selected_tiers.len(), 6);
        assert!(state.selected_tiers.contains(&0));
    }
}
