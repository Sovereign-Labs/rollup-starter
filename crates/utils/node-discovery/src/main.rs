//! Node discovery service that monitors cluster membership changes.
//!
//! This binary connects to a PostgreSQL database and subscribes to cluster
//! information updates, writing the current cluster state to an output file.
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
}

#[tokio::main]
async fn main() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug,sqlx=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let args = Args::parse();

    tracing::info!("Starting node discovery.");

    let (node_discovery, _) = NodeDiscovery::new(&args.database_url)
        .await
        .expect("Failed to create NodeDiscovery");

    node_discovery
        .subscribe_cluster_info_loop(&args.output_file)
        .await
        .unwrap_or_else(|e| panic!("Failed to start node discovery loop: {:?}", e));
}
