use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::Html,
    routing::{get, post},
};

use hive_protocol::{
    Heartbeat, NodeInfo, NodeStatus, NodeSummary, TaskAssignment, TaskRequest, TaskResult,
};

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::RwLock;

use reqwest;

#[cfg(test)]
mod tests;

const NODE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone)]
struct NodeState {
    info: NodeInfo,
    last_heartbeat: Instant,
    cpu_usage: f32,
    memory_used_mb: u64,
}

#[derive(Debug, Clone)]
struct HiveState {
    nodes: Arc<RwLock<HashMap<String, NodeState>>>,
    http_client: reqwest::Client,
}

async fn register_node(State(state): State<HiveState>, Json(node): Json<NodeInfo>) {
    let mut nodes = state.nodes.write().await;
    let node_state = NodeState {
        info: node.clone(),
        last_heartbeat: Instant::now(),
        cpu_usage: 0.0,
        memory_used_mb: 0,
    };
    nodes.insert(node.id.clone(), node_state);
    println!("Node registered: {}", node.id);
}

async fn list_nodes(State(state): State<HiveState>) -> Json<Vec<NodeSummary>> {
    let nodes = state.nodes.read().await;
    let result = nodes
        .values()
        .map(|node| {
            let elapsed = node.last_heartbeat.elapsed();
            let status = if elapsed <= NODE_TIMEOUT {
                NodeStatus::Online
            } else {
                NodeStatus::Offline
            };
            NodeSummary {
                info: node.info.clone(),
                status,
                cpu_usage: node.cpu_usage,
                memory_used_mb: node.memory_used_mb,
                last_seen_seconds_ago: elapsed.as_secs(),
            }
        })
        .collect();
    Json(result)
}

async fn heartbeat(State(state): State<HiveState>, Json(heartbeat): Json<Heartbeat>) -> StatusCode {
    let mut nodes = state.nodes.write().await;
    if let Some(node) = nodes.get_mut(&heartbeat.node_id) {
        node.last_heartbeat = Instant::now();
        node.cpu_usage = heartbeat.cpu_usage;
        node.memory_used_mb = heartbeat.memory_used_mb;

        println!(
            "Heartbeat from {} | CPU: {}% | Memory: {} MB",
            heartbeat.node_id, heartbeat.cpu_usage, heartbeat.memory_used_mb
        );
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn assign_task(
    State(state): State<HiveState>,
    Json(task_request): Json<TaskRequest>,
) -> Result<Json<TaskAssignment>, StatusCode> {
    let nodes = state.nodes.read().await;

    let node = nodes
        .values()
        .filter(|node| node.last_heartbeat.elapsed() <= NODE_TIMEOUT)
        .find(|node| node.info.capabilities.contains(&task_request.capability));
    match node {
        Some(node) => Ok(Json(TaskAssignment {
            node_id: node.info.id.clone(),
        })),
        None => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

async fn execute_task(
    State(state): State<HiveState>,
    Json(task): Json<TaskRequest>,
) -> Result<Json<TaskResult>, StatusCode> {
    let selected_node = {
        let nodes = state.nodes.read().await;
        nodes
            .values()
            .filter(|node| node.last_heartbeat.elapsed() <= NODE_TIMEOUT)
            .find(|node| node.info.capabilities.contains(&task.capability))
            .map(|node| node.info.clone())
    };
    let node = selected_node.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    dispatch_task(&state, &node, &task).await
}

async fn execute_node_task(
    Path(node_id): Path<String>,
    State(state): State<HiveState>,
    Json(task): Json<TaskRequest>,
) -> Result<Json<TaskResult>, StatusCode> {
    let node = {
        let nodes = state.nodes.read().await;
        let node = nodes.get(&node_id).ok_or(StatusCode::NOT_FOUND)?;
        if node.last_heartbeat.elapsed() > NODE_TIMEOUT {
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
        if !node.info.capabilities.contains(&task.capability) {
            return Err(StatusCode::FORBIDDEN);
        }
        node.info.clone()
    };
    dispatch_task(&state, &node, &task).await
}

async fn dispatch_task(
    state: &HiveState,
    node: &NodeInfo,
    task: &TaskRequest,
) -> Result<Json<TaskResult>, StatusCode> {
    let url = format!("{}/execute", node.api_url.trim_end_matches('/'));
    println!("Dispatching to {} at {}", node.hostname, url);

    let response = state
        .http_client
        .post(url)
        .json(task)
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                StatusCode::GATEWAY_TIMEOUT
            } else {
                StatusCode::BAD_GATEWAY
            }
        })?;

    if response.status() == StatusCode::GATEWAY_TIMEOUT {
        return Err(StatusCode::GATEWAY_TIMEOUT);
    }
    if !response.status().is_success() {
        return Err(StatusCode::BAD_GATEWAY);
    }

    let result = response.json::<TaskResult>().await.map_err(|error| {
        if error.is_timeout() {
            StatusCode::GATEWAY_TIMEOUT
        } else {
            StatusCode::BAD_GATEWAY
        }
    })?;

    if result.node_id != node.id {
        return Err(StatusCode::BAD_GATEWAY);
    }
    Ok(Json(result))
}

fn app(state: HiveState) -> Router {
    Router::new()
        .route("/", get(|| async { Html(include_str!("dashboard.html")) }))
        .route("/nodes", get(list_nodes))
        .route("/nodes/register", post(register_node))
        .route("/nodes/heartbeat", post(heartbeat))
        .route("/nodes/{node_id}/execute", post(execute_node_task))
        .route("/tasks/assign", post(assign_task))
        .route("/tasks/execute", post(execute_task))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let state = HiveState {
        nodes: Arc::new(RwLock::new(HashMap::new())),
        http_client: reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(195))
            .build()
            .expect("HTTP client configuration is valid"),
    };
    let app = app(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    println!("Server running on http://0.0.0.0:8080");
    axum::serve(listener, app).await.unwrap();
}
