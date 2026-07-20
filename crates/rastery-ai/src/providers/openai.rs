use std::sync::Arc;

use serde_json::json;

use super::shared::{decode_image, finish, map_http_error};
use crate::{
    AiError, ApiKey, AspectRatio, GenerationRequest, GenerationResult, HttpRequest, HttpTransport,
    Provider, ProviderCapabilities, ProviderId, Result, UreqTransport,
};

const RATIOS: &[AspectRatio] = &AspectRatio::COMMON;

pub struct OpenAiProvider {
    transport: Arc<dyn HttpTransport>,
    generation_endpoint: String,
    edit_endpoint: String,
    model: String,
}

impl Default for OpenAiProvider {
    fn default() -> Self {
        Self::new(Arc::new(UreqTransport::default()))
    }
}

impl OpenAiProvider {
    pub fn new(transport: Arc<dyn HttpTransport>) -> Self {
        Self {
            transport,
            generation_endpoint: "https://api.openai.com/v1/images/generations".into(),
            edit_endpoint: "https://api.openai.com/v1/images/edits".into(),
            model: "gpt-image-2".into(),
        }
    }

    fn multipart(&self, request: &GenerationRequest) -> (String, Vec<u8>) {
        let boundary = "rastery-gpt-image-boundary";
        let mut body = Vec::new();
        let mut text = |name: &str, value: &str| {
            body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
        };
        text("model", &self.model);
        text("prompt", &request.prompt);
        text("size", request.aspect_ratio.openai_size());
        text("quality", request.quality.as_str());
        text("n", &request.count.to_string());
        text("output_format", "png");
        for (index, image) in request.reference_images.iter().enumerate() {
            body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"image[]\"; filename=\"reference-{index}.png\"\r\nContent-Type: {}\r\n\r\n", image.mime_type).as_bytes());
            body.extend_from_slice(&image.bytes);
            body.extend_from_slice(b"\r\n");
        }
        if let Some(mask) = &request.mask {
            body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"mask\"; filename=\"mask.png\"\r\nContent-Type: {}\r\n\r\n", mask.mime_type).as_bytes());
            body.extend_from_slice(&mask.bytes);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        (format!("multipart/form-data; boundary={boundary}"), body)
    }
}

impl Provider for OpenAiProvider {
    fn id(&self) -> ProviderId {
        ProviderId::OpenAi
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            max_reference_images: 4,
            supported_aspect_ratios: RATIOS,
            max_generation_count: 10,
            text_to_image: true,
            image_to_image: true,
            region_edit: true,
            max_input_image_bytes: 50 * 1024 * 1024,
        }
    }

    fn generate(&self, request: &GenerationRequest, api_key: &ApiKey) -> Result<GenerationResult> {
        self.capabilities().validate(request)?;
        let (url, content_type, body) = if request.reference_images.is_empty()
            && request.mask.is_none()
        {
            let body = json!({
                "model": self.model,
                "prompt": request.prompt,
                "size": request.aspect_ratio.openai_size(),
                "quality": request.quality.as_str(),
                "n": request.count,
                "output_format": "png"
            });
            (
                self.generation_endpoint.clone(),
                "application/json".to_string(),
                serde_json::to_vec(&body).map_err(|_| AiError::InvalidRequest("JSON".into()))?,
            )
        } else {
            let (content_type, body) = self.multipart(request);
            (self.edit_endpoint.clone(), content_type, body)
        };
        let response = self
            .transport
            .execute(HttpRequest {
                method: "POST",
                url,
                headers: vec![(
                    "Authorization".into(),
                    format!("Bearer {}", api_key.expose()),
                )],
                content_type,
                body,
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
