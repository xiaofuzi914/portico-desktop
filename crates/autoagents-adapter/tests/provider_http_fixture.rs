use app_models::{ModelCapability, ProviderHealthStatus, ProviderKind};
use app_runtime::{ModelProviderRegistry, SqliteModelProviderRegistry, SqliteStorage, Storage};
use app_security::{InMemorySecretStore, SecretStore};
use autoagents_adapter::{build_llm_provider, check_provider_health};
use autoagents_llm::chat::ChatMessage;
use serde_json::Value;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

#[derive(Clone, Copy)]
enum FixtureMode {
    Success,
    Status(u16),
    Delayed,
}

struct HttpFixture {
    base_url: String,
    requests: Arc<Mutex<Vec<Value>>>,
    task: tokio::task::JoinHandle<()>,
}

impl HttpFixture {
    async fn start(mode: FixtureMode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind fixture");
        let address = listener.local_addr().expect("fixture address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let request_log = requests.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let request_log = request_log.clone();
                tokio::spawn(async move {
                    let mut bytes = Vec::new();
                    let mut buffer = [0_u8; 4096];
                    let header_end = loop {
                        let Ok(read) = stream.read(&mut buffer).await else {
                            return;
                        };
                        if read == 0 {
                            return;
                        }
                        bytes.extend_from_slice(&buffer[..read]);
                        if let Some(position) =
                            bytes.windows(4).position(|window| window == b"\r\n\r\n")
                        {
                            break position + 4;
                        }
                    };
                    let headers = String::from_utf8_lossy(&bytes[..header_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    while bytes.len() < header_end + content_length {
                        let Ok(read) = stream.read(&mut buffer).await else {
                            return;
                        };
                        if read == 0 {
                            break;
                        }
                        bytes.extend_from_slice(&buffer[..read]);
                    }
                    if let Ok(body) = serde_json::from_slice::<Value>(
                        &bytes[header_end..bytes.len().min(header_end + content_length)],
                    ) {
                        request_log.lock().await.push(body);
                    }
                    if matches!(mode, FixtureMode::Delayed) {
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    }
                    let (status, body) = match mode {
                        FixtureMode::Success | FixtureMode::Delayed => (
                            200,
                            serde_json::json!({
                                "choices": [{"message": {
                                    "role": "assistant",
                                    "content": "fixture response",
                                    "tool_calls": null
                                }}],
                                "usage": {
                                    "prompt_tokens": 10,
                                    "completion_tokens": 2,
                                    "total_tokens": 12
                                }
                            })
                            .to_string(),
                        ),
                        FixtureMode::Status(status) => (
                            status,
                            serde_json::json!({"error": {"message": format!("fixture {status}")}})
                                .to_string(),
                        ),
                    };
                    let reason = match status {
                        200 => "OK",
                        401 => "Unauthorized",
                        429 => "Too Many Requests",
                        _ => "Error",
                    };
                    let response = format!(
                        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
        Self {
            base_url: format!("http://{address}/v1"),
            requests,
            task,
        }
    }
}

impl Drop for HttpFixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

const fn capabilities() -> ModelCapability {
    ModelCapability {
        supports_streaming: true,
        supports_tools: true,
        supports_json_schema: true,
        supports_vision: false,
        supports_pdf: false,
        supports_system_prompt: true,
        supports_embeddings: false,
        max_context_tokens: Some(16_000),
        input_price_per_1k: None,
        output_price_per_1k: None,
    }
}

#[tokio::test]
async fn local_http_provider_records_five_turns_and_request_history() {
    let fixture = HttpFixture::start(FixtureMode::Success).await;
    let secrets = InMemorySecretStore::new();
    secrets.set("fixture-key", "secret").expect("secret");
    let mut config = app_models::ProviderConfig {
        id: app_models::ProviderId::new(),
        kind: ProviderKind::Custom,
        display_name: "Fixture".to_owned(),
        base_url: Some(fixture.base_url.clone()),
        api_key_reference: "fixture-key".to_owned(),
        organization_id: None,
        project_id: None,
        default_headers: std::collections::HashMap::new(),
        timeout_ms: 5_000,
        retry_policy: app_models::RetryPolicy::default(),
        fallback_provider_ids: Vec::new(),
        enabled: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    config.id = app_models::ProviderId::new();
    let provider = build_llm_provider(&config, Some("fixture-model"), &secrets).expect("provider");
    let mut messages = Vec::new();
    for turn in 1..=5 {
        messages.push(ChatMessage::user().content(format!("turn {turn}")).build());
        let response = provider.chat(&messages, None).await.expect("fixture chat");
        assert_eq!(response.text().as_deref(), Some("fixture response"));
    }
    let requests = fixture.requests.lock().await;
    assert_eq!(requests.len(), 5);
    for (index, request) in requests.iter().enumerate() {
        assert_eq!(request["model"], "fixture-model");
        assert_eq!(
            request["messages"].as_array().map(Vec::len),
            Some(index + 1)
        );
    }
}

#[tokio::test]
async fn health_classifies_401_429_and_timeout_without_leaking_secrets() {
    for (mode, expected) in [
        (FixtureMode::Status(401), "INVALID_CREDENTIALS"),
        (FixtureMode::Status(429), "RATE_LIMITED"),
        (FixtureMode::Delayed, "HEALTH_TIMEOUT"),
    ] {
        let fixture = HttpFixture::start(mode).await;
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("db"));
        let registry = Arc::new(SqliteModelProviderRegistry::new(storage.pool().clone()));
        let secrets = Arc::new(InMemorySecretStore::new());
        secrets.set("fixture-key", "canary-secret").expect("secret");
        let mut provider = registry
            .create_provider(
                ProviderKind::Custom,
                "Fixture",
                Some(&fixture.base_url),
                "fixture-key",
            )
            .await
            .expect("provider");
        provider.timeout_ms = 1_000;
        registry.update_provider(provider.clone()).await.expect("timeout");
        let model = registry
            .add_model(provider.id, "fixture-model", "Fixture", capabilities())
            .await
            .expect("model");
        let health = check_provider_health(registry, secrets, provider.id, model.id)
            .await
            .expect("safe health");
        assert_ne!(health.status, ProviderHealthStatus::Ready);
        assert_eq!(health.error_code.as_deref(), Some(expected));
        assert!(!health.message.as_deref().unwrap_or_default().contains("canary-secret"));
    }
}

#[tokio::test]
async fn delayed_http_provider_request_is_cancellable_without_retrying() {
    let fixture = HttpFixture::start(FixtureMode::Delayed).await;
    let secrets = InMemorySecretStore::new();
    secrets.set("fixture-key", "secret").expect("secret");
    let config = app_models::ProviderConfig {
        id: app_models::ProviderId::new(),
        kind: ProviderKind::Custom,
        display_name: "Fixture".to_owned(),
        base_url: Some(fixture.base_url.clone()),
        api_key_reference: "fixture-key".to_owned(),
        organization_id: None,
        project_id: None,
        default_headers: std::collections::HashMap::new(),
        timeout_ms: 5_000,
        retry_policy: app_models::RetryPolicy::default(),
        fallback_provider_ids: Vec::new(),
        enabled: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let provider = build_llm_provider(&config, Some("fixture-model"), &secrets).expect("provider");
    let task = tokio::spawn(async move {
        provider.chat(&[ChatMessage::user().content("cancel me").build()], None).await
    });
    for _ in 0..100 {
        if fixture.requests.lock().await.len() == 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(fixture.requests.lock().await.len(), 1);
    task.abort();
    assert!(task.await.expect_err("request task must be cancelled").is_cancelled());
    assert_eq!(
        fixture.requests.lock().await.len(),
        1,
        "cancel must not retry"
    );
}
