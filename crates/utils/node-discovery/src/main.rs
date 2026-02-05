//! Node discovery service that monitors cluster membership changes.
//!
//! This binary connects to a PostgreSQL database and subscribes to cluster
//! information updates, writing the current cluster state to an output file.
use async_trait::async_trait;
use clap::Parser;
use sov_proxy_utils::{ClusterInfo, ClusterUpdateNotifier, NodeDiscovery};
use tokio::process::Command;

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

struct ReloadOpenResty;

#[async_trait]
impl ClusterUpdateNotifier for ReloadOpenResty {
    async fn on_cluster_update(&self, _cluster_info: &ClusterInfo) {
        match Command::new("systemctl")
            .arg("reload")
            .arg("openresty")
            .output()
            .await
        {
            Ok(output) => {
                if output.status.success() {
                    tracing::info!("Successfully reloaded openresty");
                } else {
                    tracing::error!(
                        exit_code = ?output.status.code(),
                        stderr = ?String::from_utf8_lossy(&output.stderr),
                        stdout = ?String::from_utf8_lossy(&output.stdout),
                        "Failed to reload openresty"
                    );
                }
            }
            Err(e) => {
                tracing::error!(error = ?e, "Failed to execute systemctl reload openresty");
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug,sqlx=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let args = Args::parse();

    tracing::info!("Starting node discovery.");

    let max_age = std::time::Duration::from_millis(args.max_age_millis);

    let mut node_discovery =
        NodeDiscovery::new(&args.database_url, max_age, Box::new(ReloadOpenResty))
            .await
            .expect("Failed to create NodeDiscovery");

    node_discovery
        .subscribe_cluster_info_loop(&args.output_file)
        .await
        .unwrap_or_else(|e| panic!("Failed to start node discovery loop: {e:?}"));
}
