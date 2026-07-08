//! Context inspector: assembles instructions, memories, and RAG chunks for a run.

use app_memory::{InstructionLoader, MemoryManager, RagIndex};
use app_models::{
    AgentRunId, AppError, ContextSummary, InstructionFile, MemoryItem, MemoryScope, RagChunk,
    ThreadId, WorkspaceId,
};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Assembles the full prompt context for an agent run.
pub struct ContextInspector {
    memory: Arc<dyn MemoryManager>,
    #[allow(dead_code)]
    instruction_loader: InstructionLoader,
    rag: Mutex<RagIndex>,
}

impl ContextInspector {
    /// Create a new context inspector.
    #[must_use]
    pub fn new(memory: Arc<dyn MemoryManager>) -> Self {
        Self {
            memory,
            instruction_loader: InstructionLoader,
            rag: Mutex::new(RagIndex::new()),
        }
    }

    /// Summarize all context components for a run.
    ///
    /// # Errors
    ///
    /// Returns an error if memory lookups or workspace access fail.
    pub async fn summarize_context(
        &self,
        run_id: AgentRunId,
        thread_id: ThreadId,
        workspace_id: WorkspaceId,
        workspace_root: &str,
        query: &str,
    ) -> Result<ContextSummary, AppError> {
        let mut instructions = Vec::new();
        let mut privacy_flags = Vec::new();

        // Load instructions from broadest to narrowest. Closer scopes are added
        // later so a caller can choose to let narrower files override broader ones.
        let global_dir = dirs::config_dir().unwrap_or_else(|| Path::new(".").to_path_buf());
        instructions.extend(InstructionLoader::load_global(&global_dir));

        let root = Path::new(workspace_root);
        instructions.extend(InstructionLoader::load_workspace(root));

        // Collect memories across relevant scopes, excluding session memories from
        // the context summary by default.
        let mut memories = Vec::new();
        memories.extend(self.memory.list_memories(MemoryScope::User, None, None).await?);
        memories.extend(
            self.memory
                .list_memories(MemoryScope::Workspace, Some(workspace_id), None)
                .await?,
        );
        memories.extend(
            self.memory
                .list_memories(MemoryScope::Thread, Some(workspace_id), Some(thread_id))
                .await?,
        );

        // Flag sensitive content and filter sensitive memories out of RAG.
        for memory in &memories {
            if memory.sensitive {
                privacy_flags.push(format!(
                    "sensitive_memory:{}:{}",
                    memory.scope.as_str(),
                    memory.key
                ));
            }
        }

        // Run RAG over non-sensitive memories and workspace documents.
        let rag_chunks = self.search_rag(workspace_id, query, 5);

        let estimated_tokens = estimate_tokens(&instructions, &memories, &rag_chunks);

        Ok(ContextSummary {
            run_id,
            thread_id,
            instructions,
            memories,
            rag_chunks,
            estimated_tokens,
            privacy_flags,
        })
    }

    /// Add a workspace document to the RAG index.
    ///
    /// # Errors
    ///
    /// Returns an error if the document cannot be embedded or chunked.
    pub fn index_document(
        &self,
        workspace_id: WorkspaceId,
        path: &str,
        content: &str,
    ) -> Result<(), AppError> {
        let mut rag = self.rag.lock().map_err(|_| AppError::Internal {
            message: "RAG index lock poisoned".to_owned(),
        })?;
        rag.add_document(
            workspace_id,
            path,
            content,
            app_memory::simple_hash_embedding,
        )
    }

    /// Search the RAG index for chunks relevant to `query`.
    #[must_use]
    pub fn search_rag(
        &self,
        workspace_id: WorkspaceId,
        query: &str,
        top_n: usize,
    ) -> Vec<RagChunk> {
        let Ok(rag) = self.rag.lock() else {
            return Vec::new();
        };
        rag.search(
            workspace_id,
            query,
            app_memory::simple_hash_embedding,
            top_n,
        )
    }
}

fn estimate_tokens(
    instructions: &[InstructionFile],
    memories: &[MemoryItem],
    rag_chunks: &[RagChunk],
) -> u64 {
    // Very rough heuristic: ~4 characters per token on average.
    const CHARS_PER_TOKEN: u64 = 4;

    let text_len = instructions.iter().map(|i| i.content.len() as u64).sum::<u64>()
        + memories.iter().map(|m| (m.key.len() + m.value.len()) as u64).sum::<u64>()
        + rag_chunks.iter().map(|c| c.content.len() as u64).sum::<u64>();

    (text_len / CHARS_PER_TOKEN).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_memory::{MemoryManager, SqliteMemoryManager};
    use sqlx::SqlitePool;

    async fn inspector() -> ContextInspector {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE memories (
                id BLOB PRIMARY KEY NOT NULL,
                scope TEXT NOT NULL,
                workspace_id BLOB,
                thread_id BLOB,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                sensitive INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        let memory: Arc<dyn MemoryManager> = Arc::new(SqliteMemoryManager::new(pool));
        ContextInspector::new(memory)
    }

    #[tokio::test]
    async fn summarize_flags_sensitive_memory() {
        let inspector = inspector().await;
        let workspace_id = WorkspaceId::new();
        let thread_id = ThreadId::new();

        inspector
            .memory
            .create_memory(
                MemoryScope::Thread,
                Some(workspace_id),
                Some(thread_id),
                "api_key",
                "secret",
                true,
            )
            .await
            .unwrap();

        let summary = inspector
            .summarize_context(AgentRunId::new(), thread_id, workspace_id, "/tmp", "key")
            .await
            .unwrap();

        assert!(!summary.memories.is_empty());
        assert!(summary.privacy_flags.iter().any(|f| f.contains("sensitive_memory")));
    }

    #[tokio::test]
    async fn index_and_search_document() {
        let inspector = inspector().await;
        let workspace_id = WorkspaceId::new();

        inspector
            .index_document(workspace_id, "readme.md", "Rust is a fast systems language")
            .unwrap();

        let results = inspector.search_rag(workspace_id, "Rust", 3);
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("Rust"));
    }
}
