use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

struct HiveState {
}

fn main() {
    println!("Hello, world!");
}
