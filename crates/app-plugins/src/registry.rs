//! Plugin and skill registry for Portico.

use app_models::{
    AppError, McpServerConfig, PluginId, PluginManifest, PluginPermissions, Skill, SkillId,
};
use async_trait::async_trait;
use serde_json;
use sqlx::{FromRow, SqlitePool};

/// Registry for plugins, their skills, and persisted MCP server configurations.
#[async_trait]
pub trait PluginRegistry: Send + Sync {
    /// Install or update a plugin manifest.
    ///
    /// If a plugin with the same id already exists it is replaced.
    async fn install_plugin(&self, manifest: PluginManifest) -> Result<PluginManifest, AppError>;

    /// List all installed plugins.
    async fn list_plugins(&self) -> Result<Vec<PluginManifest>, AppError>;

    /// Enable or disable a plugin.
    async fn enable_plugin(&self, id: PluginId, enabled: bool) -> Result<(), AppError>;

    /// Uninstall a plugin and its skills.
    async fn uninstall_plugin(&self, id: PluginId) -> Result<(), AppError>;

    /// Register a skill.
    async fn register_skill(&self, skill: Skill) -> Result<Skill, AppError>;

    /// List skills, optionally filtered to a single plugin.
    async fn list_skills(&self, plugin_id: Option<PluginId>) -> Result<Vec<Skill>, AppError>;

    /// Persist a new MCP server configuration.
    async fn add_mcp_server(&self, config: McpServerConfig) -> Result<McpServerConfig, AppError>;

    /// Remove an MCP server configuration by id.
    async fn remove_mcp_server(&self, id: i64) -> Result<(), AppError>;

    /// List persisted MCP server configurations.
    async fn list_mcp_servers(&self) -> Result<Vec<McpServerConfig>, AppError>;
}

/// SQLite-backed implementation of [`PluginRegistry`].
#[derive(Debug, Clone)]
pub struct SqlitePluginRegistry {
    pool: SqlitePool,
}

impl SqlitePluginRegistry {
    /// Create a new registry around an existing `SQLite` pool.
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Open an in-memory registry for tests, running all migrations.
    ///
    /// # Errors
    ///
    /// Returns an error if the in-memory database cannot be set up.
    pub async fn open_in_memory() -> Result<Self, AppError> {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(SqliteConnectOptions::new().filename(":memory:"))
            .await
            .map_err(|e| AppError::Internal {
                message: format!("failed to connect to in-memory sqlite: {e}"),
            })?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS plugins (
                id BLOB PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                display_name TEXT NOT NULL,
                description TEXT NOT NULL,
                skills TEXT NOT NULL,
                tools TEXT NOT NULL,
                entrypoint TEXT,
                capabilities TEXT NOT NULL DEFAULT '[]',
                install_path TEXT,
                permissions TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                installed_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("create plugins table failed: {e}"),
        })?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS skills (
                id BLOB PRIMARY KEY NOT NULL,
                plugin_id BLOB NOT NULL,
                name TEXT NOT NULL,
                description TEXT NOT NULL,
                trigger_description TEXT NOT NULL,
                instruction_file TEXT,
                required_tools TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("create skills table failed: {e}"),
        })?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS mcp_servers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                transport TEXT NOT NULL,
                command TEXT,
                args TEXT NOT NULL,
                url TEXT,
                env TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1
            )",
        )
        .execute(&pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("create mcp_servers table failed: {e}"),
        })?;

        Ok(Self::new(pool))
    }
}

#[async_trait]
impl PluginRegistry for SqlitePluginRegistry {
    async fn install_plugin(&self, manifest: PluginManifest) -> Result<PluginManifest, AppError> {
        let skills_json =
            serde_json::to_string(&manifest.skills).map_err(|e| AppError::Internal {
                message: format!("serialize skills failed: {e}"),
            })?;
        let tools_json =
            serde_json::to_string(&manifest.tools).map_err(|e| AppError::Internal {
                message: format!("serialize tools failed: {e}"),
            })?;
        let permissions_json =
            serde_json::to_string(&manifest.permissions).map_err(|e| AppError::Internal {
                message: format!("serialize permissions failed: {e}"),
            })?;
        let capabilities_json =
            serde_json::to_string(&manifest.capabilities).map_err(|e| AppError::Internal {
                message: format!("serialize capabilities failed: {e}"),
            })?;

        sqlx::query(
            "INSERT INTO plugins (
                id, name, version, display_name, description, skills, tools, entrypoint,
                capabilities, install_path, permissions, enabled, installed_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                version = excluded.version,
                display_name = excluded.display_name,
                description = excluded.description,
                skills = excluded.skills,
                tools = excluded.tools,
                entrypoint = excluded.entrypoint,
                capabilities = excluded.capabilities,
                install_path = excluded.install_path,
                permissions = excluded.permissions,
                enabled = excluded.enabled,
                installed_at = excluded.installed_at",
        )
        .bind(manifest.id.0)
        .bind(&manifest.name)
        .bind(&manifest.version)
        .bind(&manifest.display_name)
        .bind(&manifest.description)
        .bind(skills_json)
        .bind(tools_json)
        .bind(&manifest.entrypoint)
        .bind(capabilities_json)
        .bind(&manifest.install_path)
        .bind(permissions_json)
        .bind(i64::from(manifest.enabled))
        .bind(manifest.installed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("install_plugin failed: {e}"),
        })?;

        Ok(manifest)
    }

    async fn list_plugins(&self) -> Result<Vec<PluginManifest>, AppError> {
        let rows = sqlx::query_as::<_, PluginRow>(
            "SELECT id, name, version, display_name, description, skills, tools, entrypoint,
                    capabilities, install_path, permissions, enabled, installed_at
             FROM plugins
             ORDER BY installed_at ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("list_plugins failed: {e}"),
        })?;

        rows.into_iter().map(PluginRow::try_into).collect()
    }

    async fn enable_plugin(&self, id: PluginId, enabled: bool) -> Result<(), AppError> {
        let result = sqlx::query("UPDATE plugins SET enabled = ? WHERE id = ?")
            .bind(i64::from(enabled))
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("enable_plugin failed: {e}"),
            })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                resource: format!("plugin {id:?}"),
            });
        }
        Ok(())
    }

    async fn uninstall_plugin(&self, id: PluginId) -> Result<(), AppError> {
        let mut tx = self.pool.begin().await.map_err(|e| AppError::Internal {
            message: format!("uninstall_plugin transaction failed: {e}"),
        })?;

        sqlx::query("DELETE FROM skills WHERE plugin_id = ?")
            .bind(id.0)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("uninstall_plugin delete skills failed: {e}"),
            })?;

        let result = sqlx::query("DELETE FROM plugins WHERE id = ?")
            .bind(id.0)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("uninstall_plugin delete plugin failed: {e}"),
            })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                resource: format!("plugin {id:?}"),
            });
        }

        tx.commit().await.map_err(|e| AppError::Internal {
            message: format!("uninstall_plugin commit failed: {e}"),
        })
    }

    async fn register_skill(&self, skill: Skill) -> Result<Skill, AppError> {
        let required_tools_json =
            serde_json::to_string(&skill.required_tools).map_err(|e| AppError::Internal {
                message: format!("serialize required_tools failed: {e}"),
            })?;

        sqlx::query(
            "INSERT INTO skills (
                id, plugin_id, name, description, trigger_description,
                instruction_file, required_tools
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                plugin_id = excluded.plugin_id,
                name = excluded.name,
                description = excluded.description,
                trigger_description = excluded.trigger_description,
                instruction_file = excluded.instruction_file,
                required_tools = excluded.required_tools",
        )
        .bind(skill.id.0)
        .bind(skill.plugin_id.0)
        .bind(&skill.name)
        .bind(&skill.description)
        .bind(&skill.trigger_description)
        .bind(&skill.instruction_file)
        .bind(required_tools_json)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("register_skill failed: {e}"),
        })?;

        Ok(skill)
    }

    async fn list_skills(&self, plugin_id: Option<PluginId>) -> Result<Vec<Skill>, AppError> {
        let rows = if let Some(plugin_id) = plugin_id {
            sqlx::query_as::<_, SkillRow>(
                "SELECT id, plugin_id, name, description, trigger_description,
                        instruction_file, required_tools
                 FROM skills
                 WHERE plugin_id = ?
                 ORDER BY name ASC",
            )
            .bind(plugin_id.0)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, SkillRow>(
                "SELECT id, plugin_id, name, description, trigger_description,
                        instruction_file, required_tools
                 FROM skills
                 ORDER BY name ASC",
            )
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|e| AppError::Internal {
            message: format!("list_skills failed: {e}"),
        })?;

        rows.into_iter().map(SkillRow::try_into).collect()
    }

    async fn add_mcp_server(
        &self,
        mut config: McpServerConfig,
    ) -> Result<McpServerConfig, AppError> {
        let args_json = serde_json::to_string(&config.args).map_err(|e| AppError::Internal {
            message: format!("serialize args failed: {e}"),
        })?;
        let env_json = serde_json::to_string(&config.env).map_err(|e| AppError::Internal {
            message: format!("serialize env failed: {e}"),
        })?;

        let id = sqlx::query(
            "INSERT INTO mcp_servers (
                name, transport, command, args, url, env, enabled
            ) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&config.name)
        .bind(config.transport.as_str())
        .bind(&config.command)
        .bind(args_json)
        .bind(&config.url)
        .bind(env_json)
        .bind(i64::from(config.enabled))
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("add_mcp_server failed: {e}"),
        })?
        .last_insert_rowid();

        config.id = id;
        Ok(config)
    }

    async fn remove_mcp_server(&self, id: i64) -> Result<(), AppError> {
        let result = sqlx::query("DELETE FROM mcp_servers WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("remove_mcp_server failed: {e}"),
            })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                resource: format!("mcp server {id}"),
            });
        }
        Ok(())
    }

    async fn list_mcp_servers(&self) -> Result<Vec<McpServerConfig>, AppError> {
        let rows = sqlx::query_as::<_, McpServerRow>(
            "SELECT id, name, transport, command, args, url, env, enabled
             FROM mcp_servers
             ORDER BY id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("list_mcp_servers failed: {e}"),
        })?;

        rows.into_iter().map(McpServerRow::try_into).collect()
    }
}

#[derive(FromRow)]
struct PluginRow {
    id: uuid::Uuid,
    name: String,
    version: String,
    display_name: String,
    description: String,
    skills: String,
    tools: String,
    entrypoint: Option<String>,
    capabilities: String,
    install_path: Option<String>,
    permissions: String,
    enabled: i64,
    installed_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<PluginRow> for PluginManifest {
    type Error = AppError;

    fn try_from(row: PluginRow) -> Result<Self, Self::Error> {
        let skills: Vec<String> =
            serde_json::from_str(&row.skills).map_err(|e| AppError::Internal {
                message: format!("deserialize skills failed: {e}"),
            })?;
        let tools: Vec<String> =
            serde_json::from_str(&row.tools).map_err(|e| AppError::Internal {
                message: format!("deserialize tools failed: {e}"),
            })?;
        let permissions: PluginPermissions =
            serde_json::from_str(&row.permissions).map_err(|e| AppError::Internal {
                message: format!("deserialize permissions failed: {e}"),
            })?;
        let capabilities =
            serde_json::from_str(&row.capabilities).map_err(|e| AppError::Internal {
                message: format!("deserialize capabilities failed: {e}"),
            })?;

        Ok(Self {
            id: PluginId(row.id),
            name: row.name,
            version: row.version,
            display_name: row.display_name,
            description: row.description,
            skills,
            tools,
            entrypoint: row.entrypoint,
            capabilities,
            install_path: row.install_path,
            permissions,
            enabled: row.enabled != 0,
            installed_at: row.installed_at,
        })
    }
}

#[derive(FromRow)]
struct SkillRow {
    id: uuid::Uuid,
    plugin_id: uuid::Uuid,
    name: String,
    description: String,
    trigger_description: String,
    instruction_file: Option<String>,
    required_tools: String,
}

impl TryFrom<SkillRow> for Skill {
    type Error = AppError;

    fn try_from(row: SkillRow) -> Result<Self, Self::Error> {
        let required_tools: Vec<String> =
            serde_json::from_str(&row.required_tools).map_err(|e| AppError::Internal {
                message: format!("deserialize required_tools failed: {e}"),
            })?;

        Ok(Self {
            id: SkillId(row.id),
            plugin_id: PluginId(row.plugin_id),
            name: row.name,
            description: row.description,
            trigger_description: row.trigger_description,
            instruction_file: row.instruction_file,
            required_tools,
        })
    }
}

#[derive(FromRow)]
struct McpServerRow {
    id: i64,
    name: String,
    transport: String,
    command: Option<String>,
    args: String,
    url: Option<String>,
    env: String,
    enabled: i64,
}

impl TryFrom<McpServerRow> for McpServerConfig {
    type Error = AppError;

    fn try_from(row: McpServerRow) -> Result<Self, Self::Error> {
        let args: Vec<String> =
            serde_json::from_str(&row.args).map_err(|e| AppError::Internal {
                message: format!("deserialize args failed: {e}"),
            })?;
        let env: std::collections::HashMap<String, String> = serde_json::from_str(&row.env)
            .map_err(|e| AppError::Internal {
                message: format!("deserialize env failed: {e}"),
            })?;

        Ok(Self {
            id: row.id,
            name: row.name,
            transport: row.transport.as_str().try_into()?,
            command: row.command,
            args,
            url: row.url,
            env,
            enabled: row.enabled != 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_models::{McpTransport, PluginCapability};
    use chrono::Utc;

    async fn setup() -> SqlitePluginRegistry {
        SqlitePluginRegistry::open_in_memory().await.expect("open registry")
    }

    fn sample_manifest() -> PluginManifest {
        PluginManifest {
            id: PluginId::new(),
            name: "test-plugin".to_owned(),
            version: "1.0.0".to_owned(),
            display_name: "Test Plugin".to_owned(),
            description: "A test plugin".to_owned(),
            skills: vec!["summarize".to_owned()],
            tools: vec!["read_file".to_owned()],
            entrypoint: Some("dist/index.html".to_owned()),
            capabilities: vec![
                PluginCapability::MarkdownPreview,
                PluginCapability::MarkdownExportHtml,
            ],
            install_path: Some("/tmp/portico/plugins/test-plugin".to_owned()),
            permissions: PluginPermissions {
                network: vec!["*.example.com".to_owned()],
                filesystem: "read".to_owned(),
            },
            enabled: true,
            installed_at: Utc::now(),
        }
    }

    fn sample_skill(plugin_id: PluginId) -> Skill {
        Skill {
            id: SkillId::new(),
            plugin_id,
            name: "summarize".to_owned(),
            description: "Summarize text".to_owned(),
            trigger_description: "summarize this".to_owned(),
            instruction_file: Some("instructions.md".to_owned()),
            required_tools: vec!["read_file".to_owned()],
        }
    }

    #[tokio::test]
    async fn plugin_crud_works() {
        let registry = setup().await;
        let manifest = sample_manifest();

        let installed = registry.install_plugin(manifest.clone()).await.expect("install plugin");
        assert_eq!(installed.id, manifest.id);

        let plugins = registry.list_plugins().await.expect("list plugins");
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "test-plugin");
        assert!(plugins[0].enabled);
        assert_eq!(plugins[0].entrypoint.as_deref(), Some("dist/index.html"));
        assert_eq!(plugins[0].capabilities, manifest.capabilities);
        assert_eq!(plugins[0].install_path, manifest.install_path);

        registry.enable_plugin(manifest.id, false).await.expect("disable plugin");
        let after_disable = registry.list_plugins().await.expect("list after disable");
        assert!(!after_disable[0].enabled);

        registry.uninstall_plugin(manifest.id).await.expect("uninstall plugin");
        let remaining = registry.list_plugins().await.expect("list after uninstall");
        assert!(remaining.is_empty());
    }

    #[tokio::test]
    async fn skill_registration_works() {
        let registry = setup().await;
        let manifest = sample_manifest();
        registry.install_plugin(manifest.clone()).await.expect("install");

        let skill = sample_skill(manifest.id);
        registry.register_skill(skill.clone()).await.expect("register skill");

        let skills = registry.list_skills(None).await.expect("list skills");
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "summarize");

        let filtered = registry.list_skills(Some(manifest.id)).await.expect("list filtered skills");
        assert_eq!(filtered.len(), 1);

        registry.uninstall_plugin(manifest.id).await.expect("uninstall");
        let after_uninstall = registry.list_skills(None).await.expect("list after uninstall");
        assert!(after_uninstall.is_empty());
    }

    #[tokio::test]
    async fn mcp_server_crud_works() {
        let registry = setup().await;

        let config = McpServerConfig {
            id: 0,
            name: "filesystem".to_owned(),
            transport: McpTransport::Stdio,
            command: Some("npx".to_owned()),
            args: vec![
                "-y".to_owned(),
                "@modelcontextprotocol/server-filesystem".to_owned(),
            ],
            url: None,
            env: std::collections::HashMap::new(),
            enabled: true,
        };

        let added = registry.add_mcp_server(config).await.expect("add mcp server");
        assert!(added.id > 0);

        let servers = registry.list_mcp_servers().await.expect("list mcp servers");
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "filesystem");

        registry.remove_mcp_server(added.id).await.expect("remove mcp server");
        let remaining = registry.list_mcp_servers().await.expect("list after remove");
        assert!(remaining.is_empty());
    }
}
