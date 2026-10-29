use axum::{Json, Router, routing::get};
use serde::Serialize;
use std::{error::Error, path::PathBuf};
use tower_http::services::ServeDir;

#[derive(Serialize)]
struct Health {
    status: &'static str,
    name: &'static str,
    version: &'static str,
    stage: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        name: "pi-trace",
        version: env!("CARGO_PKG_VERSION"),
        stage: "bootstrap",
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let web_dist = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web/dist");
    let app = Router::new()
        .route("/api/v1/health", get(health))
        .fallback_service(ServeDir::new(web_dist));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:4317").await?;

    println!("Pi Trace bootstrap: http://127.0.0.1:4317");
    println!("Build the web UI with: npm --prefix web run build");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
