use std::time::Duration;

use llm_provider::anthropic::AnthropicProvider;
use llm_provider::{ChatMessage, LlmProvider, LlmRequest};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sample_request() -> LlmRequest {
    LlmRequest {
        messages: vec![ChatMessage::system("Be concise."), ChatMessage::user("Hi")],
        model: "claude-3-5-sonnet-20241022".to_string(),
        temperature: 0.7,
        max_tokens: Some(100),
    }
}

#[tokio::test]
async fn chat_returns_response_on_success() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "test-key"))
        .and(header("anthropic-version", "2023-06-01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [
                {"type": "text", "text": "Hello from Claude!"}
            ],
            "model": "claude-3-5-sonnet-20241022",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        })))
        .mount(&server)
        .await;

    let provider = AnthropicProvider::new(&server.uri(), "test-key");
    let response = provider
        .chat(&sample_request(), CancellationToken::new())
        .await
        .expect("chat should succeed");

    assert_eq!(response.text, "Hello from Claude!");
    assert_eq!(response.model, "claude-3-5-sonnet-20241022");
    assert_eq!(response.usage_tokens, Some(15));
}

#[tokio::test]
async fn chat_sends_system_prompt_separately_from_messages() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{"type": "text", "text": "ok"}],
            "model": "claude-3-5-sonnet-20241022",
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .mount(&server)
        .await;

    let provider = AnthropicProvider::new(&server.uri(), "test-key");
    provider
        .chat(&sample_request(), CancellationToken::new())
        .await
        .expect("chat should succeed");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = requests[0].body_json().unwrap();
    assert_eq!(body["system"], "Be concise.");
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(
        messages.len(),
        1,
        "system message should not be duplicated in messages"
    );
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[0]["content"], "Hi");

    let max_tokens = body["max_tokens"]
        .as_u64()
        .expect("max_tokens must be present and numeric (Anthropic requires it)");
    assert!(max_tokens > 0, "max_tokens must be non-zero");
}

#[tokio::test]
async fn chat_returns_err_on_non_2xx_response() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": {"type": "invalid_request_error", "message": "bad request"}
        })))
        .mount(&server)
        .await;

    let provider = AnthropicProvider::new(&server.uri(), "test-key");
    let result = provider.chat(&sample_request(), CancellationToken::new()).await;

    assert!(result.is_err(), "non-2xx chat response should be an error");
}

#[tokio::test]
async fn list_models_returns_err_on_non_2xx_response() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .mount(&server)
        .await;

    let provider = AnthropicProvider::new(&server.uri(), "test-key");
    let result = provider.list_models().await;

    assert!(result.is_err(), "non-2xx list_models response should be an error");
}

#[tokio::test]
async fn list_models_returns_model_ids() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("x-api-key", "test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                {"id": "claude-3-5-sonnet-20241022"},
                {"id": "claude-3-opus-20240229"}
            ]
        })))
        .mount(&server)
        .await;

    let provider = AnthropicProvider::new(&server.uri(), "test-key");
    let models = provider
        .list_models()
        .await
        .expect("list_models should succeed");

    assert_eq!(
        models,
        vec![
            "claude-3-5-sonnet-20241022".to_string(),
            "claude-3-opus-20240229".to_string()
        ]
    );
}

#[tokio::test]
async fn is_available_true_when_models_endpoint_ok() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": []})))
        .mount(&server)
        .await;

    let provider = AnthropicProvider::new(&server.uri(), "test-key");
    assert!(provider.is_available().await);
}

#[tokio::test]
async fn is_available_false_when_unreachable() {
    let provider = AnthropicProvider::new("http://127.0.0.1:1", "test-key");
    assert!(!provider.is_available().await);
}

#[tokio::test]
async fn chat_is_cancellable_mid_flight() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({
                    "content": [{"type": "text", "text": "too slow"}],
                    "model": "claude-3-5-sonnet-20241022",
                    "usage": {"input_tokens": 1, "output_tokens": 1}
                }))
                .set_delay(Duration::from_secs(5)),
        )
        .mount(&server)
        .await;

    let provider = AnthropicProvider::new(&server.uri(), "test-key");
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel_clone.cancel();
    });

    let start = std::time::Instant::now();
    let result = provider.chat(&sample_request(), cancel).await;
    let elapsed = start.elapsed();

    assert!(result.is_err(), "cancelled request should return an error");
    assert!(
        elapsed < Duration::from_secs(2),
        "cancellation should abort promptly, took {:?}",
        elapsed
    );
}

#[tokio::test]
async fn provider_name_is_anthropic() {
    let provider = AnthropicProvider::new("http://localhost:1234", "key");
    assert_eq!(provider.provider_name(), "anthropic");
}

#[tokio::test]
async fn default_base_url_is_anthropic_api() {
    let provider = AnthropicProvider::with_default_base("test-key");
    assert_eq!(provider.provider_name(), "anthropic");
}
