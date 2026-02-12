//! Node discovery service that monitors cluster membership changes.
//!
//! This binary connects to a PostgreSQL database and subscribes to cluster
//! information updates, writing the current cluster state to an output file.
use std::io::Write;
use std::path::PathBuf;

use async_trait::async_trait;
use clap::Parser;
use sov_metrics::{init_metrics_tracker, MonitoringConfig};
use sov_proxy_utils::{ClusterInfo, ClusterInfoService, ClusterUpdateNotifier};
use tokio::process::Command;

struct ReloadOpenResty {
    nginx_binary: PathBuf,
}

#[derive(Debug)]
struct NginxReloadFailureMetric {
    reason: &'static str,
}

impl sov_metrics::Metric for NginxReloadFailureMetric {
    fn measurement_name(&self) -> &'static str {
        "sov_proxy_nginx_reload_failure"
    }

    fn serialize_for_telegraf(&self, buffer: &mut Vec<u8>) -> std::io::Result<()> {
        write!(
            buffer,
            "{},reason={} failures=1",
            self.measurement_name(),
            self.reason,
        )
    }
}

fn emit_nginx_reload_failure_metric(reason: &'static str) {
    sov_metrics::track_metrics(|tracker| {
        tracker.submit(NginxReloadFailureMetric { reason });
    });
}

#[async_trait]
impl ClusterUpdateNotifier for ReloadOpenResty {
    async fn on_cluster_update(&self, _cluster_info: &ClusterInfo) {
        match Command::new(&self.nginx_binary)
            .args(["-s", "reload"])
            .output()
            .await
        {
            Ok(output) => {
                if output.status.success() {
                    tracing::info!("Successfully reloaded nginx");
                } else {
                    emit_nginx_reload_failure_metric("nonzero_exit");
                    tracing::error!(
                        exit_code = ?output.status.code(),
                        stderr = ?String::from_utf8_lossy(&output.stderr),
                        stdout = ?String::from_utf8_lossy(&output.stdout),
                        "Failed to reload nginx"
                    );
                }
            }
            Err(e) => {
                emit_nginx_reload_failure_metric("command_error");
                tracing::error!(error = ?e, "Failed to execute reload nginx");
            }
        }
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

    let cluster_info_service = ClusterInfoService::spawn(
        &args.database_url,
        max_age,
        PathBuf::from(&args.output_file),
        Some(Box::new(ReloadOpenResty {
            nginx_binary: PathBuf::from(args.nginx_binary),
        })),
    )
    .await?;

    cluster_info_service.join().await?;
    let _ = metrics_shutdown_sender.send(());

    Ok(())
}
