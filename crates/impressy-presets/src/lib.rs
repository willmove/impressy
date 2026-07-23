//! Locked prompt templates for v2 AI editing and industry tools.
//!
//! Template text is compiled into the binary and is never returned to the UI. Callers select a
//! tool and a semantic **档位**, then pass the rendered prompt directly to `impressy-ai`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AiEditPreset {
    Removal,
    BackgroundReplacement,
    Expansion,
    ClarityEnhancement,
    BackgroundRemoval,
    EcommerceWhiteBackground,
    PortraitEnhancement,
    TextReplacement,
    StickerApplication,
    ProductPhotoSet,
    ProfessionalPortrait,
    Freeform,
}

impl AiEditPreset {
    pub const ALL: [Self; 12] = [
        Self::Removal,
        Self::BackgroundReplacement,
        Self::Expansion,
        Self::ClarityEnhancement,
        Self::BackgroundRemoval,
        Self::EcommerceWhiteBackground,
        Self::PortraitEnhancement,
        Self::TextReplacement,
        Self::StickerApplication,
        Self::ProductPhotoSet,
        Self::ProfessionalPortrait,
        Self::Freeform,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Removal => "removal",
            Self::BackgroundReplacement => "background-replacement",
            Self::Expansion => "expansion",
            Self::ClarityEnhancement => "clarity-enhancement",
            Self::BackgroundRemoval => "background-removal",
            Self::EcommerceWhiteBackground => "ecommerce-white-background",
            Self::PortraitEnhancement => "portrait-enhancement",
            Self::TextReplacement => "text-replacement",
            Self::StickerApplication => "sticker-application",
            Self::ProductPhotoSet => "product-photo-set",
            Self::ProfessionalPortrait => "professional-portrait",
            Self::Freeform => "freeform",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndustryTool {
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

impl IndustryTool {
    pub const ALL: [Self; 13] = [
        Self::OldPhotoRestoration,
        Self::IdPhoto,
        Self::AvatarStudio,
        Self::MemeGenerator,
        Self::AiPortrait,
        Self::ModelTryOn,
        Self::ProductRecolor,
        Self::PromotionalPoster,
        Self::PlatformAdaptation,
        Self::CoverFactory,
        Self::ArticleIllustration,
        Self::FoodEnhancement,
        Self::InteriorPreview,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::OldPhotoRestoration => "old-photo-restoration",
            Self::IdPhoto => "id-photo",
            Self::AvatarStudio => "avatar-studio",
            Self::MemeGenerator => "meme-generator",
            Self::AiPortrait => "ai-portrait",
            Self::ModelTryOn => "model-try-on",
            Self::ProductRecolor => "product-recolor",
            Self::PromotionalPoster => "promotional-poster",
            Self::PlatformAdaptation => "platform-adaptation",
            Self::CoverFactory => "cover-factory",
            Self::ArticleIllustration => "article-illustration",
            Self::FoodEnhancement => "food-enhancement",
            Self::InteriorPreview => "interior-preview",
        }
    }
}

pub fn render_edit_prompt(
    preset: AiEditPreset,
    tier: &str,
    user_instruction: &str,
) -> Option<String> {
    let template = match preset {
        AiEditPreset::Removal => include_str!("../templates/edit/removal.txt"),
        AiEditPreset::BackgroundReplacement => {
            include_str!("../templates/edit/background-replacement.txt")
        }
        AiEditPreset::Expansion => include_str!("../templates/edit/expansion.txt"),
        AiEditPreset::ClarityEnhancement => {
            include_str!("../templates/edit/clarity-enhancement.txt")
        }
        AiEditPreset::BackgroundRemoval => {
            include_str!("../templates/edit/background-removal.txt")
        }
        AiEditPreset::EcommerceWhiteBackground => {
            include_str!("../templates/edit/ecommerce-white-background.txt")
        }
        AiEditPreset::PortraitEnhancement => {
            include_str!("../templates/edit/portrait-enhancement.txt")
        }
        AiEditPreset::TextReplacement => include_str!("../templates/edit/text-replacement.txt"),
        AiEditPreset::StickerApplication => {
            include_str!("../templates/edit/sticker-application.txt")
        }
        AiEditPreset::ProductPhotoSet => include_str!("../templates/edit/product-photo-set.txt"),
        AiEditPreset::ProfessionalPortrait => {
            include_str!("../templates/edit/professional-portrait.txt")
        }
        AiEditPreset::Freeform => return None,
    };
    Some(render(template, tier, user_instruction))
}

pub fn render_industry_prompt(tool: IndustryTool, tier: &str, user_instruction: &str) -> String {
    let template = match tool {
        IndustryTool::OldPhotoRestoration => {
            include_str!("../templates/industry/old-photo-restoration.txt")
        }
        IndustryTool::IdPhoto => include_str!("../templates/industry/id-photo.txt"),
        IndustryTool::AvatarStudio => include_str!("../templates/industry/avatar-studio.txt"),
        IndustryTool::MemeGenerator => include_str!("../templates/industry/meme-generator.txt"),
        IndustryTool::AiPortrait => include_str!("../templates/industry/ai-portrait.txt"),
        IndustryTool::ModelTryOn => include_str!("../templates/industry/model-try-on.txt"),
        IndustryTool::ProductRecolor => include_str!("../templates/industry/product-recolor.txt"),
        IndustryTool::PromotionalPoster => {
            include_str!("../templates/industry/promotional-poster.txt")
        }
        IndustryTool::PlatformAdaptation => {
            include_str!("../templates/industry/platform-adaptation.txt")
        }
        IndustryTool::CoverFactory => include_str!("../templates/industry/cover-factory.txt"),
        IndustryTool::ArticleIllustration => {
            include_str!("../templates/industry/article-illustration.txt")
        }
        IndustryTool::FoodEnhancement => {
            include_str!("../templates/industry/food-enhancement.txt")
        }
        IndustryTool::InteriorPreview => {
            include_str!("../templates/industry/interior-preview.txt")
        }
    };
    render(template, tier, user_instruction)
}

pub fn render_poster_background_prompt(tier: &str, user_instruction: &str) -> String {
    render(
        include_str!("../templates/poster-background.txt"),
        tier,
        user_instruction,
    )
}

fn render(template: &str, tier: &str, user_instruction: &str) -> String {
    template
        .replace("{{tier}}", tier.trim())
        .replace("{{user_instruction}}", user_instruction.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_locked_edit_preset_is_embedded() {
        for preset in AiEditPreset::ALL {
            let prompt = render_edit_prompt(preset, "default", "detail");
            if preset == AiEditPreset::Freeform {
                assert!(prompt.is_none());
            } else {
                let prompt = prompt.expect("locked prompt");
                assert!(prompt.len() > 80, "{} prompt is too short", preset.id());
                assert!(!prompt.contains("{{"));
            }
        }
    }

    #[test]
    fn every_industry_tool_has_a_locked_prompt() {
        for tool in IndustryTool::ALL {
            let prompt = render_industry_prompt(tool, "tier", "detail");
            assert!(prompt.len() > 100, "{} prompt is too short", tool.id());
            assert!(!prompt.contains("{{"));
        }
    }
}
