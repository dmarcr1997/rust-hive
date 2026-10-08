use axum::{
    extract::State,
    http::StatusCode,
    routing::post,
    Json, Router
};

use hive_protocol::{
    Capability,
    Heartbeat,
    NodeInfo,
    TaskKind,
    TaskRequest,
    TaskResult,
};

use std::sync::Arc;

use sysinfo::System;

use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::{BufRead, BufWriter, BufReader, Write};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize)]
struct ObservationEntry {
    text: String,
    timestamp: u64,
}

#[derive(Clone)]
struct AgentState {
    node_id: String,
    capabilities: Arc<Vec<Capability>>,
}

async fn execute_task(
    State(state): State<AgentState>,
    Json(task_request): Json<TaskRequest>,
) -> Result<Json<TaskResult>, StatusCode> {
    if !state.capabilities.contains(&task_request.capability) {
        return Err(StatusCode::FORBIDDEN);
    }

    let output = match task_request.task {
        TaskKind::Echo { message } => message,
        TaskKind::GetSystemInfo => format!("Node {} is online", state.node_id),
        TaskKind::StoreObservation {text} => {
            let observation = ObservationEntry {
                text,
                timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            };
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open("observations.jsonl")
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let mut writer = BufWriter::new(file);
            let mut json_line = serde_json::to_vec(&observation).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            json_line.push(b'\n');
            writer.write_all(&json_line).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            writer.flush() .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            "Observation stored".to_string()
        },
        TaskKind::ListObservations => {
            let file = OpenOptions::new()
                .read(true)
                .open("observations.jsonl")
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let reader = BufReader::new(file);
            let observations: Vec<ObservationEntry> = reader
                .lines()
                .filter_map(|line| line.ok())
                .filter_map(|line| serde_json::from_str(&line).ok())
                .collect();
            serde_json::to_string(&observations).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        }
    };
    Ok(Json(TaskResult {
        node_id: state.node_id.clone(),
        output
    }))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let mut system = System::new_all();
    system.refresh_all();
     let hostname = hostname::get()?
        .to_string_lossy()
        .to_string();

    let cpu = system
        .cpus()
        .first()
        .map(|cpu| cpu.brand().to_string())
        .unwrap_or_else(|| "Unknown CPU".to_string());

    let cores = system.cpus().len();

    let memory_mb = system.total_memory() / 1024 / 1024;

    let architecture = std::env::consts::ARCH.to_string();
    let capabilities = std::env::var("HIVE_CAPABILITIES")
        .unwrap_or_else(|_| "Tools,Inference,Storage".to_string())
        .split(',')
        .filter_map(|cap| match cap.trim() {
            "Tools" => Some(Capability::Tools),
            "Inference" => Some(Capability::Inference),
            "Storage" => Some(Capability::Storage),
            _ => None,
        })
        .collect::<Vec<_>>();

    let agent_url = std::env::var("HIVE_AGENT_URL")
    .unwrap_or_else(|_| "http://127.0.0.1:9090".to_string());
    let node = NodeInfo {
        id: hostname.clone(),
        hostname,
        cpu,
        cores,
        memory_mb,
        architecture,
        capabilities,
        api_url: agent_url,
    };

    let node_id = node.id.clone();

    println!("Discovered node:");
    println!("{:#?}", node);

    let client = reqwest::Client::new();
    let control_url = std::env::var("HIVE_CONTROL_URL")
        .unwrap_or_else(|_| "http://localhost:8080".to_string());
    client
        .post(format!("{}/nodes/register", control_url))
        .json(&node)
        .send()
        .await?
        .error_for_status()?;

    let heartbeat_client = client.clone();
    let heartbeat_node_id = node_id.clone();
    tokio::spawn(async move {
        let mut system = sysinfo::System::new_all();
        loop {
            system.refresh_cpu_all();
            system.refresh_memory();
            let heartbeat = Heartbeat {
                 node_id: heartbeat_node_id.clone(),
                cpu_usage: system.global_cpu_usage(),
                memory_used_mb: system.used_memory() / 1024 / 1024,
            };
            match heartbeat_client
                .post(format!("{}/nodes/heartbeat", control_url))
                .json(&heartbeat)
                .send()
                .await
            {
                Ok(_) => {
                    println!(
                        "Heartbeat sent | CPU {:.1}% | RAM {} MB",
                        heartbeat.cpu_usage,
                        heartbeat.memory_used_mb
                    );
                }

                Err(error) => {
                    eprintln!("Heartbeat failed: {}", error);
                }
            }

            tokio::time::sleep(
                std::time::Duration::from_secs(5)
            ).await;
        }
    });

    let agent_state = AgentState {
        node_id: node_id.clone(),
        capabilities: Arc::new(node.capabilities.clone())
    };

    let app = Router::new()
        .route("/execute", post(execute_task))
        .with_state(agent_state);

    let listener =
    tokio::net::TcpListener::bind("0.0.0.0:9090")
        .await?;

    println!("Hive agent listening on port 9090");

    axum::serve(listener, app).await?;
    Ok(())
}