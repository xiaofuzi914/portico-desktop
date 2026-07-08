//! Memory and persistence abstractions for Portico.

pub mod instruction;
pub mod manager;
pub mod rag;

pub use app_models::{InstructionFile, MemoryId, MemoryItem, MemoryScope, RagChunk};
pub use instruction::InstructionLoader;
pub use manager::{MemoryManager, SqliteMemoryManager};
pub use rag::{RagIndex, simple_hash_embedding};
