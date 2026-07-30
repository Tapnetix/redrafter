use std::time::Duration;

use llm_provider::ollama::OllamaProvider;
use llm_provider::{ChatMessage, LlmProvider, LlmRequest};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sample_request() -> LlmRequest {
    LlmRequest {
        messages: vec![ChatMessage::system("Be concise."), ChatMessage::user("Hi")],
        model: "llama3".to_string(),
        temperature: Some(0.7),
        max_tokens: Some(100),
    }
}

#[tokio::test]
async fn chat_returns_response_on_success() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "llama3",
            "message": {"role": "assistant", "content": "Hello from Ollama!"},
            "done": true,
            "eval_count": 15
        })))
        .mount(&server)
        .await;

    let provider = OllamaProvider::new(&server.uri());
    let response = provider
        .chat(&sample_request(), CancellationToken::new())
        .await
        .expect("chat should succeed");

    assert_eq!(response.text, "Hello from Ollama!");
    assert_eq!(response.model, "llama3");
    assert_eq!(response.usage_tokens, Some(15));
}

#[tokio::test]
async fn chat_returns_error_on_bad_status() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({"error": "model not found"})),
        )
        .mount(&server)
        .await;

    let provider = OllamaProvider::new(&server.uri());
    let err = provider
        .chat(&sample_request(), CancellationToken::new())
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("404"),
        "error should mention status code, got: {}",
        err
    );
}

#[tokio::test]
async fn chat_is_cancellable_mid_flight() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({
                    "model": "llama3",
                    "message": {"role": "assistant", "content": "too slow"},
                    "done": true
                }))
                .set_delay(Duration::from_secs(5)),
        )
        .mount(&server)
        .await;

    let provider = OllamaProvider::new(&server.uri());
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
async fn list_models_returns_model_names_from_tags() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "models": [
                {"name": "llama3:latest", "size": 123, "digest": "abc"},
                {"name": "mistral:latest", "size": 456, "digest": "def"}
            ]
        })))
        .mount(&server)
        .await;

    let provider = OllamaProvider::new(&server.uri());
    let models = provider
        .list_models()
        .await
        .expect("list_models should succeed");

    assert_eq!(
        models,
        vec!["llama3:latest".to_string(), "mistral:latest".to_string()]
    );
}

#[tokio::test]
async fn is_available_true_when_ps_endpoint_ok() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/ps"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"models": []})))
        .mount(&server)
        .await;

    let provider = OllamaProvider::new(&server.uri());
    assert!(provider.is_available().await);
}

#[tokio::test]
async fn is_available_false_when_unreachable() {
    let provider = OllamaProvider::new("http://127.0.0.1:1");
    assert!(!provider.is_available().await);
}

#[tokio::test]
async fn provider_name_is_ollama() {
    let provider = OllamaProvider::new("http://localhost:11434");
    assert_eq!(provider.provider_name(), "ollama");
}

#[test]
fn default_uses_localhost_11434() {
    let provider = OllamaProvider::default();
    assert_eq!(provider.base_url(), "http://localhost:11434");
}

#[test]
fn new_strips_trailing_slash() {
    let provider = OllamaProvider::new("http://localhost:11434/");
    assert_eq!(provider.base_url(), "http://localhost:11434");
}

#[tokio::test]
async fn pull_streams_progress_and_reports_completion() {
    let server = MockServer::start().await;

    let ndjson = concat!(
        "{\"status\":\"pulling manifest\"}\n",
        "{\"status\":\"downloading sha256:abc\",\"digest\":\"sha256:abc\",\"total\":1000,\"completed\":500}\n",
        "{\"status\":\"downloading sha256:abc\",\"digest\":\"sha256:abc\",\"total\":1000,\"completed\":1000}\n",
        "{\"status\":\"verifying sha256 digest\"}\n",
        "{\"status\":\"success\"}\n",
    );

    Mock::given(method("POST"))
        .and(path("/api/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(ndjson, "application/x-ndjson"))
        .mount(&server)
        .await;

    let provider = OllamaProvider::new(&server.uri());
    let mut rx = provider
        .pull("llama3", CancellationToken::new())
        .await
        .expect("pull should start successfully");

    let mut events = Vec::new();
    while let Some(progress) = rx.recv().await {
        events.push(progress);
    }

    assert_eq!(
        events.len(),
        5,
        "expected 5 progress lines, got {:?}",
        events
    );
    assert_eq!(events[0].status, "pulling manifest");
    assert_eq!(events[1].completed, Some(500));
    assert_eq!(events[1].total, Some(1000));
    assert_eq!(events[2].completed, Some(1000));
    assert!(events.last().unwrap().is_done());
}

#[tokio::test]
async fn pull_returns_error_on_bad_status() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/pull"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({"error": "model not found"})),
        )
        .mount(&server)
        .await;

    let provider = OllamaProvider::new(&server.uri());
    let err = provider
        .pull("nonexistent", CancellationToken::new())
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("404"),
        "error should mention status code, got: {}",
        err
    );
}

#[tokio::test]
async fn pull_skips_malformed_lines_and_keeps_streaming() {
    let server = MockServer::start().await;

    // A stray non-JSON keep-alive line (and a blank line) should be skipped
    // rather than aborting the stream.
    let ndjson = concat!(
        "{\"status\":\"pulling manifest\"}\n",
        "\n",
        "not json at all\n",
        "{\"status\":\"success\"}\n",
    );

    Mock::given(method("POST"))
        .and(path("/api/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(ndjson, "application/x-ndjson"))
        .mount(&server)
        .await;

    let provider = OllamaProvider::new(&server.uri());
    let mut rx = provider
        .pull("llama3", CancellationToken::new())
        .await
        .expect("pull should start successfully");

    let mut events = Vec::new();
    while let Some(progress) = rx.recv().await {
        events.push(progress);
    }

    assert_eq!(events.len(), 2, "malformed/blank lines should be skipped");
    assert_eq!(events[0].status, "pulling manifest");
    assert!(events[1].is_done());
}

#[tokio::test]
async fn pull_parses_trailing_line_without_newline() {
    let server = MockServer::start().await;

    // Final line has no trailing newline; the buffered remainder must still
    // be flushed once the stream ends.
    let ndjson = "{\"status\":\"pulling manifest\"}\n{\"status\":\"success\"}";

    Mock::given(method("POST"))
        .and(path("/api/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(ndjson, "application/x-ndjson"))
        .mount(&server)
        .await;

    let provider = OllamaProvider::new(&server.uri());
    let mut rx = provider
        .pull("llama3", CancellationToken::new())
        .await
        .expect("pull should start successfully");

    let mut events = Vec::new();
    while let Some(progress) = rx.recv().await {
        events.push(progress);
    }

    assert_eq!(events.len(), 2);
    assert!(events.last().unwrap().is_done());
}

#[tokio::test]
async fn pull_is_cancellable_before_start() {
    let server = MockServer::start().await;
    let provider = OllamaProvider::new(&server.uri());

    let cancel = CancellationToken::new();
    cancel.cancel();

    let err = provider.pull("llama3", cancel).await.unwrap_err();
    assert!(err.to_string().contains("cancelled"));
}
