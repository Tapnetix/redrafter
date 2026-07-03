use std::time::Duration;

use llm_provider::gemini::GeminiProvider;
use llm_provider::{ChatMessage, LlmProvider, LlmRequest};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sample_request() -> LlmRequest {
    LlmRequest {
        messages: vec![ChatMessage::system("Be concise."), ChatMessage::user("Hi")],
        model: "gemini-1.5-flash".to_string(),
        temperature: 0.7,
        max_tokens: Some(100),
    }
}

#[tokio::test]
async fn chat_returns_response_on_success() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-1.5-flash:generateContent"))
        .and(query_param("key", "test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "candidates": [
                {
                    "content": {
                        "parts": [{"text": "Hello from Gemini!"}],
                        "role": "model"
                    }
                }
            ],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5,
                "totalTokenCount": 15
            }
        })))
        .mount(&server)
        .await;

    let provider = GeminiProvider::new(&server.uri(), "test-key");
    let response = provider
        .chat(&sample_request(), CancellationToken::new())
        .await
        .expect("chat should succeed");

    assert_eq!(response.text, "Hello from Gemini!");
    assert_eq!(response.model, "gemini-1.5-flash");
    assert_eq!(response.usage_tokens, Some(15));
}

#[tokio::test]
async fn chat_sends_system_instruction_separately_from_contents() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-1.5-flash:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "candidates": [
                {"content": {"parts": [{"text": "ok"}], "role": "model"}}
            ]
        })))
        .mount(&server)
        .await;

    let provider = GeminiProvider::new(&server.uri(), "test-key");
    provider
        .chat(&sample_request(), CancellationToken::new())
        .await
        .expect("chat should succeed");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = requests[0].body_json().unwrap();
    assert_eq!(body["systemInstruction"]["parts"][0]["text"], "Be concise.");
    let contents = body["contents"].as_array().unwrap();
    assert_eq!(
        contents.len(),
        1,
        "system message should not be duplicated in contents"
    );
    assert_eq!(contents[0]["role"], "user");
    assert_eq!(contents[0]["parts"][0]["text"], "Hi");
}

#[tokio::test]
async fn chat_returns_err_on_non_2xx_response() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-1.5-flash:generateContent"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": {"code": 400, "message": "bad request"}
        })))
        .mount(&server)
        .await;

    let provider = GeminiProvider::new(&server.uri(), "test-key");
    let result = provider.chat(&sample_request(), CancellationToken::new()).await;

    assert!(result.is_err(), "non-2xx chat response should be an error");
}

#[tokio::test]
async fn list_models_returns_err_on_non_2xx_response() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1beta/models"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .mount(&server)
        .await;

    let provider = GeminiProvider::new(&server.uri(), "test-key");
    let result = provider.list_models().await;

    assert!(result.is_err(), "non-2xx list_models response should be an error");
}

#[tokio::test]
async fn list_models_returns_model_ids_stripped_of_prefix() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1beta/models"))
        .and(query_param("key", "test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "models": [
                {"name": "models/gemini-1.5-flash"},
                {"name": "models/gemini-1.5-pro"}
            ]
        })))
        .mount(&server)
        .await;

    let provider = GeminiProvider::new(&server.uri(), "test-key");
    let models = provider
        .list_models()
        .await
        .expect("list_models should succeed");

    assert_eq!(
        models,
        vec!["gemini-1.5-flash".to_string(), "gemini-1.5-pro".to_string()]
    );
}

#[tokio::test]
async fn is_available_true_when_models_endpoint_ok() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1beta/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"models": []})))
        .mount(&server)
        .await;

    let provider = GeminiProvider::new(&server.uri(), "test-key");
    assert!(provider.is_available().await);
}

#[tokio::test]
async fn is_available_false_when_unreachable() {
    let provider = GeminiProvider::new("http://127.0.0.1:1", "test-key");
    assert!(!provider.is_available().await);
}

#[tokio::test]
async fn chat_is_cancellable_mid_flight() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-1.5-flash:generateContent"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({
                    "candidates": [
                        {"content": {"parts": [{"text": "too slow"}], "role": "model"}}
                    ]
                }))
                .set_delay(Duration::from_secs(5)),
        )
        .mount(&server)
        .await;

    let provider = GeminiProvider::new(&server.uri(), "test-key");
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
async fn provider_name_is_gemini() {
    let provider = GeminiProvider::new("http://localhost:1234", "key");
    assert_eq!(provider.provider_name(), "gemini");
}

#[tokio::test]
async fn default_base_url_is_generativelanguage_api() {
    let provider = GeminiProvider::with_default_base("test-key");
    assert_eq!(provider.provider_name(), "gemini");
}
