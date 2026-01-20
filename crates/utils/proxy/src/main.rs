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

    #[arg(long)]
    output_file: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let node_discovery = NodeDiscovery::new(&args.database_url)
        .await
        .expect("Failed to create NodeDiscovery");

    node_discovery
        .write_cluster_info_loop(&args.output_file, Duration::from_millis(200))
        .await;
}
