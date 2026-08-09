//! Memory and persistence abstractions for Portico.

pub mod candidate;
pub mod cipher;
pub mod embedding;
pub mod experience;
pub mod extractor;
pub mod instruction;
pub mod manager;
pub mod pattern;
pub mod rag;
pub mod rag_store;
pub mod recall;

pub use app_models::{InstructionFile, MemoryId, MemoryItem, MemoryScope, RagChunk};
pub use candidate::{CandidateStore, SqliteCandidateStore, candidate_fingerprint};
pub use cipher::{
    ENCRYPTED_VALUE_PLACEHOLDER, MEMORY_ENCRYPTION_VERSION, MemoryCipher, NoopMemoryCipher,
};
pub use embedding::{
    EmbeddingProvider, HashEmbeddingProvider, OllamaEmbeddingProvider,
    OpenAiCompatEmbeddingProvider,
};
pub use experience::{
    EXPERIENCE_SCHEMA_VERSION, ExperienceStore, SqliteExperienceStore, outcome_from_status,
};
pub use extractor::{EXTRACTOR_VERSION, extract_from_experience, extract_from_text};
pub use instruction::InstructionLoader;
pub use manager::{MemoryManager, SqliteMemoryManager};
pub use pattern::{
    AUTO_PROMOTE_EVIDENCE_THRESHOLD, InMemoryPatternStore, PatternStore, SqlitePatternStore,
    pattern_fingerprint,
};
pub use rag::{RagIndex, StoredChunk, simple_hash_embedding};
pub use rag_store::SqliteRagStore;
pub use recall::{
    MemoryRecallQuery, MemoryRecallResult, format_behavior_policy_for_prompt, recall_memories,
    synthesize_behavior_policy,
};
