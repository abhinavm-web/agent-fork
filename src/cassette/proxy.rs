use crate::cassette::signature::compute_request_signature;
use crate::cassette::store::CassetteStore;
use crate::cassette::types::{
    CassetteEntry, LlmProvider, RecordedRequest, RecordedResponse, StepCassette,
};
use crate::error::AgentForkError;
use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use bytes::Bytes;
use chrono::Utc;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyMode {
    Record,
    Replay,
    StrictReplay,
}

#[derive(Clone)]
pub struct ProxyInner {
    pub session_id: String,
    pub step_id: u64,
    pub branch: String,
    pub mode: ProxyMode,
    pub upstream_url: String,
    pub target_file: PathBuf,
    pub cassette: Arc<Mutex<StepCassette>>,
    pub client: reqwest::Client,
}

pub struct CassetteProxy {
    port: u16,
    inner: ProxyInner,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl CassetteProxy {
    pub async fn start_recording(
        step_id: u64,
        upstream_url: &str,
        cassette_dir: &Path,
    ) -> Result<Self, AgentForkError> {
        let session_id = cassette_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("default")
            .to_string();

        fs::create_dir_all(cassette_dir)?;
        let target_file = cassette_dir.join(format!("{}.json", step_id));

        let cassette = if target_file.is_file() {
            let data = fs::read_to_string(&target_file)?;
            serde_json::from_str(&data).unwrap_or_else(|_| {
                StepCassette::new(&session_id, step_id, "main")
            })
        } else {
            StepCassette::new(&session_id, step_id, "main")
        };

        let inner = ProxyInner {
            session_id,
            step_id,
            branch: "main".to_string(),
            mode: ProxyMode::Record,
            upstream_url: upstream_url.trim_end_matches('/').to_string(),
            target_file,
            cassette: Arc::new(Mutex::new(cassette)),
            client: reqwest::Client::builder().build().map_err(|e| {
                AgentForkError::Proxy(format!("Failed to build reqwest client: {}", e))
            })?,
        };

        Self::bind_and_serve(inner).await
    }

    pub async fn start_replay(
        step_id: u64,
        cassette_dir: &Path,
    ) -> Result<Self, AgentForkError> {
        let session_id = cassette_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("default")
            .to_string();

        let target_file = cassette_dir.join(format!("{}.json", step_id));
        let cassette = if target_file.is_file() {
            let data = fs::read_to_string(&target_file)?;
            serde_json::from_str(&data)?
        } else {
            // Check if there's any file in subdirectories
            let mut found = None;
            if cassette_dir.is_dir() {
                for entry in fs::read_dir(cassette_dir)? {
                    let entry = entry?;
                    let p = entry.path();
                    if p.is_dir() {
                        let candidate = p.join(format!("{}.json", step_id));
                        if candidate.is_file() {
                            let data = fs::read_to_string(&candidate)?;
                            if let Ok(c) = serde_json::from_str(&data) {
                                found = Some(c);
                                break;
                            }
                        }
                    }
                }
            }
            found.ok_or_else(|| {
                AgentForkError::CassetteNotFound(format!("Step {}", step_id))
            })?
        };

        let inner = ProxyInner {
            session_id,
            step_id,
            branch: "main".to_string(),
            mode: ProxyMode::Replay,
            upstream_url: String::new(),
            target_file,
            cassette: Arc::new(Mutex::new(cassette)),
            client: reqwest::Client::builder().build().map_err(|e| {
                AgentForkError::Proxy(format!("Failed to build reqwest client: {}", e))
            })?,
        };

        Self::bind_and_serve(inner).await
    }

    pub async fn start_with_config(
        session_id: &str,
        step_id: u64,
        branch: &str,
        mode: ProxyMode,
        port: u16,
        upstream_url: &str,
        cassette_store: &CassetteStore,
    ) -> Result<Self, AgentForkError> {
        let target_file = cassette_store.step_file(session_id, step_id);
        if let Some(parent) = target_file.parent() {
            fs::create_dir_all(parent)?;
        }

        let cassette = if target_file.is_file() {
            let data = fs::read_to_string(&target_file)?;
            serde_json::from_str(&data).unwrap_or_else(|_| {
                StepCassette::new(session_id, step_id, branch)
            })
        } else {
            StepCassette::new(session_id, step_id, branch)
        };

        let inner = ProxyInner {
            session_id: session_id.to_string(),
            step_id,
            branch: branch.to_string(),
            mode,
            upstream_url: upstream_url.trim_end_matches('/').to_string(),
            target_file,
            cassette: Arc::new(Mutex::new(cassette)),
            client: reqwest::Client::builder().build().map_err(|e| {
                AgentForkError::Proxy(format!("Failed to build reqwest client: {}", e))
            })?,
        };

        Self::bind_and_serve_port(inner, port).await
    }

    async fn bind_and_serve(inner: ProxyInner) -> Result<Self, AgentForkError> {
        Self::bind_and_serve_port(inner, 0).await
    }

    async fn bind_and_serve_port(inner: ProxyInner, port: u16) -> Result<Self, AgentForkError> {
        let bind_addr = format!("127.0.0.1:{}", port);
        let listener = tokio::net::TcpListener::bind(&bind_addr).await.map_err(|e| {
            AgentForkError::Proxy(format!("Failed to bind to {}: {}", bind_addr, e))
        })?;
        let actual_port = listener.local_addr()?.port();

        let app = Router::new()
            .route("/v1/messages", post(handle_proxy_request))
            .route("/v1/chat/completions", post(handle_proxy_request))
            .route("/chat/completions", post(handle_proxy_request))
            .route("/health", get(handle_health))
            .fallback(handle_fallback)
            .with_state(inner.clone());

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await;
        });

        Ok(Self {
            port: actual_port,
            inner,
            shutdown_tx: Some(shutdown_tx),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        // Flush recorded cassette to disk if in Record mode
        if self.inner.mode == ProxyMode::Record {
            let guard = self.inner.cassette.lock().await;
            if let Some(parent) = self.inner.target_file.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(json_str) = serde_json::to_string_pretty(&*guard) {
                let _ = fs::write(&self.inner.target_file, json_str);
            }
        }
    }
}

async fn handle_health() -> impl IntoResponse {
    (StatusCode::OK, "{\"status\":\"ok\"}\n")
}

async fn handle_proxy_request(
    State(inner): State<ProxyInner>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    process_request(inner, method, uri, headers, body).await
}

async fn handle_fallback(
    State(inner): State<ProxyInner>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    process_request(inner, method, uri, headers, body).await
}

async fn process_request(
    inner: ProxyInner,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let path = uri.path();
    let body_json: Option<serde_json::Value> = serde_json::from_slice(&body).ok();
    let signature = compute_request_signature(
        method.as_str(),
        path,
        body_json.as_ref(),
        if body_json.is_none() { Some(&body) } else { None },
    );

    match inner.mode {
        ProxyMode::Replay | ProxyMode::StrictReplay => {
            let guard = inner.cassette.lock().await;
            // Look for matching entry by signature
            if let Some(entry) = guard.entries.iter().find(|e| e.signature == signature) {
                let mut resp_builder = Response::builder().status(entry.response.status);
                for (k, v) in &entry.response.headers {
                    resp_builder = resp_builder.header(k, v);
                }
                return resp_builder
                    .body(Body::from(entry.response.body_raw.clone()))
                    .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "response builder error").into_response());
            }

            // Fallback: If no exact signature match, check if any entry has matching endpoint
            if inner.mode == ProxyMode::Replay {
                if let Some(entry) = guard.entries.iter().find(|e| e.endpoint == path) {
                    let mut resp_builder = Response::builder().status(entry.response.status);
                    for (k, v) in &entry.response.headers {
                        resp_builder = resp_builder.header(k, v);
                    }
                    return resp_builder
                        .body(Body::from(entry.response.body_raw.clone()))
                        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "response builder error").into_response());
                }
            }

            (
                StatusCode::NOT_FOUND,
                format!("{{\"error\":\"No recorded cassette matching signature '{}'\"}}", signature),
            )
                .into_response()
        }
        ProxyMode::Record => {
            let target_url = format!("{}{}", inner.upstream_url, uri.path_and_query().map(|pq| pq.as_str()).unwrap_or(path));

            let mut req_builder = inner.client.request(
                reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::POST),
                &target_url,
            );

            for (k, v) in &headers {
                if k != axum::http::header::HOST {
                    if let Ok(v_str) = v.to_str() {
                        req_builder = req_builder.header(k.as_str(), v_str);
                    }
                }
            }

            let start_time = Utc::now();
            let upstream_resp = match req_builder.body(body.clone()).send().await {
                Ok(resp) => resp,
                Err(err) => {
                    return (
                        StatusCode::BAD_GATEWAY,
                        format!("{{\"error\":\"Upstream connection failed: {}\"}}", err),
                    )
                        .into_response();
                }
            };

            let duration_ms = (Utc::now() - start_time).num_milliseconds().max(0) as u64;
            let status = upstream_resp.status().as_u16();
            let mut resp_headers = HashMap::new();
            for (k, v) in upstream_resp.headers().iter() {
                if let Ok(v_str) = v.to_str() {
                    resp_headers.insert(k.as_str().to_string(), v_str.to_string());
                }
            }

            let resp_bytes = match upstream_resp.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    return (
                        StatusCode::BAD_GATEWAY,
                        format!("{{\"error\":\"Failed reading upstream response: {}\"}}", e),
                    )
                        .into_response();
                }
            };

            let resp_str = String::from_utf8_lossy(&resp_bytes).to_string();

            // Record entry
            let provider = if path.contains("/messages") {
                LlmProvider::Anthropic
            } else if path.contains("/chat/completions") {
                LlmProvider::OpenAi
            } else if path.contains("/sse") || path.contains("/message") {
                LlmProvider::Mcp
            } else {
                LlmProvider::Generic
            };

            let sanitized_req_headers = sanitize_headers(&headers);

            let entry = CassetteEntry {
                id: format!("turn_{}", uuid::Uuid::new_v4()),
                index: 0,
                timestamp: Utc::now(),
                provider,
                endpoint: path.to_string(),
                signature,
                request: RecordedRequest {
                    method: method.to_string(),
                    path: path.to_string(),
                    headers: sanitized_req_headers,
                    body_json,
                    body_raw: Some(String::from_utf8_lossy(&body).to_string()),
                },
                response: RecordedResponse {
                    status,
                    headers: resp_headers.clone(),
                    is_streaming: resp_headers
                        .get("content-type")
                        .is_some_and(|ct| ct.contains("event-stream")),
                    stream_events: None,
                    body_raw: resp_str,
                    parsed_message: None,
                },
                duration_ms,
                is_edited: false,
                edit_history: None,
            };

            {
                let mut guard = inner.cassette.lock().await;
                guard.entries.push(entry);
                if let Some(parent) = inner.target_file.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Ok(json_str) = serde_json::to_string_pretty(&*guard) {
                    let _ = fs::write(&inner.target_file, json_str);
                }
            }

            let mut builder = Response::builder().status(status);
            for (k, v) in resp_headers {
                builder = builder.header(k, v);
            }
            builder
                .body(Body::from(resp_bytes))
                .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "response builder error").into_response())
        }
    }
}

fn sanitize_headers(headers: &HeaderMap) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for (k, v) in headers.iter() {
        let key = k.as_str().to_lowercase();
        let val = if key == "authorization" || key == "x-api-key" || key == "api-key" {
            "[REDACTED]".to_string()
        } else {
            v.to_str().unwrap_or("").to_string()
        };
        out.insert(key, val);
    }
    out
}
