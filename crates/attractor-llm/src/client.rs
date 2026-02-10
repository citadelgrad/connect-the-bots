use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use attractor_types::AttractorError;

use crate::{DynProvider, ProviderAdapter, Request, Response};

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

pub trait Middleware: Send + Sync {
    fn before(&self, _request: &mut Request) {}
    fn after(&self, _request: &Request, _response: &mut Response) {}
}

// ---------------------------------------------------------------------------
// Built-in middleware: LoggingMiddleware
// ---------------------------------------------------------------------------

pub struct LoggingMiddleware;

impl Middleware for LoggingMiddleware {
    fn before(&self, request: &mut Request) {
        tracing::info!(
            model = %request.model,
            messages = request.messages.len(),
            "LLM request"
        );
    }

    fn after(&self, _request: &Request, response: &mut Response) {
        tracing::info!(
            model = %response.model,
            input_tokens = response.usage.input_tokens,
            output_tokens = response.usage.output_tokens,
            finish = ?response.finish_reason,
            "LLM response"
        );
    }
}

// ---------------------------------------------------------------------------
// Built-in middleware: CostTrackingMiddleware
// ---------------------------------------------------------------------------

pub struct CostTrackingMiddleware {
    total_input: Arc<AtomicU64>,
    total_output: Arc<AtomicU64>,
}

impl CostTrackingMiddleware {
    pub fn new() -> Self {
        Self {
            total_input: Arc::new(AtomicU64::new(0)),
            total_output: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn total_input_tokens(&self) -> u64 {
        self.total_input.load(Ordering::Relaxed)
    }

    pub fn total_output_tokens(&self) -> u64 {
        self.total_output.load(Ordering::Relaxed)
    }
}

impl Default for CostTrackingMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for CostTrackingMiddleware {
    fn after(&self, _request: &Request, response: &mut Response) {
        self.total_input
            .fetch_add(response.usage.input_tokens, Ordering::Relaxed);
        self.total_output
            .fetch_add(response.usage.output_tokens, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// ModelInfo / ModelCatalog
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub provider: String,
    pub context_window: usize,
    pub supports_tools: bool,
    pub supports_reasoning: bool,
}

pub struct ModelCatalog {
    models: HashMap<String, ModelInfo>,
}

impl ModelCatalog {
    pub fn new() -> Self {
        let mut models = HashMap::new();

        // Claude models
        for (id, ctx, reasoning) in [
            ("claude-opus-4-6", 200_000, true),
            ("claude-sonnet-4-5-20250929", 200_000, true),
            ("claude-haiku-4-5-20251001", 200_000, false),
        ] {
            models.insert(
                id.to_string(),
                ModelInfo {
                    id: id.to_string(),
                    provider: "anthropic".to_string(),
                    context_window: ctx,
                    supports_tools: true,
                    supports_reasoning: reasoning,
                },
            );
        }

        // GPT models
        for (id, ctx, reasoning) in [
            ("gpt-4o", 128_000, false),
            ("gpt-4o-mini", 128_000, false),
            ("o1", 200_000, true),
            ("o3-mini", 200_000, true),
        ] {
            models.insert(
                id.to_string(),
                ModelInfo {
                    id: id.to_string(),
                    provider: "openai".to_string(),
                    context_window: ctx,
                    supports_tools: true,
                    supports_reasoning: reasoning,
                },
            );
        }

        // Gemini models
        for (id, ctx) in [
            ("gemini-2.5-pro", 1_000_000),
            ("gemini-2.5-flash", 1_000_000),
        ] {
            models.insert(
                id.to_string(),
                ModelInfo {
                    id: id.to_string(),
                    provider: "google".to_string(),
                    context_window: ctx,
                    supports_tools: true,
                    supports_reasoning: true,
                },
            );
        }

        Self { models }
    }

    pub fn lookup(&self, model: &str) -> Option<&ModelInfo> {
        self.models.get(model)
    }

    pub fn provider_for_model(&self, model: &str) -> Option<&str> {
        self.models.get(model).map(|m| m.provider.as_str())
    }
}

impl Default for ModelCatalog {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// LlmClient
// ---------------------------------------------------------------------------

pub struct LlmClient {
    providers: HashMap<String, DynProvider>,
    model_catalog: ModelCatalog,
    middleware: Vec<Box<dyn Middleware>>,
}

impl LlmClient {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            model_catalog: ModelCatalog::new(),
            middleware: Vec::new(),
        }
    }

    pub fn register_provider(&mut self, provider: impl ProviderAdapter + 'static) {
        let name = provider.name().to_string();
        self.providers.insert(name, DynProvider::new(provider));
    }

    pub fn with_middleware(mut self, m: impl Middleware + 'static) -> Self {
        self.middleware.push(Box::new(m));
        self
    }

    pub fn model_catalog(&self) -> &ModelCatalog {
        &self.model_catalog
    }

    pub async fn complete(&self, request: &Request) -> Result<Response, AttractorError> {
        let provider = self.resolve_provider(request)?;
        let mut req = request.clone();

        for m in &self.middleware {
            m.before(&mut req);
        }

        let mut resp = provider.complete(&req).await?;

        for m in &self.middleware {
            m.after(&req, &mut resp);
        }

        Ok(resp)
    }

    fn resolve_provider(&self, request: &Request) -> Result<&DynProvider, AttractorError> {
        // 1. Explicit provider field
        if let Some(ref provider_name) = request.provider {
            return self.providers.get(provider_name).ok_or_else(|| {
                AttractorError::Other(format!("Provider '{}' not registered", provider_name))
            });
        }

        // 2. Model catalog lookup
        if let Some(provider_name) = self.model_catalog.provider_for_model(&request.model) {
            if let Some(provider) = self.providers.get(provider_name) {
                return Ok(provider);
            }
        }

        // 3. Try each registered provider (return the first one)
        if let Some(provider) = self.providers.values().next() {
            return Ok(provider);
        }

        Err(AttractorError::Other(
            "No providers registered".to_string(),
        ))
    }

    /// Create from environment variables (detect available API keys).
    pub fn from_env() -> Result<Self, AttractorError> {
        let mut client = Self::new();
        let mut found_any = false;

        if let Ok(adapter) = crate::AnthropicAdapter::from_env() {
            client.register_provider(adapter);
            found_any = true;
        }

        if let Ok(adapter) = crate::OpenAiAdapter::from_env() {
            client.register_provider(adapter);
            found_any = true;
        }

        if let Ok(adapter) = crate::GeminiAdapter::from_env() {
            client.register_provider(adapter);
            found_any = true;
        }

        if !found_any {
            return Err(AttractorError::Other(
                "No LLM provider API keys found in environment".to_string(),
            ));
        }

        Ok(client)
    }
}

impl Default for LlmClient {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FinishReason, Message, ProviderAdapter, StreamEvent, Usage};
    use async_trait::async_trait;
    use futures_core::Stream;
    use std::pin::Pin;
    use std::sync::atomic::AtomicUsize;

    struct MockProvider {
        call_count: Arc<AtomicUsize>,
    }

    impl MockProvider {
        fn new() -> Self {
            Self {
                call_count: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    #[async_trait]
    impl ProviderAdapter for MockProvider {
        async fn complete(&self, _request: &Request) -> Result<Response, AttractorError> {
            self.call_count.fetch_add(1, Ordering::Relaxed);
            Ok(Response {
                id: "mock-resp".into(),
                text: "Hello from mock".into(),
                tool_calls: vec![],
                reasoning: None,
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 20,
                    reasoning_tokens: None,
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    total_tokens: 30,
                },
                model: "mock-model".into(),
                finish_reason: FinishReason::EndTurn,
            })
        }

        fn stream(
            &self,
            _request: &Request,
        ) -> Pin<Box<dyn Stream<Item = StreamEvent> + Send + '_>> {
            Box::pin(tokio_stream::empty::<StreamEvent>())
        }

        fn name(&self) -> &str {
            "mock"
        }

        fn default_model(&self) -> &str {
            "mock-model"
        }

        fn supports_tools(&self) -> bool {
            true
        }

        fn supports_streaming(&self) -> bool {
            true
        }

        fn supports_reasoning(&self) -> bool {
            false
        }

        fn context_window_size(&self) -> usize {
            128_000
        }
    }

    fn make_request(model: &str, provider: Option<&str>) -> Request {
        Request {
            model: model.into(),
            messages: vec![Message::user("hello")],
            tools: vec![],
            tool_choice: None,
            max_tokens: None,
            temperature: None,
            stop_sequences: vec![],
            reasoning_effort: None,
            provider: provider.map(String::from),
            provider_options: None,
        }
    }

    // Test 1: register_provider and resolve
    #[tokio::test]
    async fn register_provider_and_complete() {
        let mut client = LlmClient::new();
        client.register_provider(MockProvider::new());

        let req = make_request("mock-model", Some("mock"));
        let resp = client.complete(&req).await.unwrap();
        assert_eq!(resp.id, "mock-resp");
        assert_eq!(resp.text, "Hello from mock");
    }

    // Test 2: model catalog lookup
    #[test]
    fn model_catalog_lookup() {
        let catalog = ModelCatalog::new();

        let info = catalog.lookup("claude-opus-4-6").unwrap();
        assert_eq!(info.provider, "anthropic");
        assert_eq!(info.context_window, 200_000);
        assert!(info.supports_tools);
        assert!(info.supports_reasoning);

        let info = catalog.lookup("gpt-4o").unwrap();
        assert_eq!(info.provider, "openai");
        assert_eq!(info.context_window, 128_000);
        assert!(!info.supports_reasoning);

        let info = catalog.lookup("gemini-2.5-pro").unwrap();
        assert_eq!(info.provider, "google");
        assert_eq!(info.context_window, 1_000_000);

        assert!(catalog.lookup("nonexistent-model").is_none());
    }

    // Test 3: provider resolution by model name (via catalog)
    #[tokio::test]
    async fn resolve_provider_by_model_name() {
        let mut client = LlmClient::new();

        // Register a provider named "anthropic" so the catalog lookup finds it
        struct AnthropicMock;

        #[async_trait]
        impl ProviderAdapter for AnthropicMock {
            async fn complete(&self, _request: &Request) -> Result<Response, AttractorError> {
                Ok(Response {
                    id: "anthropic-resp".into(),
                    text: "Hello from anthropic mock".into(),
                    tool_calls: vec![],
                    reasoning: None,
                    usage: Usage::default(),
                    model: "claude-opus-4-6".into(),
                    finish_reason: FinishReason::EndTurn,
                })
            }
            fn stream(
                &self,
                _request: &Request,
            ) -> Pin<Box<dyn Stream<Item = StreamEvent> + Send + '_>> {
                Box::pin(tokio_stream::empty::<StreamEvent>())
            }
            fn name(&self) -> &str {
                "anthropic"
            }
            fn default_model(&self) -> &str {
                "claude-opus-4-6"
            }
            fn supports_tools(&self) -> bool {
                true
            }
            fn supports_streaming(&self) -> bool {
                true
            }
            fn supports_reasoning(&self) -> bool {
                true
            }
            fn context_window_size(&self) -> usize {
                200_000
            }
        }

        client.register_provider(AnthropicMock);

        // Request with no explicit provider, but model is in catalog -> anthropic
        let req = make_request("claude-opus-4-6", None);
        let resp = client.complete(&req).await.unwrap();
        assert_eq!(resp.id, "anthropic-resp");
    }

    // Test 4: middleware before/after called
    #[tokio::test]
    async fn middleware_before_after_called() {
        let before_count = Arc::new(AtomicUsize::new(0));
        let after_count = Arc::new(AtomicUsize::new(0));

        struct CountingMiddleware {
            before_count: Arc<AtomicUsize>,
            after_count: Arc<AtomicUsize>,
        }

        impl Middleware for CountingMiddleware {
            fn before(&self, _request: &mut Request) {
                self.before_count.fetch_add(1, Ordering::Relaxed);
            }
            fn after(&self, _request: &Request, _response: &mut Response) {
                self.after_count.fetch_add(1, Ordering::Relaxed);
            }
        }

        let bc = before_count.clone();
        let ac = after_count.clone();

        let mut client = LlmClient::new().with_middleware(CountingMiddleware {
            before_count: bc,
            after_count: ac,
        });
        client.register_provider(MockProvider::new());

        let req = make_request("mock-model", Some("mock"));
        let _resp = client.complete(&req).await.unwrap();

        assert_eq!(before_count.load(Ordering::Relaxed), 1);
        assert_eq!(after_count.load(Ordering::Relaxed), 1);
    }

    // Test 5: from_env with no keys returns error
    #[test]
    fn from_env_with_no_keys_returns_error() {
        // Ensure no provider keys are set
        std::env::remove_var("ANTHROPIC_API_KEY");
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var("GOOGLE_API_KEY");

        let result = LlmClient::from_env();
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("No LLM provider API keys found"));
    }

    // Test: resolve_provider returns error when provider not registered
    #[test]
    fn resolve_provider_unknown_returns_error() {
        let client = LlmClient::new();
        let req = make_request("some-model", Some("nonexistent"));
        let result = client.resolve_provider(&req);
        assert!(result.is_err());
    }

    // Test: resolve_provider falls back to first registered provider
    #[tokio::test]
    async fn resolve_provider_fallback_to_first() {
        let mut client = LlmClient::new();
        client.register_provider(MockProvider::new());

        // Unknown model, no explicit provider -> fallback to first registered
        let req = make_request("unknown-model", None);
        let resp = client.complete(&req).await.unwrap();
        assert_eq!(resp.text, "Hello from mock");
    }

    // Test: no providers registered returns error
    #[test]
    fn no_providers_returns_error() {
        let client = LlmClient::new();
        let req = make_request("some-model", None);
        let result = client.resolve_provider(&req);
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("No providers"));
    }

    // Test: CostTrackingMiddleware accumulates tokens
    #[tokio::test]
    async fn cost_tracking_middleware() {
        let cost = Arc::new(CostTrackingMiddleware::new());
        let cost_clone = CostTrackingMiddleware {
            total_input: cost.total_input.clone(),
            total_output: cost.total_output.clone(),
        };

        let mut client = LlmClient::new().with_middleware(cost_clone);
        client.register_provider(MockProvider::new());

        let req = make_request("mock-model", Some("mock"));
        let _resp = client.complete(&req).await.unwrap();

        assert_eq!(cost.total_input_tokens(), 10);
        assert_eq!(cost.total_output_tokens(), 20);

        // Second call accumulates
        let _resp = client.complete(&req).await.unwrap();
        assert_eq!(cost.total_input_tokens(), 20);
        assert_eq!(cost.total_output_tokens(), 40);
    }

    // Test: model catalog provider_for_model
    #[test]
    fn model_catalog_provider_for_model() {
        let catalog = ModelCatalog::new();
        assert_eq!(
            catalog.provider_for_model("claude-opus-4-6"),
            Some("anthropic")
        );
        assert_eq!(catalog.provider_for_model("gpt-4o"), Some("openai"));
        assert_eq!(
            catalog.provider_for_model("gemini-2.5-pro"),
            Some("google")
        );
        assert_eq!(catalog.provider_for_model("unknown"), None);
    }
}
