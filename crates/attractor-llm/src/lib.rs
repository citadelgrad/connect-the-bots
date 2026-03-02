//! Unified LLM client with multi-provider support (OpenAI, Anthropic, Gemini).
//!
//! Provides `ProviderAdapter` trait, `DynProvider` wrapper, middleware chain,
//! model catalog, and `LlmClient` for routing requests by provider.

#[cfg(feature = "providers")]
mod anthropic;
#[cfg(feature = "providers")]
mod client;
#[cfg(feature = "providers")]
mod gemini;
#[cfg(feature = "providers")]
mod openai;
#[cfg(feature = "providers")]
mod provider;
mod types;

#[cfg(feature = "providers")]
pub use anthropic::AnthropicAdapter;
#[cfg(feature = "providers")]
pub use client::*;
#[cfg(feature = "providers")]
pub use gemini::GeminiAdapter;
#[cfg(feature = "providers")]
pub use openai::OpenAiAdapter;
#[cfg(feature = "providers")]
pub use provider::*;
pub use types::*;
