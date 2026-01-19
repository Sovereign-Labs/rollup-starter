use clap::Parser;
use sov_proxy_utils::NodeDiscovery;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "proxy")]
#[command(about = "Writes cluster info periodically")]
struct Args {
    /// PostgreSQL connection string
    #[arg(long)]
    database_url: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let node_discovery = NodeDiscovery::new(&args.database_url)
        .await
        .expect("Failed to create NodeDiscovery");

    node_discovery
        .write_cluster_info_loop(&"cluster_info.txt", Duration::from_millis(200))
        .await;
}
