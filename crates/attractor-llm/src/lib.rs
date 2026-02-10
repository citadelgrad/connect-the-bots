//! Unified LLM client with multi-provider support (OpenAI, Anthropic, Gemini).
//!
//! Provides `ProviderAdapter` trait, `DynProvider` wrapper, middleware chain,
//! model catalog, and `LlmClient` for routing requests by provider.

mod anthropic;
mod client;
mod gemini;
mod openai;
mod provider;
mod types;

pub use anthropic::AnthropicAdapter;
pub use client::*;
pub use gemini::GeminiAdapter;
pub use openai::OpenAiAdapter;
pub use provider::*;
pub use types::*;
