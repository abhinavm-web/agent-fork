use agent_fork::cassette::proxy::CassetteProxy;
use agent_fork::cassette::store::CassetteStore;
use axum::{routing::post, Json, Router};
use serde_json::json;
use tempfile::tempdir;
use tokio::net::TcpListener;

#[tokio::test]
async fn test_cassette_record_and_replay_cycle() {
    let dir = tempdir().unwrap();
    let cassette_dir = dir.path().join(".agent-fork/cassettes/sess-1");

    // 1. Spin up mock upstream LLM server
    let mock_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mock_port = mock_listener.local_addr().unwrap().port();

    let app = Router::new().route(
        "/v1/messages",
        post(|| async {
            Json(json!({
                "id": "msg_mock",
                "content": [{"type": "text", "text": "Hello, Human!"}],
                "stop_reason": "end_turn"
            }))
        }),
    );

    let mock_server = tokio::spawn(async move {
        let _ = axum::serve(mock_listener, app).await;
    });

    // 2. Start Cassette Proxy in Recording Mode
    let upstream_url = format!("http://127.0.0.1:{}", mock_port);
    let proxy = CassetteProxy::start_recording(0, &upstream_url, &cassette_dir).await.unwrap();
    let proxy_port = proxy.port();

    // 3. Send request through proxy
    let client = reqwest::Client::new();
    let req_body = json!({
        "model": "claude-3-7-sonnet",
        "messages": [{"role": "user", "content": "Hello"}]
    });

    let resp = client
        .post(format!("http://127.0.0.1:{}/v1/messages", proxy_port))
        .json(&req_body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let resp_body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(resp_body["content"][0]["text"], "Hello, Human!");

    // Stop recording and shutdown mock upstream
    proxy.shutdown().await;
    mock_server.abort();

    // 4. Start Cassette Proxy in Replay Mode (Upstream is DEAD)
    let replay_proxy = CassetteProxy::start_replay(0, &cassette_dir).await.unwrap();
    let replay_port = replay_proxy.port();

    // 5. Send identical request to replay proxy
    let replay_resp = client
        .post(format!("http://127.0.0.1:{}/v1/messages", replay_port))
        .json(&req_body)
        .send()
        .await
        .unwrap();

    assert_eq!(replay_resp.status(), 200);
    let replay_body: serde_json::Value = replay_resp.json().await.unwrap();
    assert_eq!(replay_body["content"][0]["text"], "Hello, Human!");

    replay_proxy.shutdown().await;
}

#[tokio::test]
async fn test_cassette_turn_editing() {
    let dir = tempdir().unwrap();
    let cassette_dir = dir.path().join("cassettes");
    let store = CassetteStore::new(&cassette_dir);

    let cassette_id = "turn_001";
    store.save_turn(cassette_id, "original prompt", "original tool output").unwrap();

    // In-flight edit prompt and output
    store.edit_turn(cassette_id, Some("edited prompt"), Some("mocked tool output")).unwrap();

    let entry = store.load_turn(cassette_id).unwrap();
    assert_eq!(entry.request_prompt, "edited prompt");
    assert_eq!(entry.response_body, "mocked tool output");
}
