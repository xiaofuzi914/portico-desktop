//! Simple in-memory RAG vector index for Phase 7.

use app_models::{AppError, RagChunk, WorkspaceId};

/// Character length of each document chunk.
const CHUNK_SIZE: usize = 500;
/// Overlap between consecutive chunks to preserve context.
const CHUNK_OVERLAP: usize = 50;
/// Dimension of the simple hash embedding vector.
const EMBEDDING_DIM: usize = 64;

/// A single stored document chunk with its embedding.
#[derive(Debug, Clone)]
struct StoredChunk {
    id: i64,
    document_path: String,
    chunk_index: usize,
    content: String,
    workspace_id: WorkspaceId,
    embedding: Vec<f32>,
}

/// In-memory vector index for RAG retrieval.
#[derive(Debug, Clone, Default)]
pub struct RagIndex {
    next_id: i64,
    chunks: Vec<StoredChunk>,
}

impl RagIndex {
    /// Create an empty RAG index.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next_id: 1,
            chunks: Vec::new(),
        }
    }

    /// Chunk `content` and add it to the index under `workspace_id`.
    ///
    /// # Errors
    ///
    /// Returns an error if embedding fails or the document is empty.
    pub fn add_document(
        &mut self,
        workspace_id: WorkspaceId,
        path: &str,
        content: &str,
        embed_fn: impl Fn(&str) -> Vec<f32>,
    ) -> Result<(), AppError> {
        if content.is_empty() {
            return Err(AppError::Internal {
                message: "cannot index empty document".to_owned(),
            });
        }

        let mut chunk_index = 0usize;
        let mut start = 0usize;
        let chars: Vec<char> = content.chars().collect();

        while start < chars.len() {
            let end = (start + CHUNK_SIZE).min(chars.len());
            let chunk: String = chars[start..end].iter().collect();
            let embedding = embed_fn(&chunk);

            if embedding.is_empty() {
                return Err(AppError::Internal {
                    message: "embedding produced empty vector".to_owned(),
                });
            }

            self.chunks.push(StoredChunk {
                id: self.next_id,
                document_path: path.to_owned(),
                chunk_index,
                content: chunk,
                workspace_id,
                embedding,
            });
            self.next_id += 1;
            chunk_index += 1;

            if end == chars.len() {
                break;
            }
            start = end.saturating_sub(CHUNK_OVERLAP);
            if start >= chars.len() {
                break;
            }
        }

        Ok(())
    }

    /// Search the index for chunks semantically similar to `query`.
    #[must_use]
    pub fn search(
        &self,
        workspace_id: WorkspaceId,
        query: &str,
        embed_fn: impl Fn(&str) -> Vec<f32>,
        top_n: usize,
    ) -> Vec<RagChunk> {
        let query_embedding = embed_fn(query);
        if query_embedding.is_empty() {
            return Vec::new();
        }

        let mut scored: Vec<(f64, &StoredChunk)> = self
            .chunks
            .iter()
            .filter(|chunk| chunk.workspace_id == workspace_id)
            .map(|chunk| {
                let score = cosine_similarity(&query_embedding, &chunk.embedding);
                (score, chunk)
            })
            .filter(|(score, _)| score.is_finite())
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_n);

        scored
            .into_iter()
            .map(|(score, chunk)| RagChunk {
                id: chunk.id,
                document_path: chunk.document_path.clone(),
                chunk_index: chunk.chunk_index,
                content: chunk.content.clone(),
                score,
            })
            .collect()
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    f64::from(dot) / f64::from(norm_a * norm_b)
}

/// A simple deterministic embedding function for Phase 7.
///
/// Splits text into tokens and averages hashed token vectors into a fixed
/// 64-dimensional vector. This is not semantically meaningful but is fast,
/// offline, and deterministic.
#[must_use]
pub fn simple_hash_embedding(text: &str) -> Vec<f32> {
    let mut vec = vec![0.0f32; EMBEDDING_DIM];
    let tokens: Vec<&str> = text.split_whitespace().collect();

    if tokens.is_empty() {
        return Vec::new();
    }

    for token in &tokens {
        let hash = xxhash_rust::xxh3::xxh3_64(token.as_bytes());
        for (i, value) in vec.iter_mut().enumerate().take(EMBEDDING_DIM) {
            let bit = (hash >> (i % 64)) & 1;
            *value += if bit == 1 { 1.0 } else { -1.0 };
        }
    }

    // Average by token count. Precision loss is acceptable for this simple Phase 7 embedding.
    #[allow(clippy::cast_precision_loss)]
    let count = tokens.len() as f32;
    for value in &mut vec {
        *value /= count;
    }

    vec
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_search_document() {
        let mut index = RagIndex::new();
        let workspace_id = WorkspaceId::new();

        index
            .add_document(
                workspace_id,
                "/docs/readme.md",
                "The quick brown fox jumps over the lazy dog. Rust is a systems programming language.",
                simple_hash_embedding,
            )
            .unwrap();

        let results = index.search(workspace_id, "fox", simple_hash_embedding, 3);
        assert!(!results.is_empty());
        assert!(results[0].content.to_lowercase().contains("fox"));
    }

    #[test]
    fn search_filters_by_workspace() {
        let mut index = RagIndex::new();
        let ws_a = WorkspaceId::new();
        let ws_b = WorkspaceId::new();

        index
            .add_document(ws_a, "a.md", "content about alpha", simple_hash_embedding)
            .unwrap();
        index
            .add_document(ws_b, "b.md", "content about beta", simple_hash_embedding)
            .unwrap();

        let results = index.search(ws_a, "alpha", simple_hash_embedding, 3);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].document_path, "a.md");
    }

    #[test]
    fn empty_document_errors() {
        let mut index = RagIndex::new();
        let workspace_id = WorkspaceId::new();
        let result = index.add_document(workspace_id, "empty.md", "", simple_hash_embedding);
        assert!(matches!(result, Err(AppError::Internal { .. })));
    }
}
