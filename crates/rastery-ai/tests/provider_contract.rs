use std::sync::{Arc, Mutex};

use rastery_ai::{
    ApiKey, AspectRatio, GeminiProvider, GenerationRequest, HttpRequest, HttpResponse,
    HttpTransport, ImageInput, OpenAiProvider, Provider, SeedreamProvider, TransportError,
};

#[derive(Clone)]
struct RecordedRequest {
    url: String,
    content_type: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

struct RecordingTransport {
    response: HttpResponse,
    request: Mutex<Option<RecordedRequest>>,
}

impl RecordingTransport {
    fn new(body: &str) -> Arc<Self> {
        Arc::new(Self {
            response: HttpResponse {
                status: 200,
                headers: vec![("x-request-id".into(), "req-test".into())],
                body: body.as_bytes().to_vec(),
            },
            request: Mutex::new(None),
        })
    }

    fn take(&self) -> RecordedRequest {
        self.request
            .lock()
            .expect("request lock")
            .take()
            .expect("request")
    }
}

impl HttpTransport for RecordingTransport {
    fn execute(&self, request: HttpRequest) -> Result<HttpResponse, TransportError> {
        *self.request.lock().map_err(|_| TransportError::Failed)? = Some(RecordedRequest {
            url: request.url,
            content_type: request.content_type,
            headers: request.headers,
            body: request.body,
        });
        Ok(self.response.clone())
    }
}

fn key() -> ApiKey {
    ApiKey::new("provider-secret").expect("key")
}

#[test]
fn seedream_serializes_image_to_image_contract() {
    let transport = RecordingTransport::new(r#"{"data":[{"b64_json":"aW1n"}]}"#);
    let provider = SeedreamProvider::new(transport.clone());
    let mut request = GenerationRequest::text("preserve the subject");
    request.aspect_ratio = AspectRatio::PortraitThreeFour;
    request.reference_images = vec![ImageInput::new(vec![1, 2, 3], "image/png").expect("image")];

    let result = provider.generate(&request, &key()).expect("generation");
    assert_eq!(result.images[0].bytes, b"img");
    let recorded = transport.take();
    assert!(recorded.url.starts_with("https://"));
    let json: serde_json::Value = serde_json::from_slice(&recorded.body).expect("JSON body");
    assert_eq!(json["size"], "864x1152");
    assert!(
        json["image"]
            .as_str()
            .is_some_and(|value| value.starts_with("data:image/png;base64,"))
    );
    assert!(!String::from_utf8_lossy(&recorded.body).contains("provider-secret"));
}

#[test]
fn nano_banana_parses_interactions_image_blocks() {
    let transport = RecordingTransport::new(
        r#"{"steps":[{"type":"model_output","content":[{"type":"image","mime_type":"image/png","data":"aW1n"}]}]}"#,
    );
    let provider = GeminiProvider::new(transport.clone());
    let mut request = GenerationRequest::text("series illustration");
    request.aspect_ratio = AspectRatio::LandscapeSixteenNine;

    let result = provider.generate(&request, &key()).expect("generation");
    assert_eq!(result.images.len(), 1);
    let recorded = transport.take();
    assert_eq!(recorded.content_type, "application/json");
    let json: serde_json::Value = serde_json::from_slice(&recorded.body).expect("JSON body");
    assert_eq!(json["response_format"]["aspect_ratio"], "16:9");
    assert!(
        recorded
            .headers
            .iter()
            .any(|(name, _)| name == "x-goog-api-key")
    );
}

#[test]
fn openai_uses_multipart_edits_for_reference_images() {
    let transport = RecordingTransport::new(r#"{"data":[{"b64_json":"aW1n"}]}"#);
    let provider = OpenAiProvider::new(transport.clone());
    let mut request = GenerationRequest::text("remove selected item");
    request.reference_images = vec![ImageInput::new(vec![1, 2, 3], "image/png").expect("image")];
    request.mask = Some(ImageInput::new(vec![4, 5, 6], "image/png").expect("mask"));

    let result = provider.generate(&request, &key()).expect("generation");
    assert_eq!(result.request_id.as_deref(), Some("req-test"));
    let recorded = transport.take();
    assert!(recorded.url.ends_with("/v1/images/edits"));
    assert!(
        recorded
            .content_type
            .starts_with("multipart/form-data; boundary=")
    );
    let body = String::from_utf8_lossy(&recorded.body);
    assert!(body.contains("name=\"mask\""));
    assert!(body.contains("name=\"image[]\""));
    assert!(!body.contains("provider-secret"));
}

#[test]
fn capabilities_cover_dynamic_ui_constraints() {
    let seedream = SeedreamProvider::new(RecordingTransport::new("{}"));
    let gemini = GeminiProvider::new(RecordingTransport::new("{}"));
    let openai = OpenAiProvider::new(RecordingTransport::new("{}"));
    assert_eq!(seedream.capabilities().max_reference_images, 10);
    assert_eq!(gemini.capabilities().max_reference_images, 14);
    assert_eq!(openai.capabilities().max_generation_count, 10);
    assert!(openai.capabilities().region_edit);
    assert!(!seedream.capabilities().region_edit);
}
