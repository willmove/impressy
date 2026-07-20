use std::time::Duration;

/// HTTP request intentionally omits `Debug` so authorization headers cannot be logged accidentally.
pub struct HttpRequest {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub content_type: String,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("only HTTPS provider endpoints are allowed")]
    InsecureEndpoint,
    #[error("HTTP transport failed")]
    Failed,
}

pub trait HttpTransport: Send + Sync {
    fn execute(&self, request: HttpRequest) -> std::result::Result<HttpResponse, TransportError>;
}

#[derive(Clone)]
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl Default for UreqTransport {
    fn default() -> Self {
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(180)))
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
        }
    }
}

impl HttpTransport for UreqTransport {
    fn execute(&self, request: HttpRequest) -> std::result::Result<HttpResponse, TransportError> {
        if !request.url.starts_with("https://") {
            return Err(TransportError::InsecureEndpoint);
        }
        if request.method != "POST" {
            return Err(TransportError::Failed);
        }

        let mut builder = self
            .agent
            .post(&request.url)
            .header("Content-Type", &request.content_type)
            .header("User-Agent", "rastery/0.1");
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        let mut response = builder
            .send(request.body)
            .map_err(|_| TransportError::Failed)?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.to_string(), value.to_string()))
            })
            .collect();
        let body = response
            .body_mut()
            .with_config()
            .limit(128 * 1024 * 1024)
            .read_to_vec()
            .map_err(|_| TransportError::Failed)?;
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_rejects_plain_http_before_io() {
        let request = HttpRequest {
            method: "POST",
            url: "http://example.invalid".into(),
            headers: Vec::new(),
            content_type: "application/json".into(),
            body: Vec::new(),
        };
        assert!(matches!(
            UreqTransport::default().execute(request),
            Err(TransportError::InsecureEndpoint)
        ));
    }
}
