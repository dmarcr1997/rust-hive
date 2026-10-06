use hive_protocol::{Capability, Heartbeat, NodeInfo};
use sysinfo::System;

use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    let node = NodeInfo {
        id: hostname.clone(),
        hostname,
        cpu,
        cores,
        memory_mb,
        architecture,
        capabilities: vec![
            Capability::Tools,
            Capability::Inference,
        ],
    };

    let node_id = node.id.clone();

    println!("Discovered node:");
    println!("{:#?}", node);

    let client = reqwest::Client::new();

    client
        .post("http://localhost:8080/nodes/register")
        .json(&node)
        .send()
        .await?
        .error_for_status()?;

    println!("Node registered successfully.");
    loop {
        system.refresh_cpu_all();
        system.refresh_memory();
        let heartbeat = Heartbeat {
            node_id: node_id.clone(),
            cpu_usage: system.global_cpu_usage(),
            memory_used_mb: system.used_memory() / 1024 / 1024,
        };

        client
            .post("http://localhost:8080/nodes/heartbeat")
            .json(&heartbeat)
            .send()
            .await?
            .error_for_status()?;
         println!(
            "Heartbeat sent | CPU: {:.1}% | Memory: {} MB",
            heartbeat.cpu_usage,
            heartbeat.memory_used_mb
        );

        sleep(Duration::from_secs(5)).await;
    }
    Ok(())
}