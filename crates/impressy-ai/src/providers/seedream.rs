use std::sync::Arc;

use base64::Engine as _;
use serde_json::json;

use super::shared::{decode_image, finish, map_http_error};
use crate::{
    AiError, ApiKey, AspectRatio, GenerationRequest, GenerationResult, HttpRequest, HttpTransport,
    Provider, ProviderCapabilities, ProviderId, Result, UreqTransport,
};

const RATIOS: &[AspectRatio] = &AspectRatio::COMMON;

pub struct SeedreamProvider {
    transport: Arc<dyn HttpTransport>,
    endpoint: String,
    model: String,
}

impl Default for SeedreamProvider {
    fn default() -> Self {
        Self::new(Arc::new(UreqTransport::default()))
    }
}

impl SeedreamProvider {
    pub fn new(transport: Arc<dyn HttpTransport>) -> Self {
        Self {
            transport,
            endpoint: "https://ark.ap-southeast.bytepluses.com/api/v3/images/generations".into(),
            model: "dola-seedream-5-0-pro-260628".into(),
        }
    }
}

impl Provider for SeedreamProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Seedream
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            max_reference_images: 10,
            supported_aspect_ratios: RATIOS,
            max_generation_count: 1,
            text_to_image: true,
            image_to_image: true,
            region_edit: false,
            max_input_image_bytes: 30 * 1024 * 1024,
        }
    }

    fn generate(&self, request: &GenerationRequest, api_key: &ApiKey) -> Result<GenerationResult> {
        self.capabilities().validate(request)?;
        let images = request
            .reference_images
            .iter()
            .map(|image| {
                format!(
                    "data:{};base64,{}",
                    image.mime_type,
                    base64::engine::general_purpose::STANDARD.encode(&image.bytes)
                )
            })
            .collect::<Vec<_>>();
        let mut body = json!({
            "model": self.model,
            "prompt": request.prompt,
            "size": request.aspect_ratio.seedream_size(),
            "output_format": "png",
            "response_format": "b64_json",
            "watermark": false
        });
        if !images.is_empty() {
            body["image"] = if images.len() == 1 {
                json!(images[0])
            } else {
                json!(images)
            };
        }
        let response = self
            .transport
            .execute(HttpRequest {
                method: "POST",
                url: self.endpoint.clone(),
                headers: vec![(
                    "Authorization".into(),
                    format!("Bearer {}", api_key.expose()),
                )],
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
            .get("data")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|item| item.get("b64_json").and_then(|value| value.as_str()))
            .map(|data| decode_image(data, Some("image/png")))
            .collect::<Result<Vec<_>>>()?;
        finish(self.id(), response, images)
    }
}
