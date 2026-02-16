//! Node discovery service that monitors cluster membership changes.
//!
//! This binary connects to a PostgreSQL database and subscribes to cluster
//! information updates, writing the current cluster state to an output file.
use std::path::PathBuf;

use async_trait::async_trait;
use clap::Parser;
use sov_metrics::{init_metrics_tracker, MonitoringConfig};
use sov_proxy_utils::{
    write_to_file_atomically, ClusterInfo, ClusterInfoService, ClusterUpdateNotifier,
};
use tokio::process::Command;

struct ReloadNginx {
    nginx_binary: PathBuf,
    config_path: PathBuf,
}

impl ReloadNginx {
    async fn write_config(&self, cluster_info: &ClusterInfo) -> anyhow::Result<()> {
        let content = to_lua_backend_cache_content(cluster_info);
        write_to_file_atomically(&self.config_path, &content).await
    }

    async fn reload(&self) -> anyhow::Result<()> {
        match Command::new(&self.nginx_binary)
            .args(["-s", "reload"])
            .output()
            .await
        {
            Ok(output) => {
                if output.status.success() {
                    tracing::info!("Successfully reloaded nginx");
                    Ok(())
                } else {
                    anyhow::bail!(
                        "Failed to reload nginx (exit_code={:?}, stderr={}, stdout={})",
                        output.status.code(),
                        String::from_utf8_lossy(&output.stderr),
                        String::from_utf8_lossy(&output.stdout),
                    )
                }
            }
            Err(error) => anyhow::bail!("Failed to execute reload nginx: {error}"),
        }
    }
}

#[async_trait]
impl ClusterUpdateNotifier for ReloadNginx {
    async fn on_cluster_update(&mut self, _cluster_info: &ClusterInfo) -> anyhow::Result<()> {
        self.write_config(_cluster_info).await?;
        self.reload().await
    }
}

#[derive(Parser)]
#[command(name = "node-discovery")]
struct Args {
    /// PostgreSQL connection string.
    #[arg(long)]
    database_url: String,

    /// Output file path.
    #[arg(
        long,
        default_value = "/usr/local/openresty/nginx/conf/cluster_info.txt"
    )]
    output_file: String,

    /// Maximum age (in milliseconds) for cached cluster information.
    #[arg(long, default_value = "1000")]
    max_age_millis: u64,

    /// Nginx binary used for reload command (`<binary> -s reload`).
    #[arg(long, default_value = "/usr/local/openresty/nginx/sbin/nginx")]
    nginx_binary: String,

    /// UDP port for sov-metrics telegraf exporter.
    #[arg(long, default_value_t = 8094)]
    metrics_port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug,sqlx=info,hyper=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let args = Args::parse();
    tracing::info!("Starting node discovery.");

    let (metrics_shutdown_sender, mut metrics_shutdown_receiver) = tokio::sync::watch::channel(());
    metrics_shutdown_receiver.mark_unchanged();

    let monitoring_config = MonitoringConfig::default_on_port(args.metrics_port);
    init_metrics_tracker(&monitoring_config, metrics_shutdown_receiver.clone());

    let max_age = std::time::Duration::from_millis(args.max_age_millis);

    let config_path = PathBuf::from(args.output_file);
    write_to_file_atomically(&config_path, "").await?;

    let cluster_info_service = ClusterInfoService::spawn(
        &args.database_url,
        max_age,
        Some(Box::new(ReloadNginx {
            nginx_binary: PathBuf::from(args.nginx_binary),
            config_path,
        })),
    )
    .await?;

    if let Err(err) = cluster_info_service.join().await {
        tracing::error!(?err, "Failed to join cluster info service");
    }
    let _ = metrics_shutdown_sender.send(());

    Ok(())
}

fn to_lua_backend_cache_content(cluster_info: &ClusterInfo) -> String {
    let mut lines = vec![
        "local c = ngx.shared.backend_cache".to_string(),
        "c:flush_all()".to_string(),
    ];

    if let Some(leader) = &cluster_info.leader {
        lines.push(format!("c:set(\"leader\", \"{}\")", leader.address));
    }

    for (index, follower) in cluster_info.followers.values().enumerate() {
        lines.push(format!(
            "  c:set(\"follower_{}\", \"{}\")",
            index + 1,
            follower.address
        ));
    }

    lines.join("\n")
}
