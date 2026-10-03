use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};

use hive_protocol::{Heartbeat, NodeInfo};

use std::{
    collections::HashMap,
    sync::Arc,
    time::Instant,
};

use tokio::sync::RwLock;

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
) -> Json<Vec<NodeInfo>> {
    let nodes = state.nodes.read().await;
    let result = nodes
        .values()
        .map(|node| node.info.clone())
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

#[tokio::main]
async fn main() {
    let state = HiveState {
        nodes: Arc::new(RwLock::new(HashMap::new()))
    };
    let app = Router::new()
        .route("/nodes", get(list_nodes))
        .route("/nodes/register", post(register_node))
        .route("/nodes/heartbeat", post(heartbeat))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .unwrap();
    println!("Server running on http://0.0.0.0:8080");
    axum::serve(listener, app)
        .await
        .unwrap();
}