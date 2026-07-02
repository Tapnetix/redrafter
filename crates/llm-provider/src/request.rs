use serde::{Deserialize, Serialize};

/// A single message in a chat conversation, in OpenAI-compatible shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    /// Construct a `system` role message.
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }

    /// Construct a `user` role message.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }
}

/// Request payload sent to an [`crate::provider::LlmProvider`].
#[derive(Debug, Clone, Serialize)]
pub struct LlmRequest {
    pub messages: Vec<ChatMessage>,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
}

/// Response payload returned from an [`crate::provider::LlmProvider`].
#[derive(Debug, Clone, Deserialize)]
pub struct LlmResponse {
    pub text: String,
    pub model: String,
    pub usage_tokens: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_constructors() {
        let sys = ChatMessage::system("You are helpful.");
        assert_eq!(sys.role, "system");
        assert_eq!(sys.content, "You are helpful.");

        let usr = ChatMessage::user("Hello!");
        assert_eq!(usr.role, "user");
        assert_eq!(usr.content, "Hello!");
    }

    #[test]
    fn test_llm_request_serializes() {
        let request = LlmRequest {
            messages: vec![ChatMessage::system("Be concise."), ChatMessage::user("Hi")],
            model: "gpt-4o-mini".to_string(),
            temperature: 0.7,
            max_tokens: Some(100),
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["model"], "gpt-4o-mini");
        let temp = json["temperature"].as_f64().unwrap();
        assert!(
            (temp - 0.7).abs() < 0.001,
            "temperature should be ~0.7, got {}",
            temp
        );
        assert_eq!(json["max_tokens"], 100);
        assert_eq!(json["messages"].as_array().unwrap().len(), 2);
        assert_eq!(json["messages"][0]["role"], "system");
        assert_eq!(json["messages"][0]["content"], "Be concise.");
    }

    #[test]
    fn test_llm_response_deserializes() {
        let json = r#"{
            "text": "Hello there!",
            "model": "gpt-4o-mini",
            "usage_tokens": 42
        }"#;

        let response: LlmResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.text, "Hello there!");
        assert_eq!(response.model, "gpt-4o-mini");
        assert_eq!(response.usage_tokens, Some(42));
    }

    #[test]
    fn test_llm_response_optional_usage() {
        let json = r#"{
            "text": "Hi",
            "model": "gpt-4o-mini"
        }"#;

        let response: LlmResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.usage_tokens, None);
    }
}
