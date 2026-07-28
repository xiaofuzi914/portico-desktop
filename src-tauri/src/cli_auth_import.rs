//! Import session credentials from local CLI agent installs
//! (Codex / Kimi Code / Grok Build) — same model as those tools' login state,
//! not platform API-key paste.
//!
//! Sources (read-only):
//! - Codex: `~/.codex/auth.json` (or `$CODEX_HOME/auth.json`)
//! - Grok Build: `~/.grok/auth.json`
//! - Kimi Code: `~/.kimi-code/credentials/*.json` and `~/.kimi/config.toml` hints

use app_models::{AppError, ProviderKind};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// One detected CLI login that can be imported into Portico.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliAuthSource {
    /// Stable id for UI selection (`codex`, `grok`, `kimi-code:<name>`).
    pub id: String,
    pub kind: ProviderKind,
    /// Human label, e.g. "Codex CLI (ChatGPT session)".
    pub label: String,
    /// Absolute path of the credential file (for transparency).
    pub path: String,
    /// Auth flavour for UI: `chatgpt_oauth` | `access_token` | `api_key` | `unknown`.
    pub auth_mode: String,
    /// Whether a usable secret was found (not expired-empty).
    pub available: bool,
    /// Masked preview of the credential (never the full secret).
    pub preview: String,
    /// Hint if login is missing / unreadable.
    pub hint: Option<String>,
}

/// Result of importing a CLI source into the Portico secret store shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliAuthImport {
    pub kind: ProviderKind,
    pub display_name: String,
    pub base_url: Option<String>,
    /// Opaque token/key to store as the provider secret (Bearer-ready when possible).
    pub secret: String,
    pub auth_mode: String,
    /// Suggested secret-store reference name.
    pub key_reference: String,
}

fn home_dir() -> Result<PathBuf, AppError> {
    dirs::home_dir().ok_or_else(|| AppError::Internal {
        message: "cannot resolve home directory for CLI auth import".to_owned(),
    })
}

fn mask_secret(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return "(empty)".to_owned();
    }
    let chars: Vec<char> = t.chars().collect();
    if chars.len() <= 8 {
        return format!("{}…", chars.first().copied().unwrap_or('*'));
    }
    let head: String = chars.iter().take(4).collect();
    let tail: String = chars.iter().rev().take(4).rev().collect();
    format!("{head}…{tail}")
}

fn read_json_file(path: &Path) -> Result<Value, AppError> {
    let text = fs::read_to_string(path).map_err(|e| AppError::Internal {
        message: format!("read {} failed: {e}", path.display()),
    })?;
    serde_json::from_str(&text).map_err(|e| AppError::Internal {
        message: format!("parse {} failed: {e}", path.display()),
    })
}

fn first_string(v: &Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(s) = v.get(*k).and_then(|x| x.as_str()) {
            let t = s.trim();
            if !t.is_empty() {
                return Some(t.to_owned());
            }
        }
    }
    None
}

/// Nested token object used by Codex / Grok-style auth files.
fn tokens_object(v: &Value) -> Option<&Value> {
    v.get("tokens").or_else(|| v.get("token"))
}

fn extract_bearer_secret(v: &Value) -> Option<(String, String)> {
    // Prefer session access tokens over static API keys when both exist.
    if let Some(tokens) = tokens_object(v) {
        if let Some(access) = first_string(
            tokens,
            &["access_token", "accessToken", "token", "session_token"],
        ) {
            return Some((access, "access_token".to_owned()));
        }
        if let Some(id_token) = first_string(tokens, &["id_token", "idToken"]) {
            return Some((id_token, "id_token".to_owned()));
        }
    }
    if let Some(access) = first_string(
        v,
        &[
            "access_token",
            "accessToken",
            "session_token",
            "token",
            "auth_token",
        ],
    ) {
        return Some((access, "access_token".to_owned()));
    }
    if let Some(key) = first_string(
        v,
        &[
            "OPENAI_API_KEY",
            "api_key",
            "apiKey",
            "XAI_API_KEY",
            "MOONSHOT_API_KEY",
            "personal_access_token",
        ],
    ) {
        return Some((key, "api_key".to_owned()));
    }
    None
}

/// Extract a Grok CLI credential from both legacy token-shaped files and the
/// current scoped credential store written by Grok Build.
///
/// Current Grok versions store each login under a dynamic issuer/account key,
/// for example `https://auth.x.ai::<account-id>`, with the bearer token in the
/// nested `key` field. Keep this parser Grok-specific so a generic JSON file
/// cannot accidentally turn an unrelated nested `key` value into a credential.
fn extract_grok_bearer_secret(v: &Value) -> Option<(String, String)> {
    if let Some(secret) = extract_bearer_secret(v) {
        return Some(secret);
    }

    let entries = v.as_object()?;
    entries
        .iter()
        .filter(|(scope, _)| {
            scope.starts_with("https://auth.x.ai::")
                || scope.as_str() == "https://accounts.x.ai/sign-in"
        })
        .find_map(|(_, credential)| grok_scoped_secret(credential, "access_token"))
        .or_else(|| {
            entries
                .get("xai::api_key")
                .and_then(|credential| grok_scoped_secret(credential, "api_key"))
        })
}

fn grok_scoped_secret(credential: &Value, default_mode: &str) -> Option<(String, String)> {
    if let Some(raw_expiry) = credential.get("expires_at").and_then(Value::as_str) {
        let expiry = DateTime::parse_from_rfc3339(raw_expiry).ok()?;
        if expiry <= Utc::now() {
            return None;
        }
    }
    let secret = first_string(credential, &["key"])?;
    let mode = first_string(credential, &["auth_mode"]).unwrap_or_else(|| default_mode.to_owned());
    Some((secret, mode))
}

fn codex_home() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().map(|h| h.join(".codex")).unwrap_or_default())
}

fn scan_codex() -> CliAuthSource {
    let path = codex_home().join("auth.json");
    let id = "codex".to_owned();
    if !path.is_file() {
        return CliAuthSource {
            id,
            kind: ProviderKind::OpenAI,
            label: "Codex CLI".to_owned(),
            path: path.display().to_string(),
            auth_mode: "chatgpt_oauth".to_owned(),
            available: false,
            preview: String::new(),
            hint: Some(
                "未找到 ~/.codex/auth.json。请先在终端运行 `codex login`（ChatGPT 登录）。"
                    .to_owned(),
            ),
        };
    }
    match read_json_file(&path) {
        Ok(v) => {
            let mode = v.get("auth_mode").and_then(|x| x.as_str()).unwrap_or("chatgpt").to_owned();
            if let Some((secret, secret_kind)) = extract_bearer_secret(&v) {
                CliAuthSource {
                    id,
                    kind: ProviderKind::OpenAI,
                    label: format!("Codex CLI ({mode})"),
                    path: path.display().to_string(),
                    auth_mode: if mode.contains("chatgpt") {
                        "chatgpt_oauth".to_owned()
                    } else {
                        secret_kind
                    },
                    available: true,
                    preview: mask_secret(&secret),
                    hint: None,
                }
            } else {
                CliAuthSource {
                    id,
                    kind: ProviderKind::OpenAI,
                    label: "Codex CLI".to_owned(),
                    path: path.display().to_string(),
                    auth_mode: mode,
                    available: false,
                    preview: String::new(),
                    hint: Some("auth.json 存在但未解析到 access_token / API key。".to_owned()),
                }
            }
        }
        Err(e) => CliAuthSource {
            id,
            kind: ProviderKind::OpenAI,
            label: "Codex CLI".to_owned(),
            path: path.display().to_string(),
            auth_mode: "chatgpt_oauth".to_owned(),
            available: false,
            preview: String::new(),
            hint: Some(e.to_string()),
        },
    }
}

fn scan_grok() -> CliAuthSource {
    let path = home_dir()
        .map(|h| h.join(".grok").join("auth.json"))
        .unwrap_or_else(|_| PathBuf::from("~/.grok/auth.json"));
    let id = "grok".to_owned();
    if !path.is_file() {
        return CliAuthSource {
            id,
            kind: ProviderKind::Xai,
            label: "Grok CLI / Grok Build".to_owned(),
            path: path.display().to_string(),
            auth_mode: "oauth".to_owned(),
            available: false,
            preview: String::new(),
            hint: Some(
                "未找到 ~/.grok/auth.json。请先运行 `grok login`（浏览器登录 xAI）。".to_owned(),
            ),
        };
    }
    match read_json_file(&path) {
        Ok(v) => grok_source_from_value(&path, &v),
        Err(e) => CliAuthSource {
            id,
            kind: ProviderKind::Xai,
            label: "Grok CLI / Grok Build".to_owned(),
            path: path.display().to_string(),
            auth_mode: "oauth".to_owned(),
            available: false,
            preview: String::new(),
            hint: Some(e.to_string()),
        },
    }
}

fn grok_source_from_value(path: &Path, v: &Value) -> CliAuthSource {
    if let Some((secret, secret_kind)) = extract_grok_bearer_secret(v) {
        CliAuthSource {
            id: "grok".to_owned(),
            kind: ProviderKind::Xai,
            label: "Grok CLI / Grok Build".to_owned(),
            path: path.display().to_string(),
            auth_mode: secret_kind,
            available: true,
            preview: mask_secret(&secret),
            hint: None,
        }
    } else {
        CliAuthSource {
            id: "grok".to_owned(),
            kind: ProviderKind::Xai,
            label: "Grok CLI / Grok Build".to_owned(),
            path: path.display().to_string(),
            auth_mode: "oauth".to_owned(),
            available: false,
            preview: String::new(),
            hint: Some("auth.json 存在但未解析到 token。".to_owned()),
        }
    }
}

fn scan_kimi_dir(dir: &Path, sources: &mut Vec<CliAuthSource>) {
    if !dir.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        // Skip MCP subfolder files handled separately if nested.
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("default").to_owned();
        let id = format!("kimi-code:{name}");
        match read_json_file(&path) {
            Ok(v) => {
                if let Some((secret, secret_kind)) = extract_bearer_secret(&v) {
                    sources.push(CliAuthSource {
                        id,
                        kind: ProviderKind::Moonshot,
                        label: format!("Kimi Code CLI ({name})"),
                        path: path.display().to_string(),
                        auth_mode: secret_kind,
                        available: true,
                        preview: mask_secret(&secret),
                        hint: None,
                    });
                }
            }
            Err(_) => continue,
        }
    }
}

fn scan_kimi() -> Vec<CliAuthSource> {
    let mut sources = Vec::new();
    let home = match home_dir() {
        Ok(h) => h,
        Err(_) => {
            return vec![CliAuthSource {
                id: "kimi-code".to_owned(),
                kind: ProviderKind::Moonshot,
                label: "Kimi Code CLI".to_owned(),
                path: "~/.kimi-code/credentials".to_owned(),
                auth_mode: "oauth".to_owned(),
                available: false,
                preview: String::new(),
                hint: Some("无法解析 home 目录。".to_owned()),
            }];
        }
    };

    // Official Kimi Code data root (and env override).
    let kimi_code_home = std::env::var_os("KIMI_CODE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".kimi-code"));
    scan_kimi_dir(&kimi_code_home.join("credentials"), &mut sources);

    // Legacy / alternate layout under ~/.kimi
    scan_kimi_dir(&home.join(".kimi").join("credentials"), &mut sources);

    // Single-file auth.json variants
    for rel in [".kimi-code/auth.json", ".kimi/auth.json"] {
        let path = home.join(rel);
        if !path.is_file() {
            continue;
        }
        if let Ok(v) = read_json_file(&path) {
            if let Some((secret, secret_kind)) = extract_bearer_secret(&v) {
                sources.push(CliAuthSource {
                    id: format!("kimi-file:{}", rel),
                    kind: ProviderKind::Moonshot,
                    label: format!("Kimi CLI ({rel})"),
                    path: path.display().to_string(),
                    auth_mode: secret_kind,
                    available: true,
                    preview: mask_secret(&secret),
                    hint: None,
                });
            }
        }
    }

    if sources.is_empty() {
        sources.push(CliAuthSource {
            id: "kimi-code".to_owned(),
            kind: ProviderKind::Moonshot,
            label: "Kimi Code CLI".to_owned(),
            path: kimi_code_home.join("credentials").display().to_string(),
            auth_mode: "oauth".to_owned(),
            available: false,
            preview: String::new(),
            hint: Some(
                "未找到 Kimi 登录凭证。请先在终端运行 `kimi` 并执行 `/login`（推荐 Kimi Code OAuth）。"
                    .to_owned(),
            ),
        });
    }
    sources
}

/// Scan the machine for CLI agent login state (no secrets returned).
#[must_use]
pub fn list_cli_auth_sources() -> Vec<CliAuthSource> {
    let mut out = vec![scan_codex(), scan_grok()];
    out.extend(scan_kimi());
    out
}

fn load_secret_from_source(
    source_id: &str,
) -> Result<(ProviderKind, String, String, Option<String>), AppError> {
    let sources = list_cli_auth_sources();
    let meta =
        sources
            .into_iter()
            .find(|s| s.id == source_id)
            .ok_or_else(|| AppError::NotFound {
                resource: format!("cli_auth_source:{source_id}"),
            })?;
    if !meta.available {
        return Err(AppError::Internal {
            message: meta.hint.unwrap_or_else(|| "CLI 登录不可用".to_owned()),
        });
    }
    let path = PathBuf::from(&meta.path);
    if path.is_dir() {
        return Err(AppError::Internal {
            message: "credential path is a directory; pick a specific kimi-code:* source"
                .to_owned(),
        });
    }
    let v = read_json_file(&path)?;
    let extractor = if meta.kind == ProviderKind::Xai {
        extract_grok_bearer_secret
    } else {
        extract_bearer_secret
    };
    let (secret, mode) = extractor(&v).ok_or_else(|| AppError::Internal {
        message: format!("no token found in {}", path.display()),
    })?;

    let base_url = match meta.kind {
        ProviderKind::Moonshot => Some("https://api.moonshot.cn/v1".to_owned()),
        ProviderKind::Xai => Some("https://api.x.ai/v1".to_owned()),
        ProviderKind::OpenAI => {
            // ChatGPT-session auth often still targets OpenAI-compatible gateways;
            // users can override base_url in advanced settings if needed.
            None
        }
        _ => None,
    };

    Ok((meta.kind, secret, mode, base_url))
}

/// Load full secret material for import into Portico's secret store.
pub fn import_cli_auth_source(source_id: &str) -> Result<CliAuthImport, AppError> {
    let (kind, secret, auth_mode, base_url) = load_secret_from_source(source_id)?;
    let display_name = match kind {
        ProviderKind::OpenAI => "OpenAI (Codex session)".to_owned(),
        ProviderKind::Moonshot => "Moonshot / Kimi (CLI session)".to_owned(),
        ProviderKind::Xai => "Grok (CLI session)".to_owned(),
        other => other.as_str().to_owned(),
    };
    let key_reference = format!(
        "cli-{}-{}",
        kind.as_str().to_lowercase(),
        uuid::Uuid::new_v4()
    );
    Ok(CliAuthImport {
        kind,
        display_name,
        base_url,
        secret,
        auth_mode,
        key_reference,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn masks_secrets() {
        assert_eq!(mask_secret("sk-abcdefghijklmnop"), "sk-a…mnop");
    }

    #[test]
    fn extracts_codex_style_tokens() {
        let v = serde_json::json!({
            "auth_mode": "chatgpt",
            "tokens": {
                "access_token": "atk_hello_world_token",
                "refresh_token": "rtk_xxx"
            }
        });
        let (s, mode) = extract_bearer_secret(&v).expect("token");
        assert_eq!(s, "atk_hello_world_token");
        assert_eq!(mode, "access_token");
    }

    #[test]
    fn extracts_api_key_fallback() {
        let v = serde_json::json!({ "OPENAI_API_KEY": "sk-test-key-12345678" });
        let (s, mode) = extract_bearer_secret(&v).expect("key");
        assert!(s.starts_with("sk-"));
        assert_eq!(mode, "api_key");
    }

    #[test]
    fn extracts_grok_oidc_auth_store_key() {
        let v = serde_json::json!({
            "https://auth.x.ai::00000000-0000-0000-0000-000000000000": {
                "key": "eyJ.test-grok-session-token",
                "auth_mode": "oidc",
                "refresh_token": "must-not-be-imported",
                "expires_at": "2099-01-01T00:00:00Z",
                "oidc_issuer": "https://auth.x.ai"
            }
        });

        let (secret, mode) = extract_grok_bearer_secret(&v).expect("grok token");
        assert_eq!(secret, "eyJ.test-grok-session-token");
        assert_eq!(mode, "oidc");
    }

    #[test]
    fn marks_current_grok_auth_store_as_available() {
        let v = serde_json::json!({
            "https://auth.x.ai::00000000-0000-0000-0000-000000000000": {
                "key": "eyJ.test-grok-session-token",
                "auth_mode": "oidc"
            }
        });

        let source = grok_source_from_value(Path::new("/tmp/grok-auth.json"), &v);
        assert!(source.available);
        assert_eq!(source.id, "grok");
        assert_eq!(source.kind, ProviderKind::Xai);
        assert_eq!(source.auth_mode, "oidc");
        assert!(!source.preview.contains("test-grok-session-token"));
    }

    #[test]
    fn extracts_grok_documented_sign_in_scope() {
        let v = serde_json::json!({
            "https://accounts.x.ai/sign-in": {
                "key": "documented-grok-session-token"
            }
        });

        let (secret, mode) = extract_grok_bearer_secret(&v).expect("grok token");
        assert_eq!(secret, "documented-grok-session-token");
        assert_eq!(mode, "access_token");
    }

    #[test]
    fn does_not_import_grok_refresh_token_as_bearer_secret() {
        let v = serde_json::json!({
            "https://auth.x.ai::00000000-0000-0000-0000-000000000000": {
                "key": "",
                "auth_mode": "oidc",
                "refresh_token": "must-not-be-imported"
            }
        });

        assert!(extract_grok_bearer_secret(&v).is_none());
    }

    #[test]
    fn skips_empty_grok_account_and_uses_next_session() {
        let v = serde_json::json!({
            "https://auth.x.ai::00000000-0000-0000-0000-000000000000": {
                "key": "",
                "auth_mode": "oidc"
            },
            "https://auth.x.ai::11111111-1111-1111-1111-111111111111": {
                "key": "usable-grok-session-token",
                "auth_mode": "oidc"
            }
        });

        let (secret, mode) = extract_grok_bearer_secret(&v).expect("grok token");
        assert_eq!(secret, "usable-grok-session-token");
        assert_eq!(mode, "oidc");
    }

    #[test]
    fn skips_expired_grok_session_and_uses_cached_api_key() {
        let v = serde_json::json!({
            "https://auth.x.ai::00000000-0000-0000-0000-000000000000": {
                "key": "expired-session-token",
                "auth_mode": "oidc",
                "expires_at": "2000-01-01T00:00:00Z"
            },
            "xai::api_key": {
                "key": "xai-test-api-key"
            }
        });

        let (secret, mode) = extract_grok_bearer_secret(&v).expect("xAI API key");
        assert_eq!(secret, "xai-test-api-key");
        assert_eq!(mode, "api_key");
    }

    #[test]
    fn reads_token_from_codex_shaped_file() {
        let dir = std::env::temp_dir().join(format!("portico-codex-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let auth = dir.join("auth.json");
        let mut f = fs::File::create(&auth).unwrap();
        write!(
            f,
            r#"{{"auth_mode":"chatgpt","tokens":{{"access_token":"atk_abc_def_ghi_jkl"}}}}"#
        )
        .unwrap();
        let v = read_json_file(&auth).expect("json");
        let (secret, mode) = extract_bearer_secret(&v).expect("secret");
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(secret, "atk_abc_def_ghi_jkl");
        assert_eq!(mode, "access_token");
        assert_eq!(mask_secret(&secret), "atk_…_jkl");
    }
}
