//! Embedding providers for RAG.

use app_models::AppError;
use async_trait::async_trait;
use serde::Deserialize;
use std::time::Duration;

use crate::rag::simple_hash_embedding;

const EMBEDDING_TIMEOUT: Duration = Duration::from_secs(30);

/// Abstract source of text embeddings.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a batch of texts into vectors.
    ///
    /// # Errors
    ///
    /// Returns an error if embedding fails.
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AppError>;

    /// Vector dimension produced by this provider.
    fn dimension(&self) -> usize;

    /// Stable provider identifier, persisted with each chunk.
    fn id(&self) -> &'static str;
}

/// Deterministic hash-based embedding fallback for offline use.
#[derive(Debug, Clone, Default)]
pub struct HashEmbeddingProvider;

impl HashEmbeddingProvider {
    /// Create a new hash embedding provider.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl EmbeddingProvider for HashEmbeddingProvider {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AppError> {
        Ok(texts.iter().map(|text| simple_hash_embedding(text)).collect())
    }

    fn dimension(&self) -> usize {
        64
    }

    fn id(&self) -> &'static str {
        "hash-64"
    }
}

/// OpenAI-compatible embedding provider.
///
/// Works with `OpenAI`, `Moonshot`, `DeepSeek`, and any provider exposing
/// `{base_url}/embeddings` with the `OpenAI` request/response shape.
#[derive(Debug, Clone)]
pub struct OpenAiCompatEmbeddingProvider {
    id: &'static str,
    client: reqwest::Client,
    base_url: String,
    model: String,
    api_key: String,
    dimension: usize,
}

impl OpenAiCompatEmbeddingProvider {
    /// Create a new OpenAI-compatible embedding provider.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn new(
        id: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: impl Into<String>,
        dimension: usize,
    ) -> Result<Self, AppError> {
        let client = reqwest::Client::builder()
            .timeout(EMBEDDING_TIMEOUT)
            .build()
            .map_err(|e| AppError::Internal {
                message: format!("failed to build OpenAI embedding client: {e}"),
            })?;

        let id = Box::leak(id.into().into_boxed_str());

        Ok(Self {
            id,
            client,
            base_url: base_url.into(),
            model: model.into(),
            api_key: api_key.into(),
            dimension,
        })
    }

    /// Build the JSON request body for a batch of texts.
    ///
    /// Exposed for unit testing without making network calls.
    #[must_use]
    pub fn build_request_body(&self, texts: &[String]) -> serde_json::Value {
        serde_json::json!({
            "model": self.model,
            "input": texts,
        })
    }

    /// Build the full URL for the embeddings endpoint.
    #[must_use]
    pub fn embeddings_url(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        format!("{base}/embeddings")
    }

    /// Parse the OpenAI-compatible embeddings response.
    fn parse_response(payload: &serde_json::Value) -> Result<Vec<Vec<f32>>, AppError> {
        let data = payload.get("data").ok_or_else(|| AppError::Internal {
            message: "embedding response missing 'data' field".to_owned(),
        })?;

        let items = data.as_array().ok_or_else(|| AppError::Internal {
            message: "embedding response 'data' is not an array".to_owned(),
        })?;

        let mut indexed: Vec<(usize, Vec<f32>)> = Vec::with_capacity(items.len());
        for item in items {
            let index = item
                .get("index")
                .and_then(serde_json::Value::as_u64)
                .map_or(indexed.len(), |n| {
                    usize::try_from(n).unwrap_or(indexed.len())
                });
            let embedding = item.get("embedding").ok_or_else(|| AppError::Internal {
                message: "embedding item missing 'embedding' field".to_owned(),
            })?;
            let vector: Vec<f32> = serde_json::from_value(embedding.clone()).map_err(|e| {
                AppError::Internal {
                    message: format!("failed to parse embedding vector: {e}"),
                }
            })?;
            indexed.push((index, vector));
        }

        indexed.sort_by_key(|a| a.0);
        Ok(indexed.into_iter().map(|(_, vector)| vector).collect())
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAiCompatEmbeddingProvider {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AppError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let url = self.embeddings_url();
        let body = self.build_request_body(texts);

        let request = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body);

        let response = request.send().await.map_err(|e| AppError::Internal {
            message: format!("OpenAI embedding request failed: {e}"),
        })?;

        let status = response.status();
        if !status.is_success() {
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable>".to_owned());
            return Err(AppError::Internal {
                message: format!("OpenAI embedding request returned {status}: {text}"),
            });
        }

        let payload: serde_json::Value = response.json().await.map_err(|e| AppError::Internal {
            message: format!("failed to parse OpenAI embedding response: {e}"),
        })?;

        Self::parse_response(&payload)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn id(&self) -> &'static str {
        self.id
    }
}

/// Ollama embedding provider.
///
/// Posts to `{base_url}/api/embeddings` and expects a single embedding back.
#[derive(Debug, Clone)]
pub struct OllamaEmbeddingProvider {
    id: &'static str,
    client: reqwest::Client,
    base_url: String,
    model: String,
    dimension: usize,
}

/// Response body from Ollama's `/api/embeddings` endpoint.
#[derive(Debug, Deserialize)]
struct OllamaEmbeddingResponse {
    embedding: Vec<f32>,
}

impl OllamaEmbeddingProvider {
    /// Create a new Ollama embedding provider.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn new(
        id: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
        dimension: usize,
    ) -> Result<Self, AppError> {
        let client = reqwest::Client::builder()
            .timeout(EMBEDDING_TIMEOUT)
            .build()
            .map_err(|e| AppError::Internal {
                message: format!("failed to build Ollama embedding client: {e}"),
            })?;

        let id = Box::leak(id.into().into_boxed_str());

        Ok(Self {
            id,
            client,
            base_url: base_url.into(),
            model: model.into(),
            dimension,
        })
    }

    /// Build the embeddings endpoint URL.
    #[must_use]
    pub fn embeddings_url(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        format!("{base}/api/embeddings")
    }

    /// Build the JSON request body for a single prompt.
    #[must_use]
    pub fn build_request_body(&self, prompt: &str) -> serde_json::Value {
        serde_json::json!({
            "model": self.model,
            "prompt": prompt,
        })
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaEmbeddingProvider {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AppError> {
        let url = self.embeddings_url();
        let mut embeddings = Vec::with_capacity(texts.len());

        for text in texts {
            let body = self.build_request_body(text);
            let response = self
                .client
                .post(&url)
                .json(&body)
                .send()
                .await
                .map_err(|e| AppError::Internal {
                    message: format!("Ollama embedding request failed: {e}"),
                })?;

            let status = response.status();
            if !status.is_success() {
                let text_err = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "<unreadable>".to_owned());
                return Err(AppError::Internal {
                    message: format!("Ollama embedding request returned {status}: {text_err}"),
                });
            }

            let payload: OllamaEmbeddingResponse = response.json().await.map_err(|e| {
                AppError::Internal {
                    message: format!("failed to parse Ollama embedding response: {e}"),
                }
            })?;

            embeddings.push(payload.embedding);
        }

        Ok(embeddings)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn id(&self) -> &'static str {
        self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hash_provider_embeds_single_text() {
        let provider = HashEmbeddingProvider::new();
        let embeddings = provider.embed(&["hello world".to_owned()]).await.unwrap();
        assert_eq!(embeddings.len(), 1);
        assert_eq!(embeddings[0].len(), provider.dimension());
        assert_eq!(provider.id(), "hash-64");
    }

    #[tokio::test]
    async fn hash_provider_is_deterministic() {
        let provider = HashEmbeddingProvider::new();
        let a = provider.embed(&["rust".to_owned()]).await.unwrap();
        let b = provider.embed(&["rust".to_owned()]).await.unwrap();
        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn hash_provider_empty_text_returns_empty() {
        let provider = HashEmbeddingProvider::new();
        let embeddings = provider.embed(&[String::new()]).await.unwrap();
        assert!(embeddings[0].is_empty());
    }

    #[test]
    fn openai_request_body_matches_api_shape() {
        let provider = OpenAiCompatEmbeddingProvider::new(
            "openai",
            "https://api.openai.com/v1",
            "text-embedding-3-small",
            "sk-test",
            1536,
        )
        .unwrap();

        let body = provider.build_request_body(&["foo".to_owned(), "bar".to_owned()]);
        assert_eq!(body["model"], "text-embedding-3-small");
        let input = body["input"].as_array().unwrap();
        assert_eq!(input.len(), 2);
        assert_eq!(input[0], "foo");
        assert_eq!(input[1], "bar");
    }

    #[test]
    fn openai_url_trims_trailing_slash() {
        let provider = OpenAiCompatEmbeddingProvider::new(
            "openai",
            "https://api.openai.com/v1/",
            "text-embedding-3-small",
            "sk-test",
            1536,
        )
        .unwrap();
        assert_eq!(provider.embeddings_url(), "https://api.openai.com/v1/embeddings");
    }

    #[test]
    fn openai_parse_response_orders_by_index() {
        let payload = serde_json::json!({
            "data": [
                { "embedding": [0.0, 0.0, 1.0], "index": 1 },
                { "embedding": [1.0, 0.0, 0.0], "index": 0 },
            ]
        });

        let result = OpenAiCompatEmbeddingProvider::parse_response(&payload).unwrap();
        assert_eq!(result.len(), 2);
        // Result is reordered by index.
        assert_eq!(result[0], vec![1.0, 0.0, 0.0]);
        assert_eq!(result[1], vec![0.0, 0.0, 1.0]);
    }

    #[test]
    fn ollama_request_body_matches_api_shape() {
        let provider = OllamaEmbeddingProvider::new(
            "ollama",
            "http://localhost:11434",
            "nomic-embed-text",
            768,
        )
        .unwrap();

        let body = provider.build_request_body("hello");
        assert_eq!(body["model"], "nomic-embed-text");
        assert_eq!(body["prompt"], "hello");
    }

    #[test]
    fn ollama_url_trims_trailing_slash() {
        let provider = OllamaEmbeddingProvider::new(
            "ollama",
            "http://localhost:11434/",
            "nomic-embed-text",
            768,
        )
        .unwrap();
        assert_eq!(provider.embeddings_url(), "http://localhost:11434/api/embeddings");
    }
}
