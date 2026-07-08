//! Instruction loader for `AGENTS.md` files.

use app_models::{AppError, InstructionFile};
use std::path::Path;

/// Maximum size for a single instruction file (1 MiB).
const MAX_INSTRUCTION_BYTES: u64 = 1_048_576;

/// Loads `AGENTS.md` instruction files from global, workspace, or directory scopes.
#[derive(Debug, Clone, Copy, Default)]
pub struct InstructionLoader;

impl InstructionLoader {
    /// Load global instructions from the application preferences directory.
    #[must_use]
    pub fn load_global(preferences_dir: &Path) -> Vec<InstructionFile> {
        Self::load_file(&preferences_dir.join("AGENTS.md"), "global")
    }

    /// Load workspace-level instructions from the workspace root.
    #[must_use]
    pub fn load_workspace(workspace_root: &Path) -> Vec<InstructionFile> {
        Self::load_file(&workspace_root.join("AGENTS.md"), "workspace")
    }

    /// Load directory-level instructions from a specific directory.
    #[must_use]
    pub fn load_directory(directory: &Path) -> Vec<InstructionFile> {
        Self::load_file(&directory.join("AGENTS.md"), "directory")
    }

    fn load_file(path: &Path, scope: &str) -> Vec<InstructionFile> {
        match Self::read_limited(path) {
            Ok(Some(content)) => vec![InstructionFile {
                path: path.to_string_lossy().into_owned(),
                content,
                scope: scope.to_owned(),
            }],
            Ok(None) => Vec::new(),
            Err(err) => {
                eprintln!(
                    "warning: failed to load instructions from {}: {err}",
                    path.display()
                );
                Vec::new()
            }
        }
    }

    /// Read a file if it exists, enforcing a size limit.
    ///
    /// Returns `Ok(None)` if the file does not exist.
    fn read_limited(path: &Path) -> Result<Option<String>, AppError> {
        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                return Err(AppError::Internal {
                    message: format!("failed to stat instruction file: {err}"),
                });
            }
        };

        if !metadata.is_file() {
            return Ok(None);
        }

        if metadata.len() > MAX_INSTRUCTION_BYTES {
            return Err(AppError::Internal {
                message: format!(
                    "instruction file exceeds {MAX_INSTRUCTION_BYTES} byte limit: {}",
                    path.display()
                ),
            });
        }

        let content = std::fs::read_to_string(path).map_err(|err| AppError::Internal {
            message: format!("failed to read instruction file: {err}"),
        })?;

        Ok(Some(content))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn load_global_reads_agents_md() {
        let dir = tempfile::tempdir().unwrap();
        let mut file = std::fs::File::create(dir.path().join("AGENTS.md")).unwrap();
        file.write_all(b"global instructions").unwrap();

        let instructions = InstructionLoader::load_global(dir.path());
        assert_eq!(instructions.len(), 1);
        assert_eq!(instructions[0].scope, "global");
        assert_eq!(instructions[0].content, "global instructions");
    }

    #[test]
    fn load_missing_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let instructions = InstructionLoader::load_workspace(dir.path());
        assert!(instructions.is_empty());
    }

    #[test]
    fn load_directory_reads_agents_md() {
        let dir = tempfile::tempdir().unwrap();
        let subdir = dir.path().join("src");
        std::fs::create_dir(&subdir).unwrap();
        std::fs::write(subdir.join("AGENTS.md"), b"src rules").unwrap();

        let instructions = InstructionLoader::load_directory(&subdir);
        assert_eq!(instructions.len(), 1);
        assert_eq!(instructions[0].scope, "directory");
        assert_eq!(instructions[0].content, "src rules");
    }
}
