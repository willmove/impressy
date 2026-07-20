#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("an API key is required")]
    ApiKeyRequired,
    #[error("the API key is invalid")]
    InvalidCredentials,
    #[error("the provider account has insufficient balance")]
    InsufficientBalance,
    #[error("the provider rate limit was exceeded")]
    RateLimited,
    #[error("the request was blocked by the provider content policy")]
    ContentBlocked,
    #[error("network request failed")]
    Network,
    #[error("provider service is unavailable")]
    ProviderUnavailable,
    #[error("provider returned an invalid response")]
    InvalidResponse,
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("provider does not support {0}")]
    UnsupportedCapability(&'static str),
    #[error("too many reference images: maximum {maximum}, got {actual}")]
    TooManyReferenceImages { maximum: u8, actual: usize },
    #[error("input image exceeds the provider limit of {maximum_bytes} bytes")]
    ImageTooLarge { maximum_bytes: usize },
    #[error("credential store is unavailable")]
    CredentialStore,
    #[error("no credential is stored for this provider")]
    CredentialMissing,
}

pub type Result<T> = std::result::Result<T, AiError>;
