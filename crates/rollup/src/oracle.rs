use std::collections::{HashMap, HashSet};
use std::fs;
use std::future;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context};
use price_oracle_ipc::{
    connect, read_frame_with_timeout, Backoff, IpcError, OracleFrame, OracleStream, B256,
    FEEDS_MAX, PROTOCOL_VERSION, READ_DEADLINE,
};
use serde::Deserialize;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, trace, warn};

use stf_starter::prices;

const ORACLE_CONFIG_FILE: &str = "oracle.toml";
const ORACLE_CONFIG_CONTENT: &str = "[oracle]\nenabled = false\n";
const SUPERVISOR_GUARD_MIN: Duration = Duration::from_secs(1);
const SUPERVISOR_GUARD_MAX: Duration = Duration::from_secs(30);
const HEALTHY_CONNECTION_THRESHOLD: Duration = Duration::from_secs(10);
const REQUIRE_SOURCES_TIMEOUT: Duration = Duration::from_secs(5);
const METRICS_REPORT_INTERVAL: Duration = Duration::from_secs(15);
const WARN_THROTTLE: Duration = Duration::from_secs(60);
const REPORT_TTL: Duration = Duration::from_secs(300);
const TTL_SWEEP_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Default)]
struct WarnThrottle {
    last: Option<Instant>,
}

impl WarnThrottle {
    fn allow(&mut self) -> bool {
        let now = Instant::now();
        if self
            .last
            .is_some_and(|prev| now.duration_since(prev) < WARN_THROTTLE)
        {
            return false;
        }
        self.last = Some(now);
        true
    }
}

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
    Tcp,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceConfig {
    pub name: String,
    pub provider_id: String,
    #[serde(default)]
    pub transport: Transport,
    #[serde(default)]
    pub socket_address: Option<String>,
}

impl SourceConfig {
    fn provider_hash(&self) -> B256 {
        alloy_primitives::keccak256(self.provider_id.as_bytes())
    }

    fn address(&self) -> anyhow::Result<String> {
        match self.transport {
            Transport::Tcp => self.socket_address.clone().ok_or_else(|| {
                anyhow!(
                    "oracle source '{}': transport = \"tcp\" requires socket_address",
                    self.name
                )
            }),
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
        source.address()?;
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
        tokio::spawn(ignore_sighup());
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

    tokio::spawn(metrics::run_reporter(METRICS_REPORT_INTERVAL));
    tokio::spawn(ttl_sweeper());
    tokio::spawn(reload_manager(path, startup_config, registry));
    Ok(())
}

async fn ttl_sweeper() {
    let mut ticker = tokio::time::interval(TTL_SWEEP_INTERVAL);
    ticker.tick().await;
    loop {
        ticker.tick().await;
        let evicted = prices::evict_expired(REPORT_TTL);
        if !evicted.is_empty() {
            metrics::add_evicted(evicted.len() as u64);
            warn!(
                count = evicted.len(),
                feeds = ?evicted,
                "Evicted oracle reports with no accepted update within the TTL"
            );
        }
    }
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
    let address = match source.address() {
        Ok(address) => address,
        Err(e) => {
            error!(source = %source.name, error = %e, "Skipping oracle source with invalid address");
            return None;
        }
    };
    info!(source = %source.name, address = %address, "Starting oracle source client");
    let (cancel_tx, cancel_rx) = watch::channel(());
    let join = tokio::spawn(supervise_source(source.clone(), address, ready, cancel_rx));
    Some(SourceHandle {
        config: source,
        cancel: cancel_tx,
        join,
    })
}

async fn ignore_sighup() {
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
            let evicted = prices::remove_source(&name);
            if evicted > 0 {
                info!(source = %name, evicted, "Evicted feeds for removed oracle source");
            }
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
    address: String,
    ready: Option<mpsc::UnboundedSender<String>>,
    mut cancel: watch::Receiver<()>,
) {
    let mut guard = Backoff::new(SUPERVISOR_GUARD_MIN, SUPERVISOR_GUARD_MAX);
    loop {
        let started = Instant::now();
        let mut child = tokio::spawn(run_source(source.clone(), address.clone(), ready.clone()));

        tokio::select! {
            biased;
            _ = cancel.changed() => {
                child.abort();
                let _ = child.await;
                metrics::handle(&source.name).set_connected(false);
                info!(source = %source.name, "Oracle source stopped (config reload)");
                return;
            }
            result = &mut child => match result {
                Ok(()) => {
                    warn!(source = %source.name, "Oracle client task exited unexpectedly, restarting");
                }
                Err(join_error) => {
                    metrics::handle(&source.name).set_connected(false);
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

async fn run_source(
    source: SourceConfig,
    address: String,
    mut ready: Option<mpsc::UnboundedSender<String>>,
) {
    let mut backoff = Backoff::default();
    let mut reconnect = false;
    let source_metrics = metrics::handle(&source.name);
    loop {
        let stream = match connect(&address).await {
            Ok(stream) => {
                if reconnect {
                    source_metrics.inc_reconnects();
                    info!(source = %source.name, "Connected to oracle source");
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

        source_metrics.set_connected(true);
        let started = Instant::now();
        let outcome = consume(&source, &source_metrics, stream, &mut ready).await;
        source_metrics.set_connected(false);

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

async fn consume(
    source: &SourceConfig,
    source_metrics: &metrics::SourceMetrics,
    mut stream: OracleStream,
    ready: &mut Option<mpsc::UnboundedSender<String>>,
) -> SessionOutcome {
    let mut session_provider: Option<B256> = None;
    let mut unadvertised_warn = WarnThrottle::default();
    let mut divergence_warn = WarnThrottle::default();
    let mut conflict_warn = WarnThrottle::default();
    loop {
        let frame = match read_frame_with_timeout(&mut stream, READ_DEADLINE).await {
            Ok(frame) => frame,
            Err(error) => {
                return SessionOutcome {
                    hello: session_provider.is_some(),
                    error,
                }
            }
        };
        source_metrics.inc_frames();
        source_metrics.set_last_frame(now_unix());
        if session_provider.is_none() && !matches!(frame, OracleFrame::Hello { .. }) {
            warn!(
                source = %source.name,
                "Oracle source sent a frame before Hello, dropping connection"
            );
            return SessionOutcome {
                hello: false,
                error: IpcError::Closed,
            };
        }
        match frame {
            OracleFrame::Hello {
                protocol_version,
                provider_id,
                feeds,
            } => {
                if protocol_version != PROTOCOL_VERSION {
                    warn!(
                        source = %source.name,
                        theirs = protocol_version,
                        ours = PROTOCOL_VERSION,
                        "Oracle source protocol version mismatch, dropping connection"
                    );
                    return SessionOutcome {
                        hello: session_provider.is_some(),
                        error: IpcError::Closed,
                    };
                }
                if provider_id != source.provider_hash() {
                    warn!(
                        source = %source.name,
                        expected = %source.provider_id,
                        advertised = %provider_id,
                        "Oracle source advertised an unexpected provider id, dropping connection"
                    );
                    return SessionOutcome {
                        hello: session_provider.is_some(),
                        error: IpcError::Closed,
                    };
                }
                let feed_count = feeds.len();
                if feed_count > FEEDS_MAX {
                    warn!(
                        source = %source.name,
                        feed_count,
                        max = FEEDS_MAX,
                        "Oracle source advertised too many feeds, dropping connection"
                    );
                    return SessionOutcome {
                        hello: session_provider.is_some(),
                        error: IpcError::Closed,
                    };
                }
                let registration = prices::register_feeds(&source.name, provider_id, feeds);
                let conflicts = &registration.feed_set_conflicts;
                if !conflicts.is_empty() {
                    source_metrics.inc_feed_set_conflicts();
                    if divergence_warn.allow() {
                        warn!(
                            source = %source.name,
                            %provider_id,
                            conflicting = conflicts.len(),
                            sample = ?&conflicts[..conflicts.len().min(5)],
                            "Oracle provider replicas advertise conflicting feed sets"
                        );
                    }
                }
                session_provider = Some(provider_id);
                info!(
                    source = %source.name,
                    %provider_id,
                    feed_count,
                    evicted = registration.evicted,
                    "Oracle source handshake"
                );
                if let Some(ready) = ready.take() {
                    let _ = ready.send(source.name.clone());
                }
            }
            OracleFrame::PriceUpdate {
                feed_id,
                payload,
                source_time_ms,
            } => {
                let Some(provider_id) = session_provider else {
                    continue;
                };
                trace!(
                    target: "oracle::frames",
                    source = %source.name,
                    %feed_id,
                    source_time_ms,
                    bytes = payload.len(),
                    "Received price update"
                );
                match prices::insert_if_newer(provider_id, feed_id, payload, source_time_ms) {
                    prices::InsertOutcome::Inserted => source_metrics.inc_inserted(),
                    prices::InsertOutcome::Duplicate => source_metrics.inc_duplicates(),
                    prices::InsertOutcome::Conflict => {
                        source_metrics.inc_payload_conflicts();
                        if conflict_warn.allow() {
                            warn!(
                                source = %source.name,
                                %feed_id,
                                source_time_ms,
                                "Oracle replicas sent conflicting payloads for the same timestamp"
                            );
                        }
                    }
                    prices::InsertOutcome::Stale => source_metrics.inc_stale(),
                    prices::InsertOutcome::Unexpected => {
                        source_metrics.inc_unadvertised();
                        if unadvertised_warn.allow() {
                            warn!(
                                source = %source.name,
                                %feed_id,
                                "Oracle source sent an update for an unexpected feed, dropping"
                            );
                        }
                    }
                }
            }
            OracleFrame::Heartbeat => {
                trace!(
                    target: "oracle::frames",
                    source = %source.name,
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
    use std::collections::BTreeMap;
    use std::io::{self, Write};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, LazyLock};
    use std::time::Duration;

    use parking_lot::Mutex;

    use sov_metrics::Metric;
    use tracing::info;

    #[derive(Default)]
    pub struct SourceMetrics {
        connected: AtomicBool,
        reconnects: AtomicU64,
        frames: AtomicU64,
        last_frame_unix: AtomicU64,
        inserted: AtomicU64,
        duplicates: AtomicU64,
        payload_conflicts: AtomicU64,
        stale: AtomicU64,
        unadvertised: AtomicU64,
        feed_set_conflicts: AtomicU64,
    }

    impl SourceMetrics {
        pub fn set_connected(&self, connected: bool) {
            self.connected.store(connected, Ordering::Relaxed);
        }

        pub fn inc_reconnects(&self) {
            self.reconnects.fetch_add(1, Ordering::Relaxed);
        }

        pub fn inc_frames(&self) {
            self.frames.fetch_add(1, Ordering::Relaxed);
        }

        pub fn set_last_frame(&self, unix_secs: u64) {
            self.last_frame_unix.store(unix_secs, Ordering::Relaxed);
        }

        pub fn inc_inserted(&self) {
            self.inserted.fetch_add(1, Ordering::Relaxed);
        }

        pub fn inc_duplicates(&self) {
            self.duplicates.fetch_add(1, Ordering::Relaxed);
        }

        pub fn inc_payload_conflicts(&self) {
            self.payload_conflicts.fetch_add(1, Ordering::Relaxed);
        }

        pub fn inc_stale(&self) {
            self.stale.fetch_add(1, Ordering::Relaxed);
        }

        pub fn inc_unadvertised(&self) {
            self.unadvertised.fetch_add(1, Ordering::Relaxed);
        }

        pub fn inc_feed_set_conflicts(&self) {
            self.feed_set_conflicts.fetch_add(1, Ordering::Relaxed);
        }
    }

    static SOURCES: LazyLock<Mutex<BTreeMap<String, Arc<SourceMetrics>>>> =
        LazyLock::new(|| Mutex::new(BTreeMap::new()));

    pub fn handle(name: &str) -> Arc<SourceMetrics> {
        SOURCES.lock().entry(name.to_owned()).or_default().clone()
    }

    // One InfluxDB measurement per source, tagged by source name.
    #[derive(Debug)]
    struct OracleMetric {
        source: String,
        connected: bool,
        reconnects: u64,
        frames: u64,
        last_frame_unix: u64,
        inserted: u64,
        duplicates: u64,
        payload_conflicts: u64,
        stale: u64,
        unadvertised: u64,
        feed_set_conflicts: u64,
    }

    impl Metric for OracleMetric {
        fn measurement_name(&self) -> &'static str {
            "oracle"
        }

        fn serialize_for_telegraf(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
            write!(
                buffer,
                "{},source={} connected={},reconnects={},frames={},last_frame={},inserted={},duplicates={},payload_conflicts={},stale={},unadvertised={},feed_set_conflicts={}",
                self.measurement_name(),
                sov_metrics::safe_telegraf_string(&self.source),
                self.connected as u8,
                self.reconnects,
                self.frames,
                self.last_frame_unix,
                self.inserted,
                self.duplicates,
                self.payload_conflicts,
                self.stale,
                self.unadvertised,
                self.feed_set_conflicts,
            )
        }
    }

    static EVICTED: AtomicU64 = AtomicU64::new(0);

    pub fn add_evicted(count: u64) {
        EVICTED.fetch_add(count, Ordering::Relaxed);
    }

    #[derive(Debug)]
    struct StoreMetric {
        evicted: u64,
    }

    impl Metric for StoreMetric {
        fn measurement_name(&self) -> &'static str {
            "oracle_store"
        }

        fn serialize_for_telegraf(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
            write!(
                buffer,
                "{} evicted={}",
                self.measurement_name(),
                self.evicted,
            )
        }
    }

    /// Submits a snapshot of every source's counters to sov-metrics and logs it.
    pub async fn run_reporter(interval: Duration) {
        let mut ticker = tokio::time::interval(interval);
        ticker.tick().await;
        loop {
            ticker.tick().await;
            report_once();
        }
    }

    fn report_once() {
        let snapshot: Vec<OracleMetric> = {
            let sources = SOURCES.lock();
            sources
                .iter()
                .map(|(name, metrics)| OracleMetric {
                    source: name.clone(),
                    connected: metrics.connected.load(Ordering::Relaxed),
                    reconnects: metrics.reconnects.load(Ordering::Relaxed),
                    frames: metrics.frames.load(Ordering::Relaxed),
                    last_frame_unix: metrics.last_frame_unix.load(Ordering::Relaxed),
                    inserted: metrics.inserted.load(Ordering::Relaxed),
                    duplicates: metrics.duplicates.load(Ordering::Relaxed),
                    payload_conflicts: metrics.payload_conflicts.load(Ordering::Relaxed),
                    stale: metrics.stale.load(Ordering::Relaxed),
                    unadvertised: metrics.unadvertised.load(Ordering::Relaxed),
                    feed_set_conflicts: metrics.feed_set_conflicts.load(Ordering::Relaxed),
                })
                .collect()
        };

        for metric in snapshot {
            info!(
                source = %metric.source,
                connected = metric.connected,
                reconnects = metric.reconnects,
                frames = metric.frames,
                last_frame = metric.last_frame_unix,
                inserted = metric.inserted,
                duplicates = metric.duplicates,
                payload_conflicts = metric.payload_conflicts,
                stale = metric.stale,
                unadvertised = metric.unadvertised,
                feed_set_conflicts = metric.feed_set_conflicts,
                "oracle source metrics"
            );
            sov_metrics::track_metrics(|tracker| tracker.submit(metric));
        }

        let store = StoreMetric {
            evicted: EVICTED.load(Ordering::Relaxed),
        };
        info!(evicted = store.evicted, "oracle store metrics");
        sov_metrics::track_metrics(|tracker| tracker.submit(store));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROVIDER_ID: &str = "chainlink";

    #[test]
    fn require_sources_defaults_to_false() {
        let config = toml::from_str::<OracleConfigFile>("[oracle]")
            .unwrap()
            .oracle;
        assert!(!config.require_sources);
    }

    #[test]
    fn transport_defaults_to_tcp() {
        let toml = format!(
            r#"
            [oracle]
            [[oracle.source]]
            name = "chainlink"
            provider_id = "{PROVIDER_ID}"
            socket_address = "127.0.0.1:9801"
        "#
        );
        let config = toml::from_str::<OracleConfigFile>(&toml).unwrap().oracle;
        assert_eq!(config.sources[0].transport, Transport::Tcp);
    }

    fn source(toml: &str) -> SourceConfig {
        toml::from_str::<OracleConfigFile>(&format!(
            "[oracle]\n[[oracle.source]]\nprovider_id = \"{PROVIDER_ID}\"\n{toml}"
        ))
        .unwrap()
        .oracle
        .sources
        .remove(0)
    }

    #[test]
    fn parses_full_config() {
        let toml = format!(
            r#"
            [oracle]
            enabled = true
            require_sources = true
            [[oracle.source]]
            name = "chainlink-1"
            provider_id = "{PROVIDER_ID}"
            transport = "tcp"
            socket_address = "127.0.0.1:9801"
            [[oracle.source]]
            name = "chainlink-2"
            provider_id = "{PROVIDER_ID}"
            socket_address = "127.0.0.1:9802"
        "#
        );
        let config = toml::from_str::<OracleConfigFile>(&toml).unwrap().oracle;
        assert!(config.enabled);
        assert!(config.require_sources);
        assert_eq!(config.sources.len(), 2);
        assert_eq!(config.sources[0].name, "chainlink-1");
        assert_eq!(config.sources[0].provider_id, PROVIDER_ID);
        assert_eq!(
            config.sources[0].provider_hash(),
            alloy_primitives::keccak256(PROVIDER_ID)
        );
        assert_eq!(config.sources[0].transport, Transport::Tcp);
        assert_eq!(config.sources[1].transport, Transport::Tcp);
    }

    #[test]
    fn missing_provider_id_is_an_error() {
        let toml = r#"
            [oracle]
            [[oracle.source]]
            name = "chainlink"
            socket_address = "127.0.0.1:9801"
        "#;
        assert!(toml::from_str::<OracleConfigFile>(toml).is_err());
    }

    #[test]
    fn tcp_source_resolves_to_address() {
        let s = source("name = \"a\"\ntransport = \"tcp\"\nsocket_address = \"127.0.0.1:9802\"");
        assert_eq!(s.address().unwrap(), "127.0.0.1:9802");
    }

    #[test]
    fn default_transport_resolves_to_address() {
        let s = source("name = \"a\"\nsocket_address = \"127.0.0.1:9802\"");
        assert_eq!(s.address().unwrap(), "127.0.0.1:9802");
    }

    #[test]
    fn tcp_without_socket_address_is_an_error() {
        let s = source("name = \"a\"\ntransport = \"tcp\"");
        assert!(s.address().is_err());
    }

    #[test]
    fn default_transport_without_address_is_an_error() {
        let s = source("name = \"a\"");
        assert!(s.address().is_err());
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

    fn tcp_src(name: &str, address: &str) -> SourceConfig {
        SourceConfig {
            name: name.to_string(),
            provider_id: PROVIDER_ID.to_string(),
            transport: Transport::Tcp,
            socket_address: Some(address.to_string()),
        }
    }

    #[test]
    fn duplicate_source_names_are_rejected() {
        let config = OracleConfig {
            sources: vec![tcp_src("dup", "127.0.0.1:1"), tcp_src("dup", "127.0.0.1:2")],
            ..Default::default()
        };
        assert!(validate(&config).is_err());
    }

    #[test]
    fn unique_source_names_pass_validation() {
        let config = OracleConfig {
            sources: vec![tcp_src("a", "127.0.0.1:1"), tcp_src("b", "127.0.0.1:2")],
            ..Default::default()
        };
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn diff_add_remove_restart() {
        let mut current = HashMap::new();
        current.insert("keep".to_string(), tcp_src("keep", "127.0.0.1:1"));
        current.insert("change".to_string(), tcp_src("change", "127.0.0.1:2"));
        current.insert("drop".to_string(), tcp_src("drop", "127.0.0.1:3"));

        let new_sources = vec![
            tcp_src("keep", "127.0.0.1:1"),
            tcp_src("change", "127.0.0.1:9"),
            tcp_src("add", "127.0.0.1:4"),
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
