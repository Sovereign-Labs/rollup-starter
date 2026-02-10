//! Node discovery service that monitors cluster membership changes.
//!
//! This binary connects to a PostgreSQL database and subscribes to cluster
//! information updates, writing the current cluster state to an output file.
use std::path::PathBuf;

use clap::Parser;
use sov_proxy_utils::NodeDiscovery;

#[derive(Parser)]
#[command(name = "node-discovery")]
struct Args {
    /// PostgreSQL connection string.
    #[arg(long)]
    database_url: String,

    /// Output file path.
    #[arg(long)]
    output_file: String,

    /// Maximum age (in milliseconds) for cached cluster information.
    #[arg(long, default_value = "1000")]
    max_age_millis: u64,
}

#[tokio::main]
async fn main() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug,sqlx=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let args = Args::parse();

    tracing::info!("Starting node discovery.");

    let max_age = std::time::Duration::from_millis(args.max_age_millis);
    let node_discovery = NodeDiscovery::connect(
        &args.database_url,
        max_age,
        PathBuf::from(&args.output_file),
        None,
    )
    .await
    .expect("Failed to create NodeDiscovery");

    let task = node_discovery.spawn();
    task.handle
        .await
        .unwrap_or_else(|e| panic!("Node discovery task panicked: {e:?}"))
        .unwrap_or_else(|e| panic!("Failed to start node discovery loop: {e:?}"));
}
