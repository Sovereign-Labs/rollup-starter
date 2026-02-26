//! Node discovery service that monitors cluster membership changes.
//!
//! This binary connects to a PostgreSQL database and subscribes to cluster
//! information updates, writing the current cluster state to an output file.
use std::{path::PathBuf, process::exit};

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
        let content = create_lua_backend_cache_content(cluster_info);
        write_to_file_atomically(&self.config_path, &content).await
    }

    async fn run_command(&self, args: &[&str], action: &str) -> anyhow::Result<()> {
        match Command::new(&self.nginx_binary).args(args).output().await {
            Ok(output) => {
                if output.status.success() {
                    Ok(())
                } else {
                    anyhow::bail!(
                        "Failed to {action} (exit_code={:?}, stderr={}, stdout={})",
                        output.status.code(),
                        String::from_utf8_lossy(&output.stderr),
                        String::from_utf8_lossy(&output.stdout),
                    )
                }
            }
            Err(error) => anyhow::bail!("Failed to execute nginx command for {action}: {error}"),
        }
    }

    async fn validate_config(&self) -> anyhow::Result<()> {
        self.run_command(&["-t"], "validate nginx config").await?;
        tracing::info!("Nginx configuration is valid");
        Ok(())
    }

    async fn reload(&self) -> anyhow::Result<()> {
        self.run_command(&["-s", "reload"], "reload nginx").await?;
        tracing::info!("Successfully reloaded nginx");
        Ok(())
    }
}

#[async_trait]
impl ClusterUpdateNotifier for ReloadNginx {
    async fn on_cluster_update(&mut self, _cluster_info: &ClusterInfo) -> anyhow::Result<()> {
        self.write_config(_cluster_info).await?;
        self.validate_config().await?;
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
        default_value = "/usr/local/openresty/nginx/conf/backends.generated.lua"
    )]
    output_file: String,

    /// Maximum age (in milliseconds) for cached cluster information.
    #[arg(long, default_value = "1000")]
    max_age_millis: u64,

    /// Nginx binary used for config test and reload commands (`<binary> -t` and `<binary> -s reload`).
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

    let content = create_lua_backend_cache_content(&ClusterInfo::default());
    write_to_file_atomically(&config_path, &content).await?;

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
        let _ = metrics_shutdown_sender.send(());
        exit(1);
    } else {
        let _ = metrics_shutdown_sender.send(());
    }

    Ok(())
}

fn create_lua_backend_cache_content(cluster_info: &ClusterInfo) -> String {
    let mut lines = vec![
        "local c = ngx.shared.backend_cache".to_string(),
        "c:flush_all()".to_string(),
    ];

    if let Some(leader) = &cluster_info.leader {
        lines.push(format!("c:set(\"leader\", \"{}\")", leader.address));
    }

    for (index, follower) in cluster_info.followers.values().enumerate() {
        lines.push(format!(
            "c:set(\"follower_{}\", \"{}\")",
            index + 1,
            follower.address
        ));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{create_lua_backend_cache_content, ClusterInfo};
    use sov_proxy_utils::{NodeInfo, OffsetDateTime};
    use std::collections::BTreeMap;

    fn node_info(node_id: &str, address: &str) -> NodeInfo {
        NodeInfo {
            node_id: node_id.to_owned(),
            address: address.parse().unwrap(),
            last_updated: OffsetDateTime::now_utc(),
        }
    }

    #[test]
    fn test_lua_backend_cache_content_for_empty_cluster() {
        let cluster_info = ClusterInfo::default();

        assert_eq!(
            create_lua_backend_cache_content(&cluster_info),
            "local c = ngx.shared.backend_cache\nc:flush_all()"
        );
    }

    #[test]
    fn test_lua_backend_cache_content_with_leader_only() {
        let cluster_info = ClusterInfo {
            leader: Some(node_info("leader_id", "127.0.0.1:3030")),
            followers: BTreeMap::new(),
        };

        assert_eq!(
            create_lua_backend_cache_content(&cluster_info),
            "local c = ngx.shared.backend_cache\nc:flush_all()\nc:set(\"leader\", \"127.0.0.1:3030\")"
        );
    }

    #[test]
    fn test_lua_backend_cache_content_orders_followers_by_node_id() {
        let mut followers = BTreeMap::new();
        followers.insert(
            "follower-b".to_owned(),
            node_info("follower-b", "127.0.0.1:3032"),
        );
        followers.insert(
            "follower-a".to_owned(),
            node_info("follower-a", "127.0.0.1:3031"),
        );

        let cluster_info = ClusterInfo {
            leader: Some(node_info("leader-1", "127.0.0.1:3030")),
            followers,
        };

        assert_eq!(
            create_lua_backend_cache_content(&cluster_info),
            "local c = ngx.shared.backend_cache\nc:flush_all()\nc:set(\"leader\", \"127.0.0.1:3030\")\nc:set(\"follower_1\", \"127.0.0.1:3031\")\nc:set(\"follower_2\", \"127.0.0.1:3032\")"
        );
    }
}
