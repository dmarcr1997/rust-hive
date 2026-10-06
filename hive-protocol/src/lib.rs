use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub hostname: String,
    pub cpu: String,
    pub cores: usize,
    pub memory_mb: u64,
    pub architecture: String,
    pub capabilities: Vec<Capability>
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Capability {
    Coordinator,
    Inference,
    Rag,
    Storage,
    Tools
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heartbeat {
    pub node_id: String,
    pub cpu_usage: f32,
    pub memory_used_mb: u64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeStatus {
    Online,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSummary {
    pub info: NodeInfo,
    pub status: NodeStatus,
    pub cpu_usage: f32,
    pub memory_used_mb: u64,
    pub last_seen_seconds_ago: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequest {
    pub capability: Capability
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssignment {
    pub node_id: String
}