use std::fmt;

use zeroize::Zeroize;

use crate::{AiError, Result};

/// Stable provider identifier stored in non-secret local configuration.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, Default,
)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderId {
    #[default]
    Seedream,
    NanoBanana,
    OpenAi,
}

impl ProviderId {
    pub const ALL: [Self; 3] = [Self::Seedream, Self::NanoBanana, Self::OpenAi];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Seedream => "seedream",
            Self::NanoBanana => "nano-banana",
            Self::OpenAi => "openai",
        }
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Secret wrapper whose debug/display representations never reveal the key.
pub struct ApiKey(String);

impl ApiKey {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(AiError::ApiKeyRequired);
        }
        Ok(Self(value))
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl Clone for ApiKey {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey([REDACTED])")
    }
}

impl fmt::Display for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl Drop for ApiKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum AspectRatio {
    Square,
    PortraitTwoThree,
    LandscapeThreeTwo,
    PortraitThreeFour,
    LandscapeFourThree,
    PortraitFourFive,
    LandscapeFiveFour,
    PortraitNineSixteen,
    LandscapeSixteenNine,
    UltrawideTwentyOneNine,
}

impl AspectRatio {
    pub const COMMON: [Self; 10] = [
        Self::Square,
        Self::PortraitTwoThree,
        Self::LandscapeThreeTwo,
        Self::PortraitThreeFour,
        Self::LandscapeFourThree,
        Self::PortraitFourFive,
        Self::LandscapeFiveFour,
        Self::PortraitNineSixteen,
        Self::LandscapeSixteenNine,
        Self::UltrawideTwentyOneNine,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Square => "1:1",
            Self::PortraitTwoThree => "2:3",
            Self::LandscapeThreeTwo => "3:2",
            Self::PortraitThreeFour => "3:4",
            Self::LandscapeFourThree => "4:3",
            Self::PortraitFourFive => "4:5",
            Self::LandscapeFiveFour => "5:4",
            Self::PortraitNineSixteen => "9:16",
            Self::LandscapeSixteenNine => "16:9",
            Self::UltrawideTwentyOneNine => "21:9",
        }
    }

    pub const fn openai_size(self) -> &'static str {
        match self {
            Self::Square => "1024x1024",
            Self::PortraitTwoThree | Self::PortraitThreeFour | Self::PortraitFourFive => {
                "1024x1536"
            }
            Self::PortraitNineSixteen => "1024x1792",
            Self::UltrawideTwentyOneNine => "1792x768",
            _ => "1536x1024",
        }
    }

    pub const fn seedream_size(self) -> &'static str {
        match self {
            Self::Square => "1024x1024",
            Self::PortraitTwoThree => "832x1248",
            Self::LandscapeThreeTwo => "1248x832",
            Self::PortraitThreeFour => "864x1152",
            Self::LandscapeFourThree => "1152x864",
            Self::PortraitFourFive => "896x1120",
            Self::LandscapeFiveFour => "1120x896",
            Self::PortraitNineSixteen => "720x1280",
            Self::LandscapeSixteenNine => "1280x720",
            Self::UltrawideTwentyOneNine => "1512x648",
        }
    }
}

impl fmt::Display for AspectRatio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GenerationQuality {
    Low,
    #[default]
    Medium,
    High,
}

impl GenerationQuality {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageInput {
    pub bytes: Vec<u8>,
    pub mime_type: String,
}

impl ImageInput {
    pub fn new(bytes: Vec<u8>, mime_type: impl Into<String>) -> Result<Self> {
        if bytes.is_empty() {
            return Err(AiError::InvalidRequest("reference image is empty".into()));
        }
        let mime_type = mime_type.into();
        if !matches!(
            mime_type.as_str(),
            "image/png" | "image/jpeg" | "image/webp"
        ) {
            return Err(AiError::InvalidRequest(
                "reference image must be PNG, JPEG, or WebP".into(),
            ));
        }
        Ok(Self { bytes, mime_type })
    }
}

#[derive(Debug, Clone)]
pub struct GenerationRequest {
    pub prompt: String,
    pub reference_images: Vec<ImageInput>,
    pub mask: Option<ImageInput>,
    pub aspect_ratio: AspectRatio,
    pub count: u8,
    pub quality: GenerationQuality,
}

impl GenerationRequest {
    pub fn text(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            reference_images: Vec::new(),
            mask: None,
            aspect_ratio: AspectRatio::Square,
            count: 1,
            quality: GenerationQuality::Medium,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub max_reference_images: u8,
    pub supported_aspect_ratios: &'static [AspectRatio],
    pub max_generation_count: u8,
    pub text_to_image: bool,
    pub image_to_image: bool,
    pub region_edit: bool,
    pub max_input_image_bytes: usize,
}

impl ProviderCapabilities {
    pub fn validate(self, request: &GenerationRequest) -> Result<()> {
        if request.prompt.trim().is_empty() {
            return Err(AiError::InvalidRequest("prompt is empty".into()));
        }
        if request.count == 0 || request.count > self.max_generation_count {
            return Err(AiError::InvalidRequest(format!(
                "requested image count must be 1..={}",
                self.max_generation_count
            )));
        }
        if !self.supported_aspect_ratios.contains(&request.aspect_ratio) {
            return Err(AiError::UnsupportedCapability("aspect ratio"));
        }
        if request.reference_images.len() > usize::from(self.max_reference_images) {
            return Err(AiError::TooManyReferenceImages {
                maximum: self.max_reference_images,
                actual: request.reference_images.len(),
            });
        }
        if request.reference_images.is_empty() && !self.text_to_image {
            return Err(AiError::UnsupportedCapability("text-to-image"));
        }
        if !request.reference_images.is_empty() && !self.image_to_image {
            return Err(AiError::UnsupportedCapability("image-to-image"));
        }
        if request.mask.is_some() && !self.region_edit {
            return Err(AiError::UnsupportedCapability("region edit"));
        }
        if request
            .reference_images
            .iter()
            .chain(request.mask.iter())
            .any(|image| image.bytes.len() > self.max_input_image_bytes)
        {
            return Err(AiError::ImageTooLarge {
                maximum_bytes: self.max_input_image_bytes,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedImage {
    pub bytes: Vec<u8>,
    pub mime_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationResult {
    pub images: Vec<GeneratedImage>,
    pub provider: ProviderId,
    pub request_id: Option<String>,
}

pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn capabilities(&self) -> ProviderCapabilities;
    fn generate(&self, request: &GenerationRequest, api_key: &ApiKey) -> Result<GenerationResult>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_debug_is_redacted() {
        let key = ApiKey::new("sk-secret-value").expect("key");
        assert_eq!(format!("{key:?}"), "ApiKey([REDACTED])");
        assert!(!format!("{key:?}").contains("secret-value"));
    }

    #[test]
    fn capabilities_reject_unsupported_inputs() {
        let caps = ProviderCapabilities {
            max_reference_images: 0,
            supported_aspect_ratios: &[AspectRatio::Square],
            max_generation_count: 1,
            text_to_image: true,
            image_to_image: false,
            region_edit: false,
            max_input_image_bytes: 1,
        };
        let mut request = GenerationRequest::text("hello");
        request.count = 2;
        assert!(matches!(
            caps.validate(&request),
            Err(AiError::InvalidRequest(_))
        ));
    }
}
