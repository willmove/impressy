use base64::Engine as _;

use crate::{AiError, GeneratedImage, HttpResponse, ProviderId, Result};

pub fn decode_image(data: &str, mime_type: Option<&str>) -> Result<GeneratedImage> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| AiError::InvalidResponse)?;
    if bytes.is_empty() {
        return Err(AiError::InvalidResponse);
    }
    let mime_type = mime_type
        .filter(|value| value.starts_with("image/"))
        .unwrap_or("image/png")
        .to_string();
    Ok(GeneratedImage { bytes, mime_type })
}

pub fn request_id(response: &HttpResponse) -> Option<String> {
    response
        .headers
        .iter()
        .find(|(name, _)| {
            name.eq_ignore_ascii_case("x-request-id")
                || name.eq_ignore_ascii_case("x-goog-request-id")
        })
        .map(|(_, value)| value.clone())
}

pub fn map_http_error(status: u16, body: &[u8]) -> AiError {
    let value: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let error = value.get("error").unwrap_or(&value);
    let code = error
        .get("code")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let message = error
        .get("message")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let combined = format!("{code} {message}");

    if status == 401 || combined.contains("api key") || combined.contains("credential") {
        AiError::InvalidCredentials
    } else if combined.contains("balance")
        || combined.contains("overdue")
        || combined.contains("billing")
    {
        AiError::InsufficientBalance
    } else if status == 429
        || combined.contains("rate limit")
        || combined.contains("resource_exhausted")
    {
        AiError::RateLimited
    } else if combined.contains("content")
        || combined.contains("safety")
        || combined.contains("policy")
        || combined.contains("moderation")
        || combined.contains("blocked")
    {
        AiError::ContentBlocked
    } else if status >= 500 {
        AiError::ProviderUnavailable
    } else {
        AiError::InvalidRequest(if message.is_empty() {
            format!("provider returned HTTP {status}")
        } else {
            message
        })
    }
}

pub fn finish(
    provider: ProviderId,
    response: HttpResponse,
    images: Vec<GeneratedImage>,
) -> Result<crate::GenerationResult> {
    if !(200..300).contains(&response.status) {
        return Err(map_http_error(response.status, &response.body));
    }
    if images.is_empty() {
        return Err(AiError::ContentBlocked);
    }
    Ok(crate::GenerationResult {
        images,
        provider,
        request_id: request_id(&response),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_vendor_errors_to_stable_semantics() {
        assert!(matches!(
            map_http_error(401, br#"{"error":{"message":"invalid API key"}}"#),
            AiError::InvalidCredentials
        ));
        assert!(matches!(
            map_http_error(402, br#"{"error":{"message":"billing balance exhausted"}}"#),
            AiError::InsufficientBalance
        ));
        assert!(matches!(map_http_error(429, b"{}"), AiError::RateLimited));
        assert!(matches!(
            map_http_error(400, br#"{"error":{"message":"blocked by safety policy"}}"#),
            AiError::ContentBlocked
        ));
        assert!(matches!(
            map_http_error(503, b"{}"),
            AiError::ProviderUnavailable
        ));
    }

    #[test]
    fn successful_response_without_images_is_a_blocked_zero_image_result() {
        let response = HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
        };
        assert!(matches!(
            finish(ProviderId::OpenAi, response, Vec::new()),
            Err(AiError::ContentBlocked)
        ));
    }
}
