//! Memory and persistence abstractions for Portico.

pub mod embedding;
pub mod instruction;
pub mod manager;
pub mod pattern;
pub mod rag;
pub mod rag_store;

pub use app_models::{InstructionFile, MemoryId, MemoryItem, MemoryScope, RagChunk};
pub use embedding::{
    EmbeddingProvider, HashEmbeddingProvider, OllamaEmbeddingProvider,
    OpenAiCompatEmbeddingProvider,
};
pub use instruction::InstructionLoader;
pub use manager::{MemoryManager, SqliteMemoryManager};
pub use pattern::{InMemoryPatternStore, PatternStore, SqlitePatternStore};
pub use rag::{RagIndex, StoredChunk, simple_hash_embedding};
pub use rag_store::SqliteRagStore;
