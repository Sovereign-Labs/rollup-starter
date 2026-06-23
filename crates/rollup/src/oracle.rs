use std::collections::{HashMap, HashSet};
use std::fs;
use std::future;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context};
use price_oracle_ipc::{
    connect, read_frame_with_timeout, Backoff, Endpoint, IpcError, OracleFrame, OracleStream,
    PROTOCOL_VERSION,
};
use serde::Deserialize;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, trace, warn};

use stf_starter::prices;

const ORACLE_CONFIG_FILE: &str = "oracle.toml";
const ORACLE_CONFIG_CONTENT: &str = "[oracle]\nenabled = false\n";
const DEADLINE_HEARTBEAT_MULTIPLIER: u32 = 3;
const BOOTSTRAP_DEADLINE: Duration = Duration::from_secs(30);
const SUPERVISOR_GUARD_MIN: Duration = Duration::from_secs(1);
const SUPERVISOR_GUARD_MAX: Duration = Duration::from_secs(30);
const HEALTHY_CONNECTION_THRESHOLD: Duration = Duration::from_secs(10);
const STALENESS_WARN_SEC: u64 = 30;
const REQUIRE_SOURCES_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OracleConfigFile {
    pub oracle: OracleConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OracleConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub require_sources: bool,
    #[serde(default, rename = "source")]
    pub sources: Vec<SourceConfig>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    #[default]
    Unix,
    Tcp,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceConfig {
    pub name: String,
    #[serde(default)]
    pub transport: Transport,
    #[serde(default)]
    pub socket_path: Option<PathBuf>,
    #[serde(default)]
    pub socket_address: Option<String>,
}

impl SourceConfig {
    fn endpoint(&self) -> anyhow::Result<Endpoint> {
        match self.transport {
            Transport::Unix => {
                if self.socket_address.is_some() {
                    return Err(anyhow!(
                        "oracle source '{}': transport = \"unix\" but socket_address is set",
                        self.name
                    ));
                }
                let path = self.socket_path.clone().ok_or_else(|| {
                    anyhow!(
                        "oracle source '{}': transport = \"unix\" requires socket_path",
                        self.name
                    )
                })?;
                Ok(Endpoint::Unix(path))
            }
            Transport::Tcp => {
                if self.socket_path.is_some() {
                    return Err(anyhow!(
                        "oracle source '{}': transport = \"tcp\" but socket_path is set",
                        self.name
                    ));
                }
                let address = self.socket_address.clone().ok_or_else(|| {
                    anyhow!(
                        "oracle source '{}': transport = \"tcp\" requires socket_address",
                        self.name
                    )
                })?;
                Ok(Endpoint::Tcp(address))
            }
        }
    }
}

pub fn resolve_config_path(explicit: Option<PathBuf>, rollup_config_path: &Path) -> PathBuf {
    match explicit {
        Some(path) => path,
        None => {
            let dir = rollup_config_path
                .parent()
                .unwrap_or_else(|| Path::new("."));
            dir.join(ORACLE_CONFIG_FILE)
        }
    }
}

fn validate(config: &OracleConfig) -> anyhow::Result<()> {
    let mut seen = HashSet::new();
    for source in &config.sources {
        if !seen.insert(source.name.as_str()) {
            return Err(anyhow!("duplicate oracle source name '{}'", source.name));
        }
        source.endpoint()?;
    }
    Ok(())
}

pub fn load_config(path: &Path) -> anyhow::Result<Option<OracleConfig>> {
    match fs::read_to_string(path) {
        Ok(contents) => {
            let file: OracleConfigFile = toml::from_str(&contents)
                .with_context(|| format!("failed to parse oracle config {}", path.display()))?;
            validate(&file.oracle)
                .with_context(|| format!("invalid oracle config {}", path.display()))?;
            Ok(Some(file.oracle))
        }
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow!(
            "failed to read oracle config {}: {e}",
            path.display()
        )),
    }
}

pub fn load_or_create_config(path: &Path) -> anyhow::Result<OracleConfig> {
    if let Some(config) = load_config(path)? {
        return Ok(config);
    }
    match fs::write(path, ORACLE_CONFIG_CONTENT) {
        Ok(()) => info!(
            path = %path.display(),
            "Oracle config not found, created default disabled config"
        ),
        Err(e) => error!(
            path = %path.display(),
            error = %e,
            "Failed to create default oracle config, proceeding with oracle disabled"
        ),
    }
    Ok(OracleConfig::default())
}

struct SourceHandle {
    config: SourceConfig,
    cancel: watch::Sender<()>,
    join: JoinHandle<()>,
}

impl SourceHandle {
    async fn shutdown(self) {
        let _ = self.cancel.send(());
        let _ = self.join.await;
    }
}

pub async fn spawn_clients(config: OracleConfig, path: PathBuf) -> anyhow::Result<()> {
    if !config.enabled {
        info!("Price oracle disabled via config, not connecting to any source");
        tokio::spawn(ignore_sighup_while_disabled());
        return Ok(());
    }

    let startup_config = config.clone();

    // When require_sources is set, each supervisor reports its name on first connect and we block startup until all report or time out.
    let (ready_tx, ready_rx) = if config.require_sources {
        let (tx, rx) = mpsc::unbounded_channel::<String>();
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };

    let mut registry: HashMap<String, SourceHandle> = HashMap::new();
    for source in config.sources {
        let name = source.name.clone();
        if let Some(handle) = spawn_source(source, ready_tx.clone()) {
            registry.insert(name, handle);
        }
    }
    drop(ready_tx);

    if let Some(ready_rx) = ready_rx {
        let pending: HashSet<String> = registry.keys().cloned().collect();
        await_sources_ready(pending, ready_rx, REQUIRE_SOURCES_TIMEOUT).await?;
        info!("All required oracle sources connected");
    }

    tokio::spawn(reload_manager(path, startup_config, registry));
    Ok(())
}

async fn await_sources_ready(
    mut pending: HashSet<String>,
    mut ready_rx: mpsc::UnboundedReceiver<String>,
    timeout: Duration,
) -> anyhow::Result<()> {
    let deadline = tokio::time::Instant::now() + timeout;
    while !pending.is_empty() {
        match tokio::time::timeout_at(deadline, ready_rx.recv()).await {
            Ok(Some(name)) => {
                pending.remove(&name);
            }
            Ok(None) | Err(_) => break,
        }
    }
    if pending.is_empty() {
        return Ok(());
    }
    let mut unconnected: Vec<String> = pending.into_iter().collect();
    unconnected.sort();
    Err(anyhow!(
        "require_sources is set, but {} source(s) did not connect within {}s: {}",
        unconnected.len(),
        timeout.as_secs(),
        unconnected.join(", ")
    ))
}

fn spawn_source(
    source: SourceConfig,
    ready: Option<mpsc::UnboundedSender<String>>,
) -> Option<SourceHandle> {
    let endpoint = match source.endpoint() {
        Ok(endpoint) => endpoint,
        Err(e) => {
            error!(source = %source.name, error = %e, "Skipping oracle source with invalid endpoint");
            return None;
        }
    };
    info!(source = %source.name, endpoint = %endpoint, "Starting oracle source client");
    let (cancel_tx, cancel_rx) = watch::channel(());
    let join = tokio::spawn(supervise_source(source.clone(), endpoint, ready, cancel_rx));
    Some(SourceHandle {
        config: source,
        cancel: cancel_tx,
        join,
    })
}

async fn ignore_sighup_while_disabled() {
    let mut sighup = match signal(SignalKind::hangup()) {
        Ok(sighup) => sighup,
        Err(e) => {
            error!(error = %e, "Failed to install SIGHUP handler");
            return;
        }
    };
    loop {
        sighup.recv().await;
        info!("Received SIGHUP, but the price oracle is disabled, ignoring reload");
    }
}

async fn reload_manager(
    path: PathBuf,
    startup_config: OracleConfig,
    mut registry: HashMap<String, SourceHandle>,
) {
    let mut sighup = match signal(SignalKind::hangup()) {
        Ok(sighup) => sighup,
        Err(e) => {
            error!(error = %e, "Failed to install SIGHUP handler, oracle config reload disabled");
            // Park forever so the running sources (held in `registry`) are not dropped.
            future::pending::<()>().await;
            return;
        }
    };

    loop {
        sighup.recv().await;
        info!("Received SIGHUP, reloading oracle config");

        let new_config = match load_config(&path) {
            Ok(Some(config)) => config,
            Ok(None) => {
                warn!("Oracle config not found on reload, keeping current sources");
                continue;
            }
            Err(e) => {
                error!(error = %format!("{e:#}"), "Oracle config reload failed, keeping current sources");
                continue;
            }
        };

        if new_config.enabled != startup_config.enabled
            || new_config.require_sources != startup_config.require_sources
        {
            warn!("Oracle top-level config is startup-only, ignoring changed values until restart");
        }

        apply_source_diff(&mut registry, new_config.sources).await;
    }
}

struct SourceDiff {
    to_add: Vec<SourceConfig>,
    to_restart: Vec<SourceConfig>,
    to_remove: Vec<String>,
}

fn diff_sources(
    current: &HashMap<String, SourceConfig>,
    new_sources: Vec<SourceConfig>,
) -> SourceDiff {
    let mut diff = SourceDiff {
        to_add: Vec::new(),
        to_restart: Vec::new(),
        to_remove: Vec::new(),
    };
    let mut new_names = HashSet::new();
    for source in new_sources {
        new_names.insert(source.name.clone());
        match current.get(&source.name) {
            None => diff.to_add.push(source),
            Some(existing) if *existing != source => diff.to_restart.push(source),
            Some(_) => {}
        }
    }
    for name in current.keys() {
        if !new_names.contains(name) {
            diff.to_remove.push(name.clone());
        }
    }
    diff
}

async fn apply_source_diff(
    registry: &mut HashMap<String, SourceHandle>,
    new_sources: Vec<SourceConfig>,
) {
    let current: HashMap<String, SourceConfig> = registry
        .iter()
        .map(|(name, handle)| (name.clone(), handle.config.clone()))
        .collect();
    let diff = diff_sources(&current, new_sources);
    let (added, restarted, removed) = (
        diff.to_add.len(),
        diff.to_restart.len(),
        diff.to_remove.len(),
    );

    for name in diff.to_remove {
        if let Some(handle) = registry.remove(&name) {
            info!(source = %name, "Removing oracle source (config reload)");
            handle.shutdown().await;
        }
    }
    for source in diff.to_restart {
        let name = source.name.clone();
        if let Some(handle) = registry.remove(&name) {
            info!(source = %name, "Restarting oracle source (config changed)");
            handle.shutdown().await;
        }
        if let Some(handle) = spawn_source(source, None) {
            registry.insert(name, handle);
        }
    }
    for source in diff.to_add {
        let name = source.name.clone();
        info!(source = %name, "Adding oracle source (config reload)");
        if let Some(handle) = spawn_source(source, None) {
            registry.insert(name, handle);
        }
    }

    debug!(added, restarted, removed, "Applied oracle config reload");
}

async fn supervise_source(
    source: SourceConfig,
    endpoint: Endpoint,
    ready: Option<mpsc::UnboundedSender<String>>,
    mut cancel: watch::Receiver<()>,
) {
    let mut guard = Backoff::new(SUPERVISOR_GUARD_MIN, SUPERVISOR_GUARD_MAX);
    loop {
        let started = Instant::now();
        let mut child = tokio::spawn(run_source(source.clone(), endpoint.clone(), ready.clone()));

        tokio::select! {
            biased;
            _ = cancel.changed() => {
                child.abort();
                let _ = child.await;
                metrics::set_connected(&source.name, false);
                info!(source = %source.name, "Oracle source stopped (config reload)");
                return;
            }
            result = &mut child => match result {
                Ok(()) => {
                    warn!(source = %source.name, "Oracle client task exited unexpectedly, restarting");
                }
                Err(join_error) => {
                    metrics::set_connected(&source.name, false);
                    error!(source = %source.name, error = %join_error, "Oracle client task panicked, restarting");
                }
            },
        }

        if started.elapsed() >= HEALTHY_CONNECTION_THRESHOLD {
            guard.reset();
        }

        tokio::select! {
            biased;
            _ = cancel.changed() => {
                info!(source = %source.name, "Oracle source stopped (config reload)");
                return;
            }
            _ = tokio::time::sleep(guard.next_delay()) => {}
        }
    }
}

fn heartbeat_deadline(heartbeat_interval_sec: u32) -> Duration {
    if heartbeat_interval_sec == 0 {
        return BOOTSTRAP_DEADLINE;
    }
    Duration::from_secs(
        u64::from(heartbeat_interval_sec) * u64::from(DEADLINE_HEARTBEAT_MULTIPLIER),
    )
}

async fn run_source(
    source: SourceConfig,
    endpoint: Endpoint,
    ready: Option<mpsc::UnboundedSender<String>>,
) {
    let mut backoff = Backoff::default();
    let mut reconnect = false;
    loop {
        let stream = match connect(&endpoint).await {
            Ok(stream) => {
                if reconnect {
                    metrics::inc_reconnects(&source.name);
                    info!(source = %source.name, "Connected to oracle source");
                } else if let Some(ready) = &ready {
                    let _ = ready.send(source.name.clone());
                }
                stream
            }
            Err(e) => {
                let delay = backoff.next_delay();
                warn!(
                    source = %source.name,
                    error = %e,
                    retry_in_secs = delay.as_secs(),
                    "Oracle source connect failed, retrying"
                );
                tokio::time::sleep(delay).await;
                continue;
            }
        };
        reconnect = true;

        metrics::set_connected(&source.name, true);
        let started = Instant::now();
        let outcome = consume(&source, stream).await;
        metrics::set_connected(&source.name, false);

        if outcome.hello && started.elapsed() >= HEALTHY_CONNECTION_THRESHOLD {
            backoff.reset();
        }

        match outcome.error {
            IpcError::Closed => {
                info!(source = %source.name, "Oracle source disconnected, reconnecting")
            }
            other => {
                warn!(source = %source.name, error = %other, "Oracle source connection error, reconnecting")
            }
        }
        tokio::time::sleep(backoff.next_delay()).await;
    }
}

struct SessionOutcome {
    hello: bool,
    error: IpcError,
}

async fn consume(source: &SourceConfig, mut stream: OracleStream) -> SessionOutcome {
    let mut read_deadline = BOOTSTRAP_DEADLINE;
    let mut stale = false;
    let mut hello = false;
    loop {
        let frame = match read_frame_with_timeout(&mut stream, read_deadline).await {
            Ok(frame) => frame,
            Err(error) => return SessionOutcome { hello, error },
        };
        metrics::inc_frames(&source.name);
        metrics::set_last_frame(&source.name, now_unix());
        match frame {
            OracleFrame::Hello {
                protocol_version,
                provider_id,
                feeds,
                heartbeat_interval_sec,
            } => {
                if protocol_version != PROTOCOL_VERSION {
                    warn!(
                        source = %source.name,
                        theirs = protocol_version,
                        ours = PROTOCOL_VERSION,
                        "Oracle source protocol version mismatch, dropping connection"
                    );
                    return SessionOutcome {
                        hello,
                        error: IpcError::Closed,
                    };
                }
                hello = true;
                read_deadline = heartbeat_deadline(heartbeat_interval_sec);
                info!(
                    source = %source.name,
                    %provider_id,
                    feed_count = feeds.len(),
                    heartbeat_interval_sec,
                    "Oracle source handshake"
                );
            }
            OracleFrame::PriceUpdate {
                provider_id,
                feed_id,
                payload,
                ingested_at,
            } => {
                let age = now_unix().saturating_sub(ingested_at);
                trace!(
                    target: "oracle::frames",
                    source = %source.name,
                    %feed_id,
                    age_secs = age,
                    bytes = payload.len(),
                    "Received price update"
                );
                if age > STALENESS_WARN_SEC {
                    if !stale {
                        stale = true;
                        warn!(
                            source = %source.name,
                            %feed_id,
                            age_secs = age,
                            "Oracle source data is stale (payload older than threshold)"
                        );
                    }
                } else if stale {
                    stale = false;
                    info!(source = %source.name, "Oracle source data freshness recovered");
                }
                prices::insert(provider_id, feed_id, payload);
            }
            OracleFrame::Heartbeat { sent_at_unix } => {
                trace!(
                    target: "oracle::frames",
                    source = %source.name,
                    sent_at_unix,
                    "Received heartbeat"
                );
            }
        }
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

mod metrics {
    use std::sync::LazyLock;

    use prometheus_exporter::prometheus::{
        register_int_counter_vec, register_int_gauge_vec, IntCounterVec, IntGaugeVec,
    };

    static CONNECTED: LazyLock<IntGaugeVec> = LazyLock::new(|| {
        register_int_gauge_vec!(
            "oracle_source_connected",
            "1 if the oracle source connection is currently established, else 0",
            &["source"]
        )
        .expect("register connected gauge")
    });

    static RECONNECTS: LazyLock<IntCounterVec> = LazyLock::new(|| {
        register_int_counter_vec!(
            "oracle_reconnects_total",
            "Number of times an oracle source connection was re-established",
            &["source"]
        )
        .expect("register reconnects counter")
    });

    static FRAMES: LazyLock<IntCounterVec> = LazyLock::new(|| {
        register_int_counter_vec!(
            "oracle_frames_total",
            "Number of frames received from an oracle source",
            &["source"]
        )
        .expect("register frames counter")
    });

    static LAST_FRAME_TIMESTAMP: LazyLock<IntGaugeVec> = LazyLock::new(|| {
        register_int_gauge_vec!(
            "oracle_last_frame_timestamp_seconds",
            "Unix timestamp of the last frame received from an oracle source",
            &["source"]
        )
        .expect("register last-frame timestamp gauge")
    });

    pub fn set_connected(source: &str, connected: bool) {
        CONNECTED.with_label_values(&[source]).set(connected as i64);
    }

    pub fn inc_reconnects(source: &str) {
        RECONNECTS.with_label_values(&[source]).inc();
    }

    pub fn inc_frames(source: &str) {
        FRAMES.with_label_values(&[source]).inc();
    }

    pub fn set_last_frame(source: &str, unix_secs: u64) {
        LAST_FRAME_TIMESTAMP
            .with_label_values(&[source])
            .set(unix_secs as i64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_sources_defaults_to_false() {
        let config = toml::from_str::<OracleConfigFile>("[oracle]")
            .unwrap()
            .oracle;
        assert!(!config.require_sources);
    }

    #[test]
    fn transport_defaults_to_unix() {
        let toml = r#"
            [oracle]
            [[oracle.source]]
            name = "chainlink"
            socket_path = "/run/relay/chainlink.sock"
        "#;
        let config = toml::from_str::<OracleConfigFile>(toml).unwrap().oracle;
        assert_eq!(config.sources[0].transport, Transport::Unix);
    }

    fn source(toml: &str) -> SourceConfig {
        toml::from_str::<OracleConfigFile>(&format!("[oracle]\n[[oracle.source]]\n{toml}"))
            .unwrap()
            .oracle
            .sources
            .remove(0)
    }

    #[test]
    fn parses_full_config() {
        let toml = r#"
            [oracle]
            enabled = true
            require_sources = true
            [[oracle.source]]
            name = "chainlink"
            transport = "unix"
            socket_path = "/run/relay/chainlink.sock"
            [[oracle.source]]
            name = "pyth"
            transport = "tcp"
            socket_address = "127.0.0.1:9802"
        "#;
        let config = toml::from_str::<OracleConfigFile>(toml).unwrap().oracle;
        assert!(config.enabled);
        assert!(config.require_sources);
        assert_eq!(config.sources.len(), 2);
        assert_eq!(config.sources[0].name, "chainlink");
        assert_eq!(config.sources[0].transport, Transport::Unix);
        assert_eq!(config.sources[1].transport, Transport::Tcp);
    }

    #[test]
    fn unix_source_resolves_to_unix_endpoint() {
        let s = source("name = \"a\"\nsocket_path = \"/run/a.sock\"");
        assert_eq!(s.endpoint().unwrap(), Endpoint::unix("/run/a.sock"));
    }

    #[test]
    fn tcp_source_resolves_to_tcp_endpoint() {
        let s = source("name = \"a\"\ntransport = \"tcp\"\nsocket_address = \"127.0.0.1:9802\"");
        assert_eq!(s.endpoint().unwrap(), Endpoint::tcp("127.0.0.1:9802"));
    }

    #[test]
    fn unix_without_socket_path_is_an_error() {
        let s = source("name = \"a\"");
        assert!(s.endpoint().is_err());
    }

    #[test]
    fn tcp_without_socket_address_is_an_error() {
        let s = source("name = \"a\"\ntransport = \"tcp\"");
        assert!(s.endpoint().is_err());
    }

    #[test]
    fn tcp_with_stray_socket_path_is_an_error() {
        let s = source(
            "name = \"a\"\ntransport = \"tcp\"\nsocket_address = \"127.0.0.1:9802\"\nsocket_path = \"/run/a.sock\"",
        );
        assert!(s.endpoint().is_err());
    }

    #[test]
    fn unix_with_stray_socket_address_is_an_error() {
        let s = source(
            "name = \"a\"\nsocket_path = \"/run/a.sock\"\nsocket_address = \"127.0.0.1:9802\"",
        );
        assert!(s.endpoint().is_err());
    }

    #[test]
    fn applies_defaults() {
        let config = toml::from_str::<OracleConfigFile>("[oracle]")
            .unwrap()
            .oracle;
        assert!(!config.enabled);
        assert!(config.sources.is_empty());
    }

    #[test]
    fn resolve_derives_path() {
        let path = resolve_config_path(None, Path::new("configs/mock/rollup.toml"));
        assert_eq!(path, PathBuf::from("configs/mock/oracle.toml"));
    }

    #[test]
    fn resolve_uses_explicit_path() {
        let path = resolve_config_path(
            Some(PathBuf::from("/etc/relay/oracle.toml")),
            Path::new("configs/mock/rollup.toml"),
        );
        assert_eq!(path, PathBuf::from("/etc/relay/oracle.toml"));
    }

    #[test]
    fn missing_file_loads_as_none() {
        let result = load_config(Path::new("/nonexistent/oracle.toml")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn load_or_create_writes_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oracle.toml");

        let config = load_or_create_config(&path).unwrap();
        assert!(!config.enabled);
        assert!(config.sources.is_empty());
        assert!(path.exists());

        let reloaded = load_config(&path).unwrap().unwrap();
        assert!(!reloaded.enabled);
    }

    #[test]
    fn malformed_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oracle.toml");
        fs::write(&path, "this is not = valid toml [[[").unwrap();
        assert!(load_config(&path).is_err());
    }

    fn unix_src(name: &str, path: &str) -> SourceConfig {
        SourceConfig {
            name: name.to_string(),
            transport: Transport::Unix,
            socket_path: Some(PathBuf::from(path)),
            socket_address: None,
        }
    }

    #[test]
    fn duplicate_source_names_are_rejected() {
        let config = OracleConfig {
            sources: vec![unix_src("dup", "/a.sock"), unix_src("dup", "/b.sock")],
            ..Default::default()
        };
        assert!(validate(&config).is_err());
    }

    #[test]
    fn unique_source_names_pass_validation() {
        let config = OracleConfig {
            sources: vec![unix_src("a", "/a.sock"), unix_src("b", "/b.sock")],
            ..Default::default()
        };
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn diff_add_remove_restart() {
        let mut current = HashMap::new();
        current.insert("keep".to_string(), unix_src("keep", "/keep.sock"));
        current.insert("change".to_string(), unix_src("change", "/old.sock"));
        current.insert("drop".to_string(), unix_src("drop", "/drop.sock"));

        let new_sources = vec![
            unix_src("keep", "/keep.sock"),
            unix_src("change", "/new.sock"),
            unix_src("add", "/add.sock"),
        ];
        let diff = diff_sources(&current, new_sources);

        assert_eq!(
            diff.to_add
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            ["add"]
        );
        assert_eq!(
            diff.to_restart
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            ["change"]
        );
        assert_eq!(diff.to_remove, ["drop"]);
    }

    #[test]
    fn heartbeat_deadline_scales() {
        assert_eq!(
            heartbeat_deadline(10),
            Duration::from_secs(10 * u64::from(DEADLINE_HEARTBEAT_MULTIPLIER))
        );
    }

    #[test]
    fn heartbeat_deadline_zero_interval() {
        assert_eq!(heartbeat_deadline(0), BOOTSTRAP_DEADLINE);
    }

    #[tokio::test]
    async fn require_sources_ready() {
        let (tx, rx) = mpsc::unbounded_channel();
        tx.send("a".to_string()).unwrap();
        tx.send("b".to_string()).unwrap();
        let pending: HashSet<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        assert!(await_sources_ready(pending, rx, Duration::from_secs(5))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn require_sources_times_out() {
        let (tx, rx) = mpsc::unbounded_channel();
        tx.send("a".to_string()).unwrap();
        let pending: HashSet<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        let result = await_sources_ready(pending, rx, Duration::from_millis(50)).await;
        assert!(result.is_err());
        drop(tx);
    }
}
