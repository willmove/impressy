//! Unified AI image provider layer for Impressy v2.
//!
//! The crate is synchronous by design. `impressy-app` runs calls on GPUI's background
//! executor, keeping networking out of the UI thread without introducing another runtime.

mod credentials;
mod error;
mod providers;
mod registry;
mod transport;
mod types;

pub use credentials::{CredentialStore, MemoryCredentialStore, SystemCredentialStore};
pub use error::{AiError, Result};
pub use providers::{GeminiProvider, OpenAiProvider, SeedreamProvider};
pub use registry::ProviderRegistry;
pub use transport::{HttpRequest, HttpResponse, HttpTransport, TransportError, UreqTransport};
pub use types::{
    ApiKey, AspectRatio, GeneratedImage, GenerationQuality, GenerationRequest, GenerationResult,
    ImageInput, Provider, ProviderCapabilities, ProviderId,
};
