use std::sync::Arc;

use base64::Engine as _;
use serde_json::json;

use super::shared::{decode_image, finish, map_http_error};
use crate::{
    AiError, ApiKey, AspectRatio, GenerationRequest, GenerationResult, HttpRequest, HttpTransport,
    Provider, ProviderCapabilities, ProviderId, Result, UreqTransport,
};

const RATIOS: &[AspectRatio] = &AspectRatio::COMMON;

pub struct GeminiProvider {
    transport: Arc<dyn HttpTransport>,
    endpoint: String,
    model: String,
}

impl Default for GeminiProvider {
    fn default() -> Self {
        Self::new(Arc::new(UreqTransport::default()))
    }
}

impl GeminiProvider {
    pub fn new(transport: Arc<dyn HttpTransport>) -> Self {
        Self {
            transport,
            endpoint: "https://generativelanguage.googleapis.com/v1beta/interactions".into(),
            model: "gemini-3.1-flash-image".into(),
        }
    }
}

impl Provider for GeminiProvider {
    fn id(&self) -> ProviderId {
        ProviderId::NanoBanana
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            max_reference_images: 14,
            supported_aspect_ratios: RATIOS,
            max_generation_count: 1,
            text_to_image: true,
            image_to_image: true,
            region_edit: false,
            max_input_image_bytes: 20 * 1024 * 1024,
        }
    }

    fn generate(&self, request: &GenerationRequest, api_key: &ApiKey) -> Result<GenerationResult> {
        self.capabilities().validate(request)?;
        let mut input = vec![json!({"type": "text", "text": request.prompt})];
        input.extend(request.reference_images.iter().map(|image| {
            json!({
                "type": "image",
                "mime_type": image.mime_type,
                "data": base64::engine::general_purpose::STANDARD.encode(&image.bytes)
            })
        }));
        let body = json!({
            "model": self.model,
            "input": input,
            "response_format": {
                "type": "image",
                "mime_type": "image/png",
                "aspect_ratio": request.aspect_ratio.as_str(),
                "image_size": if request.quality == crate::GenerationQuality::High { "2K" } else { "1K" }
            }
        });
        let response = self
            .transport
            .execute(HttpRequest {
                method: "POST",
                url: self.endpoint.clone(),
                headers: vec![("x-goog-api-key".into(), api_key.expose().into())],
                content_type: "application/json".into(),
                body: serde_json::to_vec(&body)
                    .map_err(|_| AiError::InvalidRequest("JSON".into()))?,
            })
            .map_err(|_| AiError::Network)?;
        if !(200..300).contains(&response.status) {
            return Err(map_http_error(response.status, &response.body));
        }
        let value: serde_json::Value =
            serde_json::from_slice(&response.body).map_err(|_| AiError::InvalidResponse)?;
        let images = value
            .get("steps")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter(|step| {
                step.get("type").and_then(|value| value.as_str()) == Some("model_output")
            })
            .filter_map(|step| step.get("content").and_then(|value| value.as_array()))
            .flatten()
            .filter(|block| block.get("type").and_then(|value| value.as_str()) == Some("image"))
            .map(|block| {
                decode_image(
                    block
                        .get("data")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default(),
                    block.get("mime_type").and_then(|value| value.as_str()),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        finish(self.id(), response, images)
    }
}
