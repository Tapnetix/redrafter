use std::time::Duration;

use llm_provider::openai_compat::OpenAiCompatProvider;
use llm_provider::{ChatMessage, LlmProvider, LlmRequest};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sample_request() -> LlmRequest {
    LlmRequest {
        messages: vec![ChatMessage::system("Be concise."), ChatMessage::user("Hi")],
        model: "gpt-4o-mini".to_string(),
        temperature: Some(0.7),
        max_tokens: Some(100),
    }
}

#[tokio::test]
async fn chat_returns_response_on_success() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [
                {"message": {"role": "assistant", "content": "Hello from OpenAI!"}}
            ],
            "model": "gpt-4o-mini",
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        })))
        .mount(&server)
        .await;

    let provider = OpenAiCompatProvider::new(&server.uri(), "test-key");
    let response = provider
        .chat(&sample_request(), CancellationToken::new())
        .await
        .expect("chat should succeed");

    assert_eq!(response.text, "Hello from OpenAI!");
    assert_eq!(response.model, "gpt-4o-mini");
    assert_eq!(response.usage_tokens, Some(15));
}

#[tokio::test]
async fn list_models_returns_model_ids() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                {"id": "gpt-4o-mini"},
                {"id": "gpt-3.5-turbo"}
            ]
        })))
        .mount(&server)
        .await;

    let provider = OpenAiCompatProvider::new(&server.uri(), "test-key");
    let models = provider
        .list_models()
        .await
        .expect("list_models should succeed");

    assert_eq!(
        models,
        vec!["gpt-4o-mini".to_string(), "gpt-3.5-turbo".to_string()]
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

    let provider = OpenAiCompatProvider::new(&server.uri(), "test-key");
    assert!(provider.is_available().await);
}

#[tokio::test]
async fn is_available_false_when_unreachable() {
    // Nothing is listening on this port.
    let provider = OpenAiCompatProvider::new("http://127.0.0.1:1", "test-key");
    assert!(!provider.is_available().await);
}

#[tokio::test]
async fn chat_is_cancellable_mid_flight() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({
                    "choices": [{"message": {"role": "assistant", "content": "too slow"}}],
                    "model": "gpt-4o-mini",
                    "usage": null
                }))
                .set_delay(Duration::from_secs(5)),
        )
        .mount(&server)
        .await;

    let provider = OpenAiCompatProvider::new(&server.uri(), "test-key");
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    // Cancel shortly after issuing the request, well before the 5s delay elapses.
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
async fn provider_name_is_openai_compatible() {
    let provider = OpenAiCompatProvider::new("http://localhost:1234", "key");
    assert_eq!(provider.provider_name(), "openai-compatible");
}
