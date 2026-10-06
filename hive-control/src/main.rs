use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};

use hive_protocol::{
    Heartbeat,
    NodeInfo,
    NodeStatus,
    NodeSummary,
    TaskAssignment,
    TaskRequest
};

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::RwLock;

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
    nodes: Arc<RwLock<HashMap<String, NodeState>>>
}

async fn register_node(
    State(state): State<HiveState>,
    Json(node): Json<NodeInfo>
) {
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

async fn list_nodes(
    State(state): State<HiveState>
) -> Json<Vec<NodeSummary>> {
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

async fn heartbeat(
    State(state): State<HiveState>,
    Json(heartbeat): Json<Heartbeat>
) -> StatusCode {
    let mut nodes = state.nodes.write().await;
    if let Some(node) = nodes.get_mut(&heartbeat.node_id) {
        node.last_heartbeat = Instant::now();
        node.cpu_usage = heartbeat.cpu_usage;
        node.memory_used_mb = heartbeat.memory_used_mb;

        println!("Heartbeat from {} | CPU: {}% | Memory: {} MB", 
            heartbeat.node_id, heartbeat.cpu_usage,
            heartbeat.memory_used_mb
        );
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn assign_task(
    State(state): State<HiveState>,
    Json(task_request): Json<TaskRequest>
) -> Result<Json<TaskAssignment>, StatusCode> {
    let nodes = state.nodes.read().await;

    let node = nodes
        .values()
        .filter(|node| node.last_heartbeat.elapsed() <= NODE_TIMEOUT)
        .find(|node| {
            node.info.capabilities.contains(&task_request.capability)
        });
    match node {
        Some(node) => Ok(Json(TaskAssignment {
            node_id: node.info.id.clone()
        })),
        None => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

#[tokio::main]
async fn main() {
    let state = HiveState {
        nodes: Arc::new(RwLock::new(HashMap::new()))
    };
    let app = Router::new()
        .route("/nodes", get(list_nodes))
        .route("/nodes/register", post(register_node))
        .route("/nodes/heartbeat", post(heartbeat))
        .route("/tasks/assign", post(assign_task))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .unwrap();
    println!("Server running on http://0.0.0.0:8080");
    axum::serve(listener, app)
        .await
        .unwrap();
}