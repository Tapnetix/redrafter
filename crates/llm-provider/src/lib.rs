pub mod anthropic;
pub mod discover;
pub mod gemini;
pub mod ollama;
pub mod openai_compat;
pub mod provider;
pub mod request;

pub use anthropic::AnthropicProvider;
pub use discover::{discover, DiscoveryResult};
pub use gemini::GeminiProvider;
pub use ollama::OllamaProvider;
pub use openai_compat::OpenAiCompatProvider;
pub use provider::LlmProvider;
pub use request::{ChatMessage, LlmRequest, LlmResponse};
