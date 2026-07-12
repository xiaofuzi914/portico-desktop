//! Minimal `LLMProvider` implementation for tests and offline demos.

use async_trait::async_trait;
use autoagents_llm::{
    FunctionCall, HasConfig, LLMProvider, NoConfig, ToolCall,
    chat::{
        ChatMessage, ChatProvider, ChatResponse, ChatRole, StreamChoice, StreamChunk, StreamDelta,
        StreamResponse, StructuredOutputFormat, Tool,
    },
    completion::{CompletionProvider, CompletionRequest, CompletionResponse},
    embedding::EmbeddingProvider,
    error::LLMError,
    models::{ModelListRequest, ModelListResponse, ModelsProvider},
};
use futures::stream::{self, Stream};
use std::{fmt, pin::Pin};

/// A mock LLM provider that returns deterministic responses.
///
/// - `chat` / `chat_with_tools` return a fixed text response.
/// - If the user message contains the word `"tool"` and tools are available, a
///   single mock tool call is emitted on the first turn.
/// - All other capabilities return stub values or clear "not implemented" errors.
#[derive(Debug, Clone, Default)]
pub struct MockLlmProvider;

impl MockLlmProvider {
    /// Create a new mock LLM provider.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    fn user_wants_tool(messages: &[ChatMessage]) -> bool {
        messages
            .iter()
            .any(|m| m.role == ChatRole::User && m.content.to_lowercase().contains("tool"))
    }

    fn is_first_turn(messages: &[ChatMessage]) -> bool {
        !messages
            .iter()
            .any(|m| m.role == ChatRole::Assistant || m.role == ChatRole::Tool)
    }

    fn should_emit_tool(messages: &[ChatMessage], tools: Option<&[Tool]>) -> bool {
        tools.is_some()
            && Self::is_first_turn(messages)
            && (Self::user_wants_tool(messages)
                || messages.iter().any(|message| {
                    message.content.contains("PORTICO_TEST_READ:")
                        || message.content.contains("PORTICO_TEST_WRITE:")
                }))
    }

    fn mock_tool_call(messages: &[ChatMessage]) -> ToolCall {
        let content = messages
            .iter()
            .find(|message| message.role == ChatRole::User)
            .map(|message| message.content.as_str())
            .unwrap_or_default();
        if let Some(path) = content.split("PORTICO_TEST_READ:").nth(1).map(str::trim) {
            return ToolCall {
                id: "mock_read_1".to_owned(),
                call_type: "function".to_owned(),
                function: FunctionCall {
                    name: "fs_read".to_owned(),
                    arguments: serde_json::json!({"path": path}).to_string(),
                },
            };
        }
        if let Some(path) = content.split("PORTICO_TEST_WRITE:").nth(1).map(str::trim) {
            return ToolCall {
                id: "mock_write_1".to_owned(),
                call_type: "function".to_owned(),
                function: FunctionCall {
                    name: "fs_write".to_owned(),
                    arguments: serde_json::json!({
                        "path": path,
                        "content": "approved by durable loop\n"
                    })
                    .to_string(),
                },
            };
        }
        ToolCall {
            id: "mock_call_1".to_owned(),
            call_type: "function".to_owned(),
            function: FunctionCall {
                name: "mock_tool".to_owned(),
                arguments: r#"{"query":"test"}"#.to_owned(),
            },
        }
    }
}

#[async_trait]
impl ChatProvider for MockLlmProvider {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        json_schema: Option<StructuredOutputFormat>,
    ) -> Result<Box<dyn ChatResponse>, LLMError> {
        self.chat_with_tools(messages, None, json_schema).await
    }

    async fn chat_with_tools(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[Tool]>,
        _json_schema: Option<StructuredOutputFormat>,
    ) -> Result<Box<dyn ChatResponse>, LLMError> {
        let emit_tool = Self::should_emit_tool(messages, tools);

        let text = if emit_tool {
            Some("I will use a tool.".to_owned())
        } else {
            Some("Mock response".to_owned())
        };

        let tool_calls = if emit_tool {
            Some(vec![Self::mock_tool_call(messages)])
        } else {
            None
        };

        Ok(Box::new(MockChatResponse { text, tool_calls }))
    }

    async fn chat_stream_struct(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[Tool]>,
        _json_schema: Option<StructuredOutputFormat>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamResponse, LLMError>> + Send>>, LLMError>
    {
        let emit_tool = Self::should_emit_tool(messages, tools);
        let text = if emit_tool {
            "I will use a tool."
        } else {
            "Mock response"
        };

        let chunks = vec![Ok(StreamResponse {
            choices: vec![StreamChoice {
                delta: StreamDelta {
                    content: Some(text.to_owned()),
                    reasoning_content: None,
                    tool_calls: None,
                },
            }],
            usage: None,
        })];

        Ok(Box::pin(stream::iter(chunks)))
    }

    async fn chat_stream_with_tools(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[Tool]>,
        _json_schema: Option<StructuredOutputFormat>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LLMError>> + Send>>, LLMError> {
        let emit_tool = Self::should_emit_tool(messages, tools);
        let mut chunks: Vec<Result<StreamChunk, LLMError>> = Vec::new();

        if emit_tool {
            chunks.push(Ok(StreamChunk::Text("I will use a tool.".to_owned())));
            chunks.push(Ok(StreamChunk::ToolUseComplete {
                index: 0,
                tool_call: Self::mock_tool_call(messages),
            }));
            chunks.push(Ok(StreamChunk::Done {
                stop_reason: "tool_use".to_owned(),
            }));
        } else {
            chunks.push(Ok(StreamChunk::Text("Mock response".to_owned())));
            chunks.push(Ok(StreamChunk::Done {
                stop_reason: "end_turn".to_owned(),
            }));
        }

        Ok(Box::pin(stream::iter(chunks)))
    }
}

#[async_trait]
impl CompletionProvider for MockLlmProvider {
    async fn complete(
        &self,
        _req: &CompletionRequest,
        _json_schema: Option<StructuredOutputFormat>,
    ) -> Result<CompletionResponse, LLMError> {
        Ok(CompletionResponse {
            text: "Mock completion".to_owned(),
        })
    }
}

#[async_trait]
impl EmbeddingProvider for MockLlmProvider {
    #[allow(clippy::cast_precision_loss)]
    async fn embed(&self, input: Vec<String>) -> Result<Vec<Vec<f32>>, LLMError> {
        Ok(input.iter().enumerate().map(|(i, _)| vec![i as f32, (i + 1) as f32]).collect())
    }
}

#[async_trait]
impl ModelsProvider for MockLlmProvider {
    async fn list_models(
        &self,
        _request: Option<&ModelListRequest>,
    ) -> Result<Box<dyn ModelListResponse>, LLMError> {
        Err(LLMError::Generic(
            "MockLlmProvider::list_models is not implemented".to_owned(),
        ))
    }
}

impl LLMProvider for MockLlmProvider {}

impl HasConfig for MockLlmProvider {
    type Config = NoConfig;
}

struct MockChatResponse {
    text: Option<String>,
    tool_calls: Option<Vec<ToolCall>>,
}

impl ChatResponse for MockChatResponse {
    fn text(&self) -> Option<String> {
        self.text.clone()
    }

    fn tool_calls(&self) -> Option<Vec<ToolCall>> {
        self.tool_calls.clone()
    }
}

impl fmt::Debug for MockChatResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MockChatResponse")
            .field("text", &self.text)
            .field("tool_calls", &self.tool_calls.as_ref().map(Vec::len))
            .finish()
    }
}

impl fmt::Display for MockChatResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.text.as_deref().unwrap_or(""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn chat_returns_mock_response() {
        let provider = MockLlmProvider::new();
        let messages = vec![ChatMessage::user().content("hello").build()];
        let response = provider.chat(&messages, None).await.expect("chat");
        assert_eq!(response.text(), Some("Mock response".to_owned()));
    }

    #[tokio::test]
    async fn chat_with_tools_and_tool_keyword_emits_tool_call() {
        let provider = MockLlmProvider::new();
        let messages = vec![ChatMessage::user().content("use a tool").build()];
        let tool = Tool {
            tool_type: "function".to_owned(),
            function: autoagents_llm::chat::FunctionTool {
                name: "mock_tool".to_owned(),
                description: "mock".to_owned(),
                parameters: serde_json::json!({"type": "object"}),
            },
        };
        let response = provider
            .chat_with_tools(&messages, Some(&[tool]), None)
            .await
            .expect("chat_with_tools");
        assert!(response.tool_calls().is_some());
        let calls = response.tool_calls().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "mock_tool");
    }

    #[tokio::test]
    async fn chat_with_tools_no_keyword_returns_plain_response() {
        let provider = MockLlmProvider::new();
        let messages = vec![ChatMessage::user().content("hello").build()];
        let tool = Tool {
            tool_type: "function".to_owned(),
            function: autoagents_llm::chat::FunctionTool {
                name: "mock_tool".to_owned(),
                description: "mock".to_owned(),
                parameters: serde_json::json!({"type": "object"}),
            },
        };
        let response = provider
            .chat_with_tools(&messages, Some(&[tool]), None)
            .await
            .expect("chat_with_tools");
        assert!(response.tool_calls().is_none());
        assert_eq!(response.text(), Some("Mock response".to_owned()));
    }

    #[tokio::test]
    async fn completion_returns_mock_text() {
        let provider = MockLlmProvider::new();
        let request = CompletionRequest::new("prompt");
        let response = provider.complete(&request, None).await.expect("complete");
        assert_eq!(response.text, "Mock completion");
    }

    #[tokio::test]
    async fn embed_returns_incremental_vectors() {
        let provider = MockLlmProvider::new();
        let embeddings = provider.embed(vec!["a".to_owned(), "b".to_owned()]).await.expect("embed");
        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0], vec![0.0, 1.0]);
        assert_eq!(embeddings[1], vec![1.0, 2.0]);
    }

    #[tokio::test]
    async fn list_models_returns_error() {
        let provider = MockLlmProvider::new();
        let result = provider.list_models(None).await;
        assert!(result.is_err());
    }
}
