//! In-memory RAG vector index used as a query cache.

use app_models::{AppError, RagChunk, WorkspaceId};

/// Character length of each document chunk.
const CHUNK_SIZE: usize = 500;
/// Overlap between consecutive chunks to preserve context.
const CHUNK_OVERLAP: usize = 50;
/// Dimension of the simple hash embedding vector.
const EMBEDDING_DIM: usize = 64;

/// A single stored document chunk with its embedding.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredChunk {
    pub(crate) id: i64,
    pub(crate) document_path: String,
    pub(crate) chunk_index: usize,
    pub(crate) content: String,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) embedding: Vec<f32>,
}

impl StoredChunk {
    /// Create a new stored chunk.
    #[must_use]
    pub const fn new(
        id: i64,
        document_path: String,
        chunk_index: usize,
        content: String,
        workspace_id: WorkspaceId,
        embedding: Vec<f32>,
    ) -> Self {
        Self {
            id,
            document_path,
            chunk_index,
            content,
            workspace_id,
            embedding,
        }
    }
}

/// In-memory vector index for RAG retrieval, backed by `SqliteRagStore`.
#[derive(Debug, Clone, Default)]
pub struct RagIndex {
    chunks: Vec<StoredChunk>,
}

impl RagIndex {
    /// Create an empty RAG index.
    #[must_use]
    pub const fn new() -> Self {
        Self { chunks: Vec::new() }
    }

    /// Replace all chunks in the index.
    pub fn replace_chunks(&mut self, chunks: Vec<StoredChunk>) {
        self.chunks = chunks;
    }

    /// Append chunks to the index.
    pub fn append_chunks(&mut self, chunks: Vec<StoredChunk>) {
        self.chunks.extend(chunks);
    }

    /// Split `content` into overlapping text chunks.
    #[must_use]
    pub fn split_document(content: &str) -> Vec<String> {
        Self::split_text(content)
    }

    /// Build stored chunks from split text and corresponding embeddings.
    ///
    /// # Errors
    ///
    /// Returns an error if text and embedding counts differ.
    pub fn build_chunks(
        workspace_id: WorkspaceId,
        path: &str,
        contents: Vec<String>,
        embeddings: Vec<Vec<f32>>,
    ) -> Result<Vec<StoredChunk>, AppError> {
        if contents.len() != embeddings.len() {
            return Err(AppError::Internal {
                message: format!(
                    "chunk count {} does not match embedding count {}",
                    contents.len(),
                    embeddings.len()
                ),
            });
        }

        Ok(contents
            .into_iter()
            .zip(embeddings)
            .enumerate()
            .map(|(chunk_index, (content, embedding))| StoredChunk {
                id: 0,
                document_path: path.to_owned(),
                chunk_index,
                content,
                workspace_id,
                embedding,
            })
            .collect())
    }

    /// Search the index for chunks semantically similar to `query_embedding`.
    #[must_use]
    pub fn search(
        &self,
        workspace_id: WorkspaceId,
        query_embedding: &[f32],
        top_n: usize,
    ) -> Vec<RagChunk> {
        if query_embedding.is_empty() {
            return Vec::new();
        }

        let mut scored: Vec<(f64, &StoredChunk)> = self
            .chunks
            .iter()
            .filter(|chunk| chunk.workspace_id == workspace_id)
            .map(|chunk| {
                let score = cosine_similarity(query_embedding, &chunk.embedding);
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

    /// Split text into overlapping chunks.
    fn split_text(content: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut start = 0usize;
        let chars: Vec<char> = content.chars().collect();

        while start < chars.len() {
            let end = (start + CHUNK_SIZE).min(chars.len());
            let chunk: String = chars[start..end].iter().collect();
            result.push(chunk);

            if end == chars.len() {
                break;
            }
            start = end.saturating_sub(CHUNK_OVERLAP);
            if start >= chars.len() {
                break;
            }
        }

        result
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
    fn build_chunks_splits_and_pairs_embeddings() {
        let workspace_id = WorkspaceId::new();
        let contents = vec![
            "Rust is a fast systems programming language.".to_owned(),
            "It guarantees memory safety.".to_owned(),
        ];
        let embeddings = vec![
            simple_hash_embedding("chunk0"),
            simple_hash_embedding("chunk1"),
        ];
        let chunks =
            RagIndex::build_chunks(workspace_id, "/docs/readme.md", contents, embeddings).unwrap();

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].document_path, "/docs/readme.md");
        assert_eq!(chunks[0].chunk_index, 0);
        assert_eq!(chunks[1].chunk_index, 1);
    }

    #[test]
    fn split_document_respects_overlap() {
        let content = "a ".repeat(600);
        let chunks = RagIndex::split_document(&content);
        assert!(chunks.len() >= 2);
    }

    #[test]
    fn search_filters_by_workspace() {
        let mut index = RagIndex::new();
        let ws_a = WorkspaceId::new();
        let ws_b = WorkspaceId::new();

        let embedding = simple_hash_embedding("alpha beta");
        index.replace_chunks(vec![
            StoredChunk {
                id: 1,
                document_path: "a.md".to_owned(),
                chunk_index: 0,
                content: "content about alpha".to_owned(),
                workspace_id: ws_a,
                embedding: embedding.clone(),
            },
            StoredChunk {
                id: 2,
                document_path: "b.md".to_owned(),
                chunk_index: 0,
                content: "content about beta".to_owned(),
                workspace_id: ws_b,
                embedding,
            },
        ]);

        let query = simple_hash_embedding("alpha");
        let results = index.search(ws_a, &query, 3);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].document_path, "a.md");
    }

    #[test]
    fn split_document_empty_returns_empty() {
        let chunks = RagIndex::split_document("");
        assert!(chunks.is_empty());
    }
}
