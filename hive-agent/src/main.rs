use hive_protocol::{Capability, NodeInfo};
use sysinfo::System;

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
        ],
    };

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

    Ok(())
}