//! Context inspector: assembles instructions, memories, and RAG chunks for a run.
//!
//! Also owns workspace indexer (disk scan → chunk → embed → persist) so RAG is not
//! a dead data panel: rebuild/index populates from the real project tree.

use app_memory::{
    format_behavior_policy_for_prompt, recall_memories, EmbeddingProvider, HashEmbeddingProvider,
    InstructionLoader, MemoryManager, MemoryRecallQuery, PatternStore, RagIndex, SqliteRagStore,
    StoredChunk,
};
use app_models::{
    AgentRunId, AppError, ContextBudget, ContextSummary, InstructionFile, MemoryItem,
    OutboundContextManifest, ProviderKind, RagChunk, RagDocumentMeta, RagDocumentStatus,
    RagIndexStatus, RagRefreshResult, ThreadId, WorkflowPattern, WorkspaceId,
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;
use std::sync::{Arc, Mutex};

const INDEX_MAX_FILE_BYTES: u64 = 256 * 1024;
const INDEX_MAX_FILES: usize = 400;
const INDEX_MAX_DEPTH: usize = 12;
const PROMPT_INSTRUCTION_CHARS: usize = 6_000;
const PROMPT_MEMORY_CHARS: usize = 3_000;
const PROMPT_RAG_CHARS: usize = 4_000;

const SKIP_DIR_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".turbo",
    "coverage",
    "__pycache__",
    ".venv",
    "venv",
    ".idea",
    ".vscode",
    "vendor",
];

const INDEX_EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "go", "java", "kt", "swift", "md", "mdx",
    "txt", "toml", "yaml", "yml", "json", "jsonc", "css", "scss", "html", "sql", "sh", "zsh",
    "bash", "env.example", "gitignore", "dockerfile", "makefile", "c", "h", "cpp", "hpp", "rb",
    "php", "cs", "xml", "gradle", "properties",
];

/// Assembles the full prompt context for an agent run.
pub struct ContextInspector {
    memory: Arc<dyn MemoryManager>,
    #[allow(dead_code)]
    instruction_loader: InstructionLoader,
    embedding: Arc<dyn EmbeddingProvider>,
    rag_store: SqliteRagStore,
    rag: Mutex<RagIndex>,
    loaded_workspaces: Mutex<HashSet<WorkspaceId>>,
    /// Optional pattern store for BehaviorPolicy synthesis.
    patterns: Mutex<Option<Arc<dyn PatternStore>>>,
}

impl ContextInspector {
    /// Create a new context inspector with the given embedding provider and RAG store.
    ///
    /// When `embedding` is `None`, a deterministic hash-based fallback is used.
    #[must_use]
    pub fn new(
        memory: Arc<dyn MemoryManager>,
        embedding: Option<Arc<dyn EmbeddingProvider>>,
        rag_store: SqliteRagStore,
    ) -> Self {
        Self {
            memory,
            instruction_loader: InstructionLoader,
            embedding: embedding.unwrap_or_else(|| Arc::new(HashEmbeddingProvider::new())),
            rag_store,
            rag: Mutex::new(RagIndex::new()),
            loaded_workspaces: Mutex::new(HashSet::new()),
            patterns: Mutex::new(None),
        }
    }

    /// Attach a pattern store so Active habits influence single-agent BehaviorPolicy.
    pub fn set_pattern_store(&self, patterns: Arc<dyn PatternStore>) {
        if let Ok(mut guard) = self.patterns.lock() {
            *guard = Some(patterns);
        }
    }

    /// Ensure chunks for a workspace are loaded into the in-memory cache.
    async fn ensure_workspace_loaded(&self, workspace_id: WorkspaceId) -> Result<(), AppError> {
        let already_loaded = {
            let loaded = self.loaded_workspaces.lock().map_err(|_| AppError::Internal {
                message: "loaded workspaces lock poisoned".to_owned(),
            })?;
            loaded.contains(&workspace_id)
        };

        if already_loaded {
            return Ok(());
        }

        let chunks = self
            .rag_store
            .load_matching_chunks(
                workspace_id,
                self.embedding.id(),
                self.embedding.dimension(),
            )
            .await?;

        {
            let mut rag = self.rag.lock().map_err(|_| AppError::Internal {
                message: "RAG index lock poisoned".to_owned(),
            })?;
            // Append to existing cache rather than replace, because multiple
            // workspaces may be loaded over time.
            rag.append_chunks(chunks);
        }

        {
            let mut loaded = self.loaded_workspaces.lock().map_err(|_| AppError::Internal {
                message: "loaded workspaces lock poisoned".to_owned(),
            })?;
            loaded.insert(workspace_id);
        }

        Ok(())
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

        // Collect memories across relevant scopes, then rank via recall engine.
        let all_memories = self
            .memory
            .list_for_recall(workspace_id, thread_id)
            .await
            .unwrap_or_else(|_| {
                // Fallback path if list_for_recall is unavailable in older managers.
                Vec::new()
            });

        let active_patterns = self.load_active_patterns(workspace_id, query).await;
        let recall = recall_memories(
            &MemoryRecallQuery {
                task: query.to_owned(),
                workspace_id,
                thread_id,
                limit: 12,
                max_tokens: PROMPT_MEMORY_CHARS / 4,
            },
            &all_memories,
            &active_patterns,
        );
        let memories: Vec<MemoryItem> = recall.hits.iter().map(|h| h.memory.clone()).collect();
        let recalled_memory_ids: Vec<_> = recall.hits.iter().map(|h| h.memory.id).collect();
        let pattern_ids: Vec<_> = active_patterns.iter().map(|p| p.id).collect();
        let behavior_policy = recall.policy;

        // Flag sensitive content; sensitive memories stay excluded from model prompts.
        for memory in &all_memories {
            if memory.sensitive {
                privacy_flags.push(format!(
                    "sensitive_memory:{}:{}",
                    memory.scope.as_str(),
                    memory.key
                ));
            }
        }
        for (id, reason) in &recall.excluded {
            if reason == "sensitive_blocked" {
                privacy_flags.push(format!("excluded_memory:{}:{reason}", id.0));
            }
        }

        // Run RAG over workspace documents.
        let rag_chunks = self.search_rag(workspace_id, query, 5).await;

        let estimated_tokens = estimate_tokens(&instructions, &memories, &rag_chunks);
        let context_budget = ContextBudget {
            model_context_tokens: 32_000,
            reserved_output_tokens: 2_048,
            reserved_tool_tokens: 4_096,
            transcript_tokens: 8_000,
            instruction_tokens: (PROMPT_INSTRUCTION_CHARS / 4) as u64,
            memory_tokens: (PROMPT_MEMORY_CHARS / 4) as u64,
            rag_tokens: (PROMPT_RAG_CHARS / 4) as u64,
        };
        let outbound_manifest = OutboundContextManifest {
            provider_kind: ProviderKind::Custom,
            local_provider: false,
            message_count: 0,
            memory_ids: recalled_memory_ids.clone(),
            rag_paths: rag_chunks.iter().map(|c| c.document_path.clone()).collect(),
            total_bytes: estimated_tokens.saturating_mul(4),
            sensitive_content_blocked: !privacy_flags.is_empty(),
        };

        // Best-effort usage attribution.
        for id in &recalled_memory_ids {
            let _ = self.memory.record_memory_use(*id).await;
        }

        Ok(ContextSummary {
            run_id,
            thread_id,
            instructions,
            memories,
            rag_chunks,
            estimated_tokens,
            privacy_flags,
            recalled_memory_ids,
            pattern_ids,
            behavior_policy: Some(behavior_policy),
            outbound_manifest: Some(outbound_manifest),
            context_budget: Some(context_budget),
        })
    }

    async fn load_active_patterns(
        &self,
        workspace_id: WorkspaceId,
        query: &str,
    ) -> Vec<WorkflowPattern> {
        let store = {
            let guard = self.patterns.lock().ok();
            guard.and_then(|g| g.clone())
        };
        let Some(store) = store else {
            return Vec::new();
        };
        let hints = store
            .recall_patterns(query, Some(workspace_id), 5)
            .await
            .unwrap_or_default();
        let mut out = Vec::new();
        for hint in hints {
            if let Ok(p) = store.get_pattern(hint.id).await {
                out.push(p);
            }
        }
        out
    }

    /// Build a prompt-safe context block (non-sensitive only) for agent execution.
    ///
    /// Sensitive memories are never included. RAG and instructions are budget-capped.
    /// BehaviorPolicy from accepted memories / Active patterns is injected first.
    pub async fn assemble_prompt_context(
        &self,
        thread_id: ThreadId,
        workspace_id: WorkspaceId,
        workspace_root: &str,
        query: &str,
    ) -> String {
        let summary = self
            .summarize_context(
                AgentRunId::new(),
                thread_id,
                workspace_id,
                workspace_root,
                query,
            )
            .await;
        let Ok(summary) = summary else {
            return String::new();
        };

        let mut parts = Vec::new();

        if let Some(policy) = &summary.behavior_policy {
            let block = format_behavior_policy_for_prompt(policy);
            if !block.is_empty() {
                parts.push(block);
            }
        }

        let mut instruction_budget = PROMPT_INSTRUCTION_CHARS;
        let mut instruction_block = String::new();
        for file in &summary.instructions {
            if instruction_budget == 0 {
                break;
            }
            let body = truncate_chars(&file.content, instruction_budget);
            let used = body.chars().count();
            instruction_budget = instruction_budget.saturating_sub(used);
            instruction_block.push_str("### ");
            instruction_block.push_str(&file.path);
            instruction_block.push_str(" (");
            instruction_block.push_str(&file.scope);
            instruction_block.push_str(")\n");
            instruction_block.push_str(&body);
            instruction_block.push_str("\n\n");
        }
        if !instruction_block.trim().is_empty() {
            parts.push(format!("## Project instructions\n{instruction_block}"));
        }

        let mut memory_budget = PROMPT_MEMORY_CHARS;
        let mut memory_block = String::new();
        for memory in summary.memories.iter().filter(|m| !m.sensitive) {
            if memory_budget == 0 {
                break;
            }
            let line = format!("- [{}] {}: {}\n", memory.scope.as_str(), memory.key, memory.value);
            let used = line.chars().count();
            if used > memory_budget {
                memory_block.push_str(&truncate_chars(&line, memory_budget));
                memory_budget = 0;
            } else {
                memory_block.push_str(&line);
                memory_budget = memory_budget.saturating_sub(used);
            }
        }
        if !memory_block.trim().is_empty() {
            parts.push(format!("## Remembered facts (non-sensitive)\n{memory_block}"));
        }

        let mut rag_budget = PROMPT_RAG_CHARS;
        let mut rag_block = String::new();
        for chunk in &summary.rag_chunks {
            if rag_budget == 0 {
                break;
            }
            let body = truncate_chars(&chunk.content, rag_budget.min(800));
            let used = body.chars().count() + chunk.document_path.chars().count() + 32;
            rag_budget = rag_budget.saturating_sub(used);
            let _ = write!(
                rag_block,
                "### {}#{} (score {:.2})\n{}\n\n",
                chunk.document_path, chunk.chunk_index, chunk.score, body
            );
        }
        if !rag_block.trim().is_empty() {
            parts.push(format!(
                "## Relevant project excerpts (cite paths; do not invent)\n{rag_block}"
            ));
        }

        parts.join("\n")
    }

    /// Assemble context and return both the prompt string and the frozen summary
    /// (for run_context_snapshots attribution).
    pub async fn assemble_prompt_context_with_summary(
        &self,
        run_id: AgentRunId,
        thread_id: ThreadId,
        workspace_id: WorkspaceId,
        workspace_root: &str,
        query: &str,
    ) -> (String, Option<ContextSummary>) {
        let summary = self
            .summarize_context(run_id, thread_id, workspace_id, workspace_root, query)
            .await
            .ok();
        let prompt = if let Some(ref s) = summary {
            let mut parts = Vec::new();
            if let Some(policy) = &s.behavior_policy {
                let block = format_behavior_policy_for_prompt(policy);
                if !block.is_empty() {
                    parts.push(block);
                }
            }
            // Reuse the simpler path for the rest by calling assemble without re-summarize
            // would double-call RAG; instead format from summary directly.
            let rest = self.format_summary_parts(s);
            if !rest.is_empty() {
                parts.push(rest);
            }
            parts.join("\n")
        } else {
            String::new()
        };
        (prompt, summary)
    }

    fn format_summary_parts(&self, summary: &ContextSummary) -> String {
        let mut parts = Vec::new();
        let mut instruction_budget = PROMPT_INSTRUCTION_CHARS;
        let mut instruction_block = String::new();
        for file in &summary.instructions {
            if instruction_budget == 0 {
                break;
            }
            let body = truncate_chars(&file.content, instruction_budget);
            let used = body.chars().count();
            instruction_budget = instruction_budget.saturating_sub(used);
            let _ = write!(
                instruction_block,
                "### {} ({})\n{}\n\n",
                file.path, file.scope, body
            );
        }
        if !instruction_block.trim().is_empty() {
            parts.push(format!("## Project instructions\n{instruction_block}"));
        }
        let mut memory_budget = PROMPT_MEMORY_CHARS;
        let mut memory_block = String::new();
        for memory in summary.memories.iter().filter(|m| !m.sensitive) {
            if memory_budget == 0 {
                break;
            }
            let line = format!("- [{}] {}: {}\n", memory.scope.as_str(), memory.key, memory.value);
            let used = line.chars().count();
            if used > memory_budget {
                memory_block.push_str(&truncate_chars(&line, memory_budget));
                memory_budget = 0;
            } else {
                memory_block.push_str(&line);
                memory_budget = memory_budget.saturating_sub(used);
            }
        }
        if !memory_block.trim().is_empty() {
            parts.push(format!("## Remembered facts (non-sensitive)\n{memory_block}"));
        }
        let mut rag_budget = PROMPT_RAG_CHARS;
        let mut rag_block = String::new();
        for chunk in &summary.rag_chunks {
            if rag_budget == 0 {
                break;
            }
            let body = truncate_chars(&chunk.content, rag_budget.min(800));
            let used = body.chars().count() + chunk.document_path.chars().count() + 32;
            rag_budget = rag_budget.saturating_sub(used);
            let _ = write!(
                rag_block,
                "### {}#{} (score {:.2})\n{}\n\n",
                chunk.document_path, chunk.chunk_index, chunk.score, body
            );
        }
        if !rag_block.trim().is_empty() {
            parts.push(format!(
                "## Relevant project excerpts (cite paths; do not invent)\n{rag_block}"
            ));
        }
        parts.join("\n")
    }

    /// Scan a workspace directory from disk and (re)build the RAG index.
    ///
    /// Clears existing chunks for the workspace, then indexes text-like files
    /// under ignore rules (`node_modules`, `target`, `.git`, …).
    ///
    /// # Errors
    ///
    /// Returns an error if the root is invalid or embedding/persistence fails.
    pub async fn index_workspace_from_disk(
        &self,
        workspace_id: WorkspaceId,
        workspace_root: &str,
    ) -> Result<usize, AppError> {
        let root = Path::new(workspace_root)
            .canonicalize()
            .map_err(|e| AppError::Internal {
                message: format!("workspace root for indexing is unavailable: {e}"),
            })?;
        if !root.is_dir() {
            return Err(AppError::Internal {
                message: "workspace root for indexing is not a directory".to_owned(),
            });
        }

        self.rag_store.clear_workspace(workspace_id).await?;
        {
            let mut loaded = self.loaded_workspaces.lock().map_err(|_| AppError::Internal {
                message: "loaded workspaces lock poisoned".to_owned(),
            })?;
            loaded.remove(&workspace_id);
        }

        let mut files = Vec::new();
        collect_indexable_files(&root, &root, 0, &mut files)?;
        let mut total_chunks = 0usize;
        for relative in files {
            let absolute = root.join(&relative);
            let Ok(content) = std::fs::read_to_string(&absolute) else {
                continue;
            };
            if content.trim().is_empty() || content.contains('\0') {
                continue;
            }
            match self
                .index_document(workspace_id, &relative.replace('\\', "/"), &content)
                .await
            {
                Ok(()) => {
                    total_chunks = total_chunks.saturating_add(1);
                }
                Err(err) => {
                    tracing::warn!(
                        path = %relative,
                        error = %err,
                        "skip document during workspace index"
                    );
                }
            }
        }

        // Force reload cache after bulk insert.
        {
            let mut loaded = self.loaded_workspaces.lock().map_err(|_| AppError::Internal {
                message: "loaded workspaces lock poisoned".to_owned(),
            })?;
            loaded.remove(&workspace_id);
        }
        self.ensure_workspace_loaded(workspace_id).await?;
        Ok(total_chunks)
    }

    /// Add a workspace document to the RAG index.
    ///
    /// # Errors
    ///
    /// Returns an error if the document cannot be embedded or chunked.
    pub async fn index_document(
        &self,
        workspace_id: WorkspaceId,
        path: &str,
        content: &str,
    ) -> Result<(), AppError> {
        if content.is_empty() {
            return Err(AppError::Internal {
                message: "cannot index empty document".to_owned(),
            });
        }

        // Chunk first to know how many embeddings we need.
        let texts = RagIndex::split_document(content);
        if texts.is_empty() {
            return Ok(());
        }

        let embeddings = self.embedding.embed(&texts).await?;

        if embeddings.len() != texts.len() {
            return Err(AppError::Internal {
                message: format!(
                    "embedding count {} does not match chunk count {}",
                    embeddings.len(),
                    texts.len()
                ),
            });
        }

        for embedding in &embeddings {
            if embedding.len() != self.embedding.dimension() {
                return Err(AppError::Internal {
                    message: format!(
                        "embedding dimension {} does not match provider dimension {}",
                        embedding.len(),
                        self.embedding.dimension()
                    ),
                });
            }
        }

        let stored_chunks = RagIndex::build_chunks(workspace_id, path, texts, embeddings)?;
        let chunk_count = stored_chunks.len();

        self.rag_store
            .replace_document_chunks(
                workspace_id,
                path,
                self.embedding.id(),
                self.embedding.dimension(),
                stored_chunks,
            )
            .await?;

        let content_hash = content_sha256(content);
        let _ = self
            .rag_store
            .upsert_document_meta(&RagDocumentMeta {
                workspace_id,
                document_path: path.to_owned(),
                content_hash,
                file_size: i64::try_from(content.len()).unwrap_or(0),
                modified_at: Some(Utc::now()),
                embedding_provider_id: self.embedding.id().to_owned(),
                dimension: i64::try_from(self.embedding.dimension()).unwrap_or(0),
                status: RagDocumentStatus::Indexed,
                indexed_at: Some(Utc::now()),
                last_error: None,
            })
            .await;

        // Invalidate cache so subsequent searches reload.
        {
            let mut loaded = self.loaded_workspaces.lock().map_err(|_| AppError::Internal {
                message: "loaded workspaces lock poisoned".to_owned(),
            })?;
            loaded.remove(&workspace_id);
        }
        self.ensure_workspace_loaded(workspace_id).await?;
        let _ = chunk_count;
        Ok(())
    }

    /// Incremental scan: only reindex files whose content hash changed.
    ///
    /// Does not clear the whole workspace. Deleted files drop their chunks.
    pub async fn refresh_changed_documents(
        &self,
        workspace_id: WorkspaceId,
        workspace_root: &str,
    ) -> Result<RagRefreshResult, AppError> {
        let root = Path::new(workspace_root)
            .canonicalize()
            .map_err(|e| AppError::Internal {
                message: format!("workspace root for incremental index is unavailable: {e}"),
            })?;
        if !root.is_dir() {
            return Err(AppError::Internal {
                message: "workspace root for incremental index is not a directory".to_owned(),
            });
        }

        let provider_id = self.embedding.id().to_owned();
        let dimension = self.embedding.dimension();
        let _ = self
            .rag_store
            .mark_needs_reindex_for_provider(workspace_id, &provider_id, dimension)
            .await;

        let mut files = Vec::new();
        collect_indexable_files(&root, &root, 0, &mut files)?;
        let existing = self.rag_store.list_document_meta(workspace_id).await?;
        let existing_map: HashMap<String, RagDocumentMeta> = existing
            .into_iter()
            .map(|m| (m.document_path.clone(), m))
            .collect();

        let mut result = RagRefreshResult {
            workspace_id,
            scanned: 0,
            added: 0,
            updated: 0,
            removed: 0,
            unchanged: 0,
            failed: 0,
        };

        let mut seen = HashSet::new();
        for relative in files {
            let path = relative.replace('\\', "/");
            seen.insert(path.clone());
            let absolute = root.join(&relative);
            let Ok(content) = std::fs::read_to_string(&absolute) else {
                continue;
            };
            if content.trim().is_empty() || content.contains('\0') {
                continue;
            }
            result.scanned += 1;
            let hash = content_sha256(&content);
            let file_size = i64::try_from(content.len()).unwrap_or(0);
            let prev = existing_map.get(&path);
            let needs_update = match prev {
                None => true,
                Some(meta)
                    if meta.content_hash != hash
                        || meta.embedding_provider_id != provider_id
                        || meta.dimension != i64::try_from(dimension).unwrap_or(0)
                        || matches!(
                            meta.status,
                            RagDocumentStatus::Failed
                                | RagDocumentStatus::NeedsReindex
                                | RagDocumentStatus::Stale
                        ) =>
                {
                    true
                }
                Some(_) => false,
            };
            if !needs_update {
                result.unchanged += 1;
                continue;
            }
            let is_new = prev.is_none();
            match self.index_document(workspace_id, &path, &content).await {
                Ok(()) => {
                    if is_new {
                        result.added += 1;
                    } else {
                        result.updated += 1;
                    }
                    let _ = file_size;
                }
                Err(err) => {
                    result.failed += 1;
                    let _ = self
                        .rag_store
                        .upsert_document_meta(&RagDocumentMeta {
                            workspace_id,
                            document_path: path,
                            content_hash: hash,
                            file_size,
                            modified_at: Some(Utc::now()),
                            embedding_provider_id: provider_id.clone(),
                            dimension: i64::try_from(dimension).unwrap_or(0),
                            status: RagDocumentStatus::Failed,
                            indexed_at: prev.and_then(|p| p.indexed_at),
                            last_error: Some(err.to_string()),
                        })
                        .await;
                }
            }
        }

        // Remove deleted files' chunks + metadata.
        let stale_paths: Vec<String> = existing_map
            .keys()
            .filter(|p| !seen.contains(*p))
            .cloned()
            .collect();
        for path in stale_paths {
            let _ = self.rag_store.delete_document(workspace_id, &path).await;
            let _ = self.rag_store.delete_document_meta(workspace_id, &path).await;
            result.removed += 1;
        }

        {
            let mut loaded = self.loaded_workspaces.lock().map_err(|_| AppError::Internal {
                message: "loaded workspaces lock poisoned".to_owned(),
            })?;
            loaded.remove(&workspace_id);
        }
        let _ = self.ensure_workspace_loaded(workspace_id).await;
        Ok(result)
    }

    /// RAG index status for Inspector / capabilities.
    pub async fn get_rag_index_status(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<RagIndexStatus, AppError> {
        self.rag_store
            .index_status(
                workspace_id,
                self.embedding.id(),
                self.embedding.dimension(),
            )
            .await
    }

    /// Search the RAG index for chunks relevant to `query`.
    ///
    /// # Errors
    ///
    /// Returns an empty list if the workspace cannot be loaded.
    pub async fn search_rag(
        &self,
        workspace_id: WorkspaceId,
        query: &str,
        top_n: usize,
    ) -> Vec<RagChunk> {
        if let Err(err) = self.ensure_workspace_loaded(workspace_id).await {
            tracing::warn!(workspace_id = ?workspace_id, error = %err, "failed to load RAG workspace");
            return Vec::new();
        }

        let query_embeddings = match self.embedding.embed(&[query.to_owned()]).await {
            Ok(vectors) => vectors,
            Err(err) => {
                tracing::warn!(error = %err, "failed to embed RAG query");
                return Vec::new();
            }
        };

        let Some(query_embedding) = query_embeddings.into_iter().next() else {
            return Vec::new();
        };

        let Ok(rag) = self.rag.lock() else {
            return Vec::new();
        };
        rag.search(workspace_id, &query_embedding, top_n)
    }

    /// Access the underlying RAG store.
    #[must_use]
    pub const fn rag_store(&self) -> &SqliteRagStore {
        &self.rag_store
    }

    /// Access the embedding provider.
    #[must_use]
    pub fn embedding_provider(&self) -> &Arc<dyn EmbeddingProvider> {
        &self.embedding
    }

    /// Rebuild the RAG index for a workspace.
    ///
    /// When `workspace_root` is provided, performs a full disk scan (preferred).
    /// Otherwise re-embeds previously stored chunk contents (legacy path).
    ///
    /// # Errors
    ///
    /// Returns an error if clearing, reading, embedding, or inserting fails.
    pub async fn rebuild_workspace(
        &self,
        workspace_id: WorkspaceId,
        workspace_root: Option<&str>,
    ) -> Result<usize, AppError> {
        if let Some(root) = workspace_root {
            return self.index_workspace_from_disk(workspace_id, root).await;
        }

        let contents = self.rag_store.load_all_contents(workspace_id).await?;
        if contents.is_empty() {
            return Ok(0);
        }

        self.rag_store.clear_workspace(workspace_id).await?;

        let mut total = 0usize;
        let provider_id = self.embedding.id();
        let dimension = self.embedding.dimension();

        for (document_path, chunk_index, content) in contents {
            let embeddings = self.embedding.embed(std::slice::from_ref(&content)).await?;
            let Some(embedding) = embeddings.into_iter().next() else {
                continue;
            };
            if embedding.len() != dimension {
                return Err(AppError::Internal {
                    message: format!(
                        "rebuild produced dimension {} but provider '{}' expects {}",
                        embedding.len(),
                        provider_id,
                        dimension
                    ),
                });
            }

            let chunk = StoredChunk::new(
                0,
                document_path.clone(),
                chunk_index,
                content,
                workspace_id,
                embedding,
            );

            self.rag_store
                .insert_chunks(
                    workspace_id,
                    &document_path,
                    provider_id,
                    dimension,
                    vec![chunk],
                )
                .await?;
            total += 1;
        }

        // Refresh cache: remove workspace from loaded set and reload.
        {
            let mut loaded = self.loaded_workspaces.lock().map_err(|_| AppError::Internal {
                message: "loaded workspaces lock poisoned".to_owned(),
            })?;
            loaded.remove(&workspace_id);
        }
        self.ensure_workspace_loaded(workspace_id).await?;

        Ok(total)
    }
}

fn content_sha256(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
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

fn truncate_chars(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_owned();
    }
    let mut out: String = input.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn collect_indexable_files(
    root: &Path,
    current: &Path,
    depth: usize,
    out: &mut Vec<String>,
) -> Result<(), AppError> {
    if out.len() >= INDEX_MAX_FILES || depth > INDEX_MAX_DEPTH {
        return Ok(());
    }
    let entries = std::fs::read_dir(current).map_err(|e| AppError::Internal {
        message: format!("index walk failed at {}: {e}", current.display()),
    })?;
    for entry in entries {
        if out.len() >= INDEX_MAX_FILES {
            break;
        }
        let entry = entry.map_err(|e| AppError::Internal {
            message: format!("index walk entry failed: {e}"),
        })?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') && name != ".gitignore" && name != ".env.example" {
            // Still allow common config files; skip hidden dirs/files by default.
            if path.is_dir() {
                continue;
            }
        }
        let file_type = entry.file_type().map_err(|e| AppError::Internal {
            message: format!("index walk file type failed: {e}"),
        })?;
        if file_type.is_dir() {
            if SKIP_DIR_NAMES.iter().any(|skip| name.eq_ignore_ascii_case(skip)) {
                continue;
            }
            collect_indexable_files(root, &path, depth + 1, out)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        if !is_indexable_file(&path) {
            continue;
        }
        let metadata = entry.metadata().map_err(|e| AppError::Internal {
            message: format!("index walk metadata failed: {e}"),
        })?;
        if metadata.len() == 0 || metadata.len() > INDEX_MAX_FILE_BYTES {
            continue;
        }
        let relative = path.strip_prefix(root).unwrap_or(&path);
        out.push(relative.to_string_lossy().into_owned());
    }
    Ok(())
}

fn is_indexable_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(
        name.as_str(),
        "makefile" | "dockerfile" | "agents.md" | "readme" | "readme.md" | "license" | "cargo.toml"
            | "package.json" | "pnpm-workspace.yaml" | "go.mod"
    ) {
        return true;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    INDEX_EXTENSIONS.iter().any(|allowed| *allowed == ext)
}



#[cfg(test)]
mod tests {
    use super::*;
    use app_memory::{MemoryManager, SqliteMemoryManager};
    use app_models::MemoryScope;
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
                updated_at TEXT NOT NULL,
                kind TEXT,
                encrypted_value BLOB,
                encryption_version INTEGER,
                source_run_id BLOB,
                confidence REAL,
                last_used_at TEXT,
                use_count INTEGER NOT NULL DEFAULT 0
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE rag_chunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                workspace_id BLOB NOT NULL,
                document_path TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                content TEXT NOT NULL,
                embedding BLOB,
                embedding_provider_id TEXT,
                dimension INTEGER,
                created_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        let memory: Arc<dyn MemoryManager> = Arc::new(SqliteMemoryManager::new(pool.clone()));
        let rag_store = SqliteRagStore::new(pool);
        ContextInspector::new(memory, None, rag_store)
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
        inspector
            .memory
            .create_memory(
                MemoryScope::User,
                None,
                None,
                "response_language",
                "zh-CN",
                false,
            )
            .await
            .unwrap();

        let summary = inspector
            .summarize_context(AgentRunId::new(), thread_id, workspace_id, "/tmp", "key")
            .await
            .unwrap();

        // Sensitive items never enter the model-facing memory list.
        assert!(summary.memories.iter().all(|m| !m.sensitive));
        assert!(summary.privacy_flags.iter().any(|f| f.contains("sensitive_memory")));
    }

    #[tokio::test]
    async fn index_and_search_document() {
        let inspector = inspector().await;
        let workspace_id = WorkspaceId::new();

        inspector
            .index_document(workspace_id, "readme.md", "Rust is a fast systems language")
            .await
            .unwrap();

        let results = inspector.search_rag(workspace_id, "Rust", 3).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("Rust"));
    }
}
