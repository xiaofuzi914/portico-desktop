//! Import session credentials from local CLI agent installs
//! (Codex / Kimi Code / Grok Build) — same model as those tools' login state,
//! not platform API-key paste.
//!
//! Sources (read-only):
//! - Codex: `~/.codex/auth.json` (or `$CODEX_HOME/auth.json`)
//! - Grok Build: `~/.grok/auth.json`
//! - Kimi Code: `~/.kimi-code/credentials/*.json` and `~/.kimi/config.toml` hints
//!
//! Codex ChatGPT OAuth notes:
//! - `auth_mode: "chatgpt"` stores `tokens.access_token` (+ refresh/id) and often
//!   `OPENAI_API_KEY: null`. That access token is a ChatGPT session JWT, not a
//!   platform API key.
//! - Usable API base for ChatGPT-session Codex is
//!   `https://chatgpt.com/backend-api/codex/` (Responses API; stream required).
//! - Platform `api.openai.com` typically rejects the session token (scopes/quota).

use app_models::{AppError, ProviderKind};
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Official ChatGPT Codex backend (OpenAI Responses under this base).
pub const CODEX_CHATGPT_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";

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
    /// Extra headers (e.g. `ChatGPT-Account-ID` for Codex ChatGPT sessions).
    #[serde(default)]
    pub default_headers: HashMap<String, String>,
    /// Suggested model seeds: (model_name, display_name).
    #[serde(default)]
    pub suggested_models: Vec<(String, String)>,
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

/// Decode a JWT payload (no signature verification — local metadata only).
fn jwt_payload(token: &str) -> Option<Value> {
    let mut parts = token.split('.');
    let _header = parts.next()?;
    let payload_b64 = parts.next()?;
    if parts.next().is_none() {
        // need at least header.payload.sig
        return None;
    }
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let decoded = engine
        .decode(payload_b64)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload_b64))
        .ok()?;
    serde_json::from_slice(&decoded).ok()
}

fn jwt_exp_unix(token: &str) -> Option<i64> {
    jwt_payload(token)?
        .get("exp")
        .and_then(Value::as_i64)
        .or_else(|| {
            jwt_payload(token)?
                .get("exp")
                .and_then(Value::as_u64)
                .map(|u| i64::try_from(u).unwrap_or(i64::MAX))
        })
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(0))
        .unwrap_or(0)
}

/// ChatGPT plan / account hints from the access JWT (best-effort).
fn chatgpt_session_meta(access_token: &str) -> (Option<String>, Option<String>) {
    let payload = match jwt_payload(access_token) {
        Some(p) => p,
        None => return (None, None),
    };
    let auth = payload.get("https://api.openai.com/auth");
    let plan = auth
        .and_then(|a| a.get("chatgpt_plan_type"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let account = auth
        .and_then(|a| a.get("chatgpt_account_id"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    (plan, account)
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

/// Codex-specific extraction: prefer platform API key when present, else ChatGPT
/// session access token. Real `auth.json` often has `"OPENAI_API_KEY": null`.
fn extract_codex_secret(v: &Value) -> Option<(String, String)> {
    // Explicit non-null API key takes priority (platform billing path).
    if let Some(key) = first_string(
        v,
        &["OPENAI_API_KEY", "api_key", "apiKey"],
    ) {
        return Some((key, "api_key".to_owned()));
    }
    if let Some(tokens) = tokens_object(v) {
        if let Some(access) = first_string(
            tokens,
            &["access_token", "accessToken", "token", "session_token"],
        ) {
            return Some((access, "access_token".to_owned()));
        }
    }
    extract_bearer_secret(v)
}

fn codex_account_id(v: &Value) -> Option<String> {
    if let Some(tokens) = tokens_object(v) {
        if let Some(id) = first_string(tokens, &["account_id", "accountId"]) {
            return Some(id);
        }
    }
    first_string(v, &["account_id", "accountId"])
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

/// Scan a specific Codex home directory (tests + CODEX_HOME override path).
fn scan_codex_at(codex_home_dir: &Path) -> CliAuthSource {
    let path = codex_home_dir.join("auth.json");
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
        Ok(v) => codex_source_from_value(&path, &v),
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

fn codex_source_from_value(path: &Path, v: &Value) -> CliAuthSource {
    let id = "codex".to_owned();
    let mode_raw = v
        .get("auth_mode")
        .and_then(|x| x.as_str())
        .unwrap_or("chatgpt")
        .to_owned();
    let is_chatgpt = mode_raw.to_ascii_lowercase().contains("chatgpt");

    match extract_codex_secret(v) {
        Some((secret, secret_kind)) => {
            let auth_mode = if is_chatgpt && secret_kind != "api_key" {
                "chatgpt_oauth".to_owned()
            } else {
                secret_kind.clone()
            };

            // JWT expiry for session tokens (API keys have no exp).
            let mut hint: Option<String> = None;
            let mut available = true;
            if secret_kind != "api_key" {
                if let Some(exp) = jwt_exp_unix(&secret) {
                    let now = now_unix();
                    if exp <= now {
                        available = false;
                        hint = Some(
                            "ChatGPT access_token 已过期。请在终端运行 `codex login` 后重新导入。"
                                .to_owned(),
                        );
                    } else {
                        let hours_left = (exp - now) / 3600;
                        let (plan, _) = chatgpt_session_meta(&secret);
                        let plan_s = plan.unwrap_or_else(|| "chatgpt".to_owned());
                        let account = codex_account_id(v).unwrap_or_default();
                        let account_short = if account.len() > 8 {
                            format!("{}…", &account[..8])
                        } else {
                            account
                        };
                        hint = Some(format!(
                            "ChatGPT 会话（{plan_s}）· account {account_short} · 约 {hours_left}h 内有效 · 将导入 Codex 网关"
                        ));
                    }
                } else if is_chatgpt {
                    hint = Some(
                        "已检测到 ChatGPT 会话 token（将导入 Codex 后端网关，非 platform API key）。"
                            .to_owned(),
                    );
                }
            } else {
                hint = Some("已检测到 OpenAI API key。".to_owned());
            }

            CliAuthSource {
                id,
                kind: ProviderKind::OpenAI,
                label: if is_chatgpt && secret_kind != "api_key" {
                    format!("Codex CLI (ChatGPT / {mode_raw})")
                } else {
                    format!("Codex CLI ({mode_raw})")
                },
                path: path.display().to_string(),
                auth_mode,
                available,
                preview: mask_secret(&secret),
                hint,
            }
        }
        None => CliAuthSource {
            id,
            kind: ProviderKind::OpenAI,
            label: "Codex CLI".to_owned(),
            path: path.display().to_string(),
            auth_mode: if is_chatgpt {
                "chatgpt_oauth".to_owned()
            } else {
                mode_raw
            },
            available: false,
            preview: String::new(),
            hint: Some(
                "auth.json 存在但未解析到 access_token / API key（OPENAI_API_KEY 为空且 tokens 缺失）。"
                    .to_owned(),
            ),
        },
    }
}

fn scan_codex() -> CliAuthSource {
    scan_codex_at(&codex_home())
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

fn codex_suggested_models(auth_mode: &str) -> Vec<(String, String)> {
    if auth_mode == "chatgpt_oauth" || auth_mode == "access_token" {
        vec![
            ("gpt-5.4-mini".to_owned(), "GPT-5.4 Mini".to_owned()),
            ("gpt-5.4".to_owned(), "GPT-5.4".to_owned()),
            ("gpt-5.5".to_owned(), "GPT-5.5".to_owned()),
            ("gpt-5.6-sol".to_owned(), "GPT-5.6 Sol".to_owned()),
            ("gpt-5.6-terra".to_owned(), "GPT-5.6 Terra".to_owned()),
        ]
    } else {
        vec![
            ("gpt-4.1".to_owned(), "GPT-4.1".to_owned()),
            ("gpt-4.1-mini".to_owned(), "GPT-4.1 mini".to_owned()),
        ]
    }
}

fn load_secret_from_meta(
    source_id: &str,
    meta: &CliAuthSource,
) -> Result<
    (
        ProviderKind,
        String,
        String,
        Option<String>,
        HashMap<String, String>,
        Vec<(String, String)>,
    ),
    AppError,
> {
    if !meta.available {
        return Err(AppError::Internal {
            message: meta
                .hint
                .clone()
                .unwrap_or_else(|| "CLI 登录不可用".to_owned()),
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

    if meta.kind == ProviderKind::Xai {
        let (secret, mode) = extract_grok_bearer_secret(&v).ok_or_else(|| AppError::Internal {
            message: format!("no token found in {}", path.display()),
        })?;
        let base_url = Some("https://api.x.ai/v1".to_owned());
        return Ok((
            meta.kind,
            secret,
            mode,
            base_url,
            HashMap::new(),
            vec![
                ("grok-3".to_owned(), "Grok 3".to_owned()),
                ("grok-3-mini".to_owned(), "Grok 3 Mini".to_owned()),
            ],
        ));
    }

    if meta.kind == ProviderKind::OpenAI && source_id == "codex" {
        let (secret, secret_kind) = extract_codex_secret(&v).ok_or_else(|| AppError::Internal {
            message: format!("no token found in {}", path.display()),
        })?;
        let mode_raw = v
            .get("auth_mode")
            .and_then(|x| x.as_str())
            .unwrap_or("chatgpt");
        let is_chatgpt =
            mode_raw.to_ascii_lowercase().contains("chatgpt") && secret_kind != "api_key";

        if secret_kind != "api_key" {
            if let Some(exp) = jwt_exp_unix(&secret) {
                if exp <= now_unix() {
                    return Err(AppError::Internal {
                        message:
                            "Codex ChatGPT access_token 已过期。请运行 `codex login` 后重试导入。"
                                .to_owned(),
                    });
                }
            }
        }

        let auth_mode = if is_chatgpt {
            "chatgpt_oauth".to_owned()
        } else {
            secret_kind
        };

        let mut headers = HashMap::new();
        // Prefer account_id from file; fall back to JWT claim.
        let account = codex_account_id(&v).or_else(|| chatgpt_session_meta(&secret).1);
        if let Some(account_id) = account {
            headers.insert("ChatGPT-Account-ID".to_owned(), account_id);
        }

        let base_url = if is_chatgpt {
            // Responses API lives at {base}/responses; AutoAgents appends the path.
            Some(CODEX_CHATGPT_BASE_URL.to_owned())
        } else {
            None
        };

        let models = codex_suggested_models(&auth_mode);
        return Ok((meta.kind, secret, auth_mode, base_url, headers, models));
    }

    let (secret, mode) = extract_bearer_secret(&v).ok_or_else(|| AppError::Internal {
        message: format!("no token found in {}", path.display()),
    })?;

    let (base_url, models) = match meta.kind {
        ProviderKind::Moonshot => (
            Some("https://api.moonshot.cn/v1".to_owned()),
            vec![
                (
                    "kimi-k2-turbo-preview".to_owned(),
                    "Kimi K2 Turbo".to_owned(),
                ),
                ("kimi-k2-0711-preview".to_owned(), "Kimi K2".to_owned()),
            ],
        ),
        ProviderKind::Xai => (
            Some("https://api.x.ai/v1".to_owned()),
            vec![
                ("grok-3".to_owned(), "Grok 3".to_owned()),
                ("grok-3-mini".to_owned(), "Grok 3 Mini".to_owned()),
            ],
        ),
        ProviderKind::OpenAI => (
            None,
            vec![
                ("gpt-4.1".to_owned(), "GPT-4.1".to_owned()),
                ("gpt-4.1-mini".to_owned(), "GPT-4.1 mini".to_owned()),
            ],
        ),
        _ => (None, vec![("default".to_owned(), "Default".to_owned())]),
    };

    Ok((meta.kind, secret, mode, base_url, HashMap::new(), models))
}

fn load_secret_from_source(
    source_id: &str,
) -> Result<
    (
        ProviderKind,
        String,
        String,
        Option<String>,
        HashMap<String, String>,
        Vec<(String, String)>,
    ),
    AppError,
> {
    let sources = list_cli_auth_sources();
    let meta =
        sources
            .into_iter()
            .find(|s| s.id == source_id)
            .ok_or_else(|| AppError::NotFound {
                resource: format!("cli_auth_source:{source_id}"),
            })?;
    load_secret_from_meta(source_id, &meta)
}

/// Load full secret material for import into Portico's secret store.
pub fn import_cli_auth_source(source_id: &str) -> Result<CliAuthImport, AppError> {
    let (kind, secret, auth_mode, base_url, default_headers, suggested_models) =
        load_secret_from_source(source_id)?;
    let display_name = match (kind, auth_mode.as_str()) {
        (ProviderKind::OpenAI, "chatgpt_oauth") => "OpenAI (Codex ChatGPT session)".to_owned(),
        (ProviderKind::OpenAI, _) => "OpenAI (Codex session)".to_owned(),
        (ProviderKind::Moonshot, _) => "Moonshot / Kimi (CLI session)".to_owned(),
        (ProviderKind::Xai, _) => "Grok (CLI session)".to_owned(),
        (other, _) => other.as_str().to_owned(),
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
        default_headers,
        suggested_models,
    })
}

/// Whether a provider base URL points at the ChatGPT Codex backend.
#[must_use]
pub fn is_codex_chatgpt_base_url(base_url: Option<&str>) -> bool {
    base_url
        .map(|u| {
            let lower = u.trim().trim_end_matches('/').to_ascii_lowercase();
            lower.contains("chatgpt.com/backend-api/codex")
                || lower.ends_with("backend-api/codex")
        })
        .unwrap_or(false)
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
        let (s, mode) = extract_codex_secret(&v).expect("token");
        assert_eq!(s, "atk_hello_world_token");
        assert_eq!(mode, "access_token");
    }

    #[test]
    fn extracts_codex_with_null_openai_api_key() {
        // Real ~/.codex/auth.json shape: OPENAI_API_KEY is JSON null, not absent.
        let v = serde_json::json!({
            "auth_mode": "chatgpt",
            "OPENAI_API_KEY": null,
            "tokens": {
                "id_token": "eyJ.id",
                "access_token": "eyJ.access_token_value_here",
                "refresh_token": "rt.1.refresh",
                "account_id": "1e2231d2-5d7d-44ec-b245-ea325b2afa01"
            },
            "last_refresh": "2026-07-28T18:29:43.983746Z"
        });
        let (s, mode) = extract_codex_secret(&v).expect("token despite null API key");
        assert!(s.contains("access_token_value"));
        assert_eq!(mode, "access_token");
        assert_eq!(
            codex_account_id(&v).as_deref(),
            Some("1e2231d2-5d7d-44ec-b245-ea325b2afa01")
        );

        let source = codex_source_from_value(Path::new("/tmp/auth.json"), &v);
        assert!(source.available, "null OPENAI_API_KEY must not block scan");
        assert_eq!(source.auth_mode, "chatgpt_oauth");
        assert!(!source.preview.is_empty());
        assert!(source.preview.contains('…'));
    }

    #[test]
    fn prefers_openai_api_key_when_present() {
        let v = serde_json::json!({
            "auth_mode": "chatgpt",
            "OPENAI_API_KEY": "sk-platform-key-12345678",
            "tokens": {
                "access_token": "session-token-should-not-win"
            }
        });
        let (s, mode) = extract_codex_secret(&v).expect("api key");
        assert_eq!(s, "sk-platform-key-12345678");
        assert_eq!(mode, "api_key");
    }

    #[test]
    fn extracts_api_key_fallback() {
        let v = serde_json::json!({ "OPENAI_API_KEY": "sk-test-key-12345678" });
        let (s, mode) = extract_codex_secret(&v).expect("key");
        assert!(s.starts_with("sk-"));
        assert_eq!(mode, "api_key");
    }

    #[test]
    fn import_codex_chatgpt_uses_codex_backend_base_url() {
        let dir = std::env::temp_dir().join(format!("portico-codex-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let auth = dir.join("auth.json");
        let mut f = fs::File::create(&auth).unwrap();
        // Minimal JWT-shaped access token (header.payload.sig) with future exp.
        // payload: {"exp": 4102444800} → year 2100
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"exp":4102444800,"https://api.openai.com/auth":{"chatgpt_plan_type":"pro","chatgpt_account_id":"acct-test-1"}}"#);
        let token = format!("eyJhbGciOiJub25lIn0.{payload}.sig");
        write!(
            f,
            r#"{{"auth_mode":"chatgpt","OPENAI_API_KEY":null,"tokens":{{"access_token":"{token}","refresh_token":"rt.x","account_id":"acct-test-1"}}}}"#
        )
        .unwrap();

        let codex = scan_codex_at(&dir);
        assert!(codex.available, "scan should find token: {:?}", codex.hint);
        assert_eq!(codex.auth_mode, "chatgpt_oauth");
        assert!(codex.preview.contains('…'));

        let (kind, secret, auth_mode, base_url, headers, models) =
            load_secret_from_meta("codex", &codex).expect("import load");
        assert_eq!(kind, ProviderKind::OpenAI);
        assert_eq!(auth_mode, "chatgpt_oauth");
        assert_eq!(base_url.as_deref(), Some(CODEX_CHATGPT_BASE_URL));
        assert_eq!(
            headers.get("ChatGPT-Account-ID").map(String::as_str),
            Some("acct-test-1")
        );
        assert!(!secret.is_empty());
        assert!(
            models.iter().any(|(n, _)| n == "gpt-5.4-mini"),
            "should seed Codex models"
        );
        assert!(is_codex_chatgpt_base_url(base_url.as_deref()));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn marks_expired_codex_token_unavailable() {
        let dir = std::env::temp_dir().join(format!("portico-codex-exp-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let auth = dir.join("auth.json");
        let mut f = fs::File::create(&auth).unwrap();
        // exp in the past
        let payload =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"exp":1000000000}"#);
        let token = format!("eyJhbGciOiJub25lIn0.{payload}.sig");
        write!(
            f,
            r#"{{"auth_mode":"chatgpt","OPENAI_API_KEY":null,"tokens":{{"access_token":"{token}"}}}}"#
        )
        .unwrap();

        let codex = scan_codex_at(&dir);
        assert!(!codex.available);
        assert!(
            codex.hint.as_deref().unwrap_or("").contains("过期")
                || codex
                    .hint
                    .as_deref()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .contains("expir"),
            "hint={:?}",
            codex.hint
        );
        let _ = fs::remove_dir_all(&dir);
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
        let (secret, mode) = extract_codex_secret(&v).expect("secret");
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(secret, "atk_abc_def_ghi_jkl");
        assert_eq!(mode, "access_token");
        assert_eq!(mask_secret(&secret), "atk_…_jkl");
    }
}
