use super::*;
use hive_protocol::{Capability, TaskKind};

fn state() -> HiveState {
    HiveState {
        nodes: Arc::new(RwLock::new(HashMap::new())),
        http_client: reqwest::Client::new(),
    }
}

async fn serve(router: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (url, task)
}

async fn mock_node(id: &str) -> (String, tokio::task::JoinHandle<()>) {
    let router = Router::new()
        .route(
            "/execute",
            post(
                |State(id): State<String>, Json(_): Json<TaskRequest>| async move {
                    Json(TaskResult {
                        node_id: id.clone(),
                        output: format!("Reply from {id}"),
                    })
                },
            ),
        )
        .with_state(id.to_string());
    serve(router).await
}

async fn register(
    state: &HiveState,
    id: &str,
    api_url: String,
    capabilities: Vec<Capability>,
    offline: bool,
) {
    state.nodes.write().await.insert(
        id.into(),
        NodeState {
            info: NodeInfo {
                id: id.into(),
                hostname: id.into(),
                cpu: "Test CPU".into(),
                cores: 4,
                memory_mb: 8192,
                architecture: "x86_64".into(),
                capabilities,
                api_url,
            },
            last_heartbeat: Instant::now()
                - if offline {
                    NODE_TIMEOUT + Duration::from_secs(1)
                } else {
                    Duration::ZERO
                },
            cpu_usage: 12.0,
            memory_used_mb: 2048,
        },
    );
}

fn inference() -> TaskRequest {
    TaskRequest {
        capability: Capability::Inference,
        task: TaskKind::GenerateText {
            prompt: "Explain networking briefly.".into(),
        },
    }
}

#[tokio::test]
async fn dashboard_is_served_from_the_controller() {
    let (url, server) = serve(app(state())).await;
    let response = reqwest::get(&url).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    let html = response.text().await.unwrap();
    assert!(html.contains("FieldHive"));
    assert!(html.contains("id=\"hp-form\""));
    assert!(html.contains("id=\"hp-nodeList\""));
    assert!(!html.contains("<script src="));
    server.abort();
}

#[tokio::test]
async fn targeted_tasks_reach_the_selected_node_even_with_matching_capabilities() {
    let state = state();
    let (alpha_url, alpha) = mock_node("alpha").await;
    let (beta_url, beta) = mock_node("beta").await;
    register(
        &state,
        "alpha",
        alpha_url,
        vec![Capability::Inference],
        false,
    )
    .await;
    register(&state, "beta", beta_url, vec![Capability::Inference], false).await;
    let (url, server) = serve(app(state)).await;
    let client = reqwest::Client::new();
    for id in ["alpha", "beta"] {
        let response = client
            .post(format!("{url}/nodes/{id}/execute"))
            .json(&inference())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let result = response.json::<TaskResult>().await.unwrap();
        assert_eq!(result.node_id, id);
        assert_eq!(result.output, format!("Reply from {id}"));
    }
    alpha.abort();
    beta.abort();
    server.abort();
}

#[tokio::test]
async fn target_errors_distinguish_unknown_offline_and_unsupported_nodes() {
    let state = state();
    register(
        &state,
        "offline",
        "http://127.0.0.1:1".into(),
        vec![Capability::Inference],
        true,
    )
    .await;
    register(
        &state,
        "tools",
        "http://127.0.0.1:1".into(),
        vec![Capability::Tools],
        false,
    )
    .await;
    let (url, server) = serve(app(state)).await;
    let client = reqwest::Client::new();
    for (id, status) in [
        ("missing", StatusCode::NOT_FOUND),
        ("offline", StatusCode::SERVICE_UNAVAILABLE),
        ("tools", StatusCode::FORBIDDEN),
    ] {
        let response = client
            .post(format!("{url}/nodes/{id}/execute"))
            .json(&inference())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), status, "Unexpected response for {id}");
    }
    server.abort();
}

#[tokio::test]
async fn automatic_routing_remains_available() {
    let state = state();
    let (node_url, node) = mock_node("thinkcentre-llm").await;
    register(
        &state,
        "thinkcentre-llm",
        node_url,
        vec![Capability::Inference],
        false,
    )
    .await;
    let (url, server) = serve(app(state)).await;
    let result = reqwest::Client::new()
        .post(format!("{url}/tasks/execute"))
        .json(&inference())
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json::<TaskResult>()
        .await
        .unwrap();
    assert_eq!(result.node_id, "thinkcentre-llm");
    node.abort();
    server.abort();
}

#[tokio::test]
async fn response_from_an_unexpected_agent_is_rejected() {
    let state = state();
    let (node_url, node) = mock_node("wrong-agent").await;
    register(
        &state,
        "expected-agent",
        node_url,
        vec![Capability::Inference],
        false,
    )
    .await;
    let (url, server) = serve(app(state)).await;
    let response = reqwest::Client::new()
        .post(format!("{url}/nodes/expected-agent/execute"))
        .json(&inference())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    node.abort();
    server.abort();
}

#[tokio::test]
async fn upstream_errors_and_invalid_responses_are_rejected() {
    for (upstream_status, body, expected) in [
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed",
            StatusCode::BAD_GATEWAY,
        ),
        (
            StatusCode::GATEWAY_TIMEOUT,
            "timeout",
            StatusCode::GATEWAY_TIMEOUT,
        ),
        (StatusCode::OK, "invalid JSON", StatusCode::BAD_GATEWAY),
    ] {
        let state = state();
        let router = Router::new().route(
            "/execute",
            post(move || async move { (upstream_status, body) }),
        );
        let (node_url, node) = serve(router).await;
        register(
            &state,
            "test-node",
            node_url,
            vec![Capability::Inference],
            false,
        )
        .await;
        let (url, server) = serve(app(state)).await;
        let response = reqwest::Client::new()
            .post(format!("{url}/nodes/test-node/execute"))
            .json(&inference())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        node.abort();
        server.abort();
    }
}

#[tokio::test]
async fn slow_upstream_returns_gateway_timeout() {
    let mut state = state();
    state.http_client = reqwest::Client::builder()
        .timeout(Duration::from_millis(30))
        .build()
        .unwrap();
    let router = Router::new().route(
        "/execute",
        post(|| async {
            tokio::time::sleep(Duration::from_secs(1)).await;
            StatusCode::NO_CONTENT
        }),
    );
    let (node_url, node) = serve(router).await;
    register(
        &state,
        "slow-node",
        node_url,
        vec![Capability::Inference],
        false,
    )
    .await;
    let (url, server) = serve(app(state)).await;
    let response = reqwest::Client::new()
        .post(format!("{url}/nodes/slow-node/execute"))
        .json(&inference())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
    node.abort();
    server.abort();
}
