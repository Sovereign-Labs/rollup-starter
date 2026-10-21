use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use anyhow::{anyhow, Context};
use sov_rollup_manager::{ManagerConfig, RollupVersion};
use sov_soak_manager::{SoakManagerConfig, SoakWorkerConfig};
use sov_versioned_artifact_builder::{
    prepare_artifacts, BuildRequest, BuildSpec, BuildTarget, BuildTargets, RollupBuilder,
    VersionBuildSpec,
};
use tokio::process::Command as TokioCommand;
use tracing::info;

use crate::{Directories, ManagedRollupProcess};

pub const ROLLUP_REPO_URL: &str = "https://github.com/Sovereign-Labs/rollup-starter.git";
pub const VERSION_SPEC_FILE: &str = "versions.yaml";
pub const VERSION_VARS_COMMIT_KEY: &str = "rollup_commit_hash";
pub const VERSION_CONFIG_TEMPLATE_PATH: &str = "scripts/acceptance-test/rollup_config.toml";
pub const SOAK_NUM_WORKERS: u32 = 20;
pub const SOAK_SALT: u32 = 3; // existing acceptance-test-data started from 3 for some reason
pub const SOAK_SAFETY_STOP_BLOCKS: u64 = 5;
const ACCEPTANCE_TEST_FEATURES: [&str; 3] = ["acceptance-testing", "mock_da", "mock_zkvm"];
const DB_MIGRATION_FEATURE: &str = "sov-migrations";
const ACCEPTANCE_CONSTANTS_FILENAME: &str = "constants.testing.toml";
/// Repo-relative directory whose `constants.testing.toml` all versioned builds must use.
const ACCEPTANCE_CONSTANTS_REPO_DIR: &str = "scripts/acceptance-test";

fn acceptance_test_features() -> Vec<String> {
    ACCEPTANCE_TEST_FEATURES
        .iter()
        .map(|feature| feature.to_string())
        .collect()
}

/// Features for the `rollup-db-migration` binary: the migration must be built with the same
/// runtime-shaping features and constants as the rollup binary it migrates for, plus the
/// `sov-migrations` feature that gates the binary itself.
fn db_migration_features() -> Vec<String> {
    let mut features = acceptance_test_features();
    features.push(DB_MIGRATION_FEATURE.to_string());
    features
}

#[derive(Debug, Clone)]
enum VersionSource {
    RemoteCommit(String),
    LocalHead,
}

#[derive(Debug, Clone)]
struct ResolvedVersion {
    source: VersionSource,
    migration_path: Option<PathBuf>,
    /// Build this version's `rollup-db-migration` binary and have the manager run it (with
    /// `--rollup-config-path` of this version) before the version starts.
    build_db_migration: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct VersionSpecRoot {
    #[serde(default)]
    rollup_versions: Vec<VersionSpecEntry>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct VersionSpecEntry {
    version_id: String,
    vars_file: PathBuf,
    migration_path: Option<PathBuf>,
    #[serde(default)]
    build_db_migration: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct VersionVarsFile {
    rollup_commit_hash: String,
}

#[derive(Debug, Clone)]
pub struct AcceptanceRunPlan {
    pub manager_binary: PathBuf,
    pub manager_versions: Vec<RollupVersion>,
    pub soak_config: SoakManagerConfig,
    /// Where the recorded acceptance data ends, if pre-existing snapshots were found.
    pub recorded_data: Option<RecordedDataBounds>,
    /// The per-version block span the plan was built with.
    pub blocks_per_version: u64,
}

impl AcceptanceRunPlan {
    /// The highest batch *number* the recorded data is expected to contain (batch numbers lag
    /// the rollup height by one, since genesis carries no batch). Falls back to the first
    /// version's span when no recorded data exists.
    pub fn expected_setup_batches(&self) -> u64 {
        match self.recorded_data {
            Some(bounds) => bounds.end_rollup_height,
            None => self.blocks_per_version,
        }
        .saturating_sub(1)
    }

    /// The highest recorded DA slot number, when pre-existing recorded data is present.
    pub fn last_recorded_slot(&self) -> Option<u64> {
        self.recorded_data.map(|bounds| bounds.last_slot_number)
    }

    /// Cumulative batch counts at which a version handover (and its db migration) occurs:
    /// `k * blocks_per_version` for each non-last version `k`.
    pub fn migration_boundary_batch_counts(&self) -> Vec<u64> {
        (1..self.manager_versions.len() as u64)
            .map(|k| k * self.blocks_per_version)
            .collect()
    }
}

/// Bounds of the pre-existing recorded acceptance data, derived from the saved snapshots.
///
/// Rollup heights are exact at generation (each version stops precisely at its stop height, a
/// `blocks_per_version` multiple), but DA *slot* numbers run ahead of rollup heights by a
/// data-dependent amount (slots that carry no batches: genesis warmup slots, and the empty gap
/// slots a version handover leaves behind). E.g. the original data holds 1000 batches spanning
/// DA slots 3..=1002, executing at rollup heights 1..=1000. Anything slot-based (when a resync
/// has replayed all recorded data; where fresh generation begins) must therefore come from
/// these data-derived bounds, never from height arithmetic.
#[derive(Debug, Clone, Copy)]
pub struct RecordedDataBounds {
    /// The highest recorded DA slot number (the last slot with a saved snapshot).
    pub last_slot_number: u64,
    /// The rollup height at which the recorded data ends. Rollup height N applies ledger
    /// batch N-1, so this equals the last snapshot's `batch_range.end`.
    pub end_rollup_height: u64,
}

/// Scans `snapshots_dir` for saved slot snapshots and derives [`RecordedDataBounds`] from
/// the highest-numbered one. Returns `Ok(None)` when no snapshots exist (e.g. during data
/// generation from scratch).
pub fn recorded_data_bounds(snapshots_dir: &Path) -> anyhow::Result<Option<RecordedDataBounds>> {
    if !snapshots_dir.is_dir() {
        return Ok(None);
    }
    let mut last_slot_number: Option<u64> = None;
    for entry in fs::read_dir(snapshots_dir)
        .with_context(|| format!("failed to read snapshots dir {}", snapshots_dir.display()))?
    {
        let name = entry?.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(number) = name
            .strip_prefix("slot_")
            .and_then(|rest| rest.strip_suffix("_with_children.json"))
            .and_then(|digits| digits.parse::<u64>().ok())
        else {
            continue;
        };
        last_slot_number = Some(last_slot_number.map_or(number, |max| max.max(number)));
    }
    let Some(last_slot_number) = last_slot_number else {
        return Ok(None);
    };

    let snapshot = crate::fetch_and_compare::load_snapshot_json(last_slot_number, snapshots_dir)
        .with_context(|| format!("failed to load final snapshot for slot {last_slot_number}"))?;
    let end_rollup_height = snapshot
        .get("batch_range")
        .and_then(|range| range.get("end"))
        .and_then(|end| end.as_u64())
        .ok_or_else(|| {
            anyhow!("final snapshot for slot {last_slot_number} has no numeric batch_range.end")
        })?;

    Ok(Some(RecordedDataBounds {
        last_slot_number,
        end_rollup_height,
    }))
}

fn default_build_targets() -> BuildTargets {
    let mut targets = BuildTargets::upgrade_simulator_defaults();
    // The soak binary signs transactions using Runtime::CHAIN_HASH, so it must be built with the
    // exact same runtime-shaping features as the rollup binary. Both must also be compiled
    // against the acceptance-test constants manifest (like the local-HEAD builds are): among
    // other things it carries the CHAIN_HASH_OVERRIDES without which the historical DA
    // transactions fail signature verification on replay.
    let constants_dir = Some(PathBuf::from(ACCEPTANCE_CONSTANTS_REPO_DIR));
    targets.rollup.features = acceptance_test_features();
    targets.rollup.test_constants_manifest_dir = constants_dir.clone();
    if let Some(soak) = targets.soak.as_mut() {
        soak.no_default_features = true;
        soak.features = acceptance_test_features();
        soak.test_constants_manifest_dir = constants_dir.clone();
    }
    if let Some(db_migration) = targets.db_migration.as_mut() {
        db_migration.no_default_features = true;
        db_migration.features = db_migration_features();
        db_migration.test_constants_manifest_dir = constants_dir;
    }
    targets.mock_da = None;
    targets
}

fn run_checked(cmd: &mut StdCommand, context: &str) -> Result<(), anyhow::Error> {
    let output = cmd.output().with_context(|| format!("{context}: spawn"))?;
    if output.status.success() {
        return Ok(());
    }

    Err(anyhow!(
        "{context} failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

fn load_version_sources(directories: &Directories) -> anyhow::Result<Vec<ResolvedVersion>> {
    // The spec lives in the acceptance-test directory: it describes the versions of the
    // recorded acceptance data, which is a property of this test setup — not of the rollup
    // repo itself (a root-level versions.yaml is the resync tooling's convention on customer
    // branches, describing the deployed rollup's actual versions).
    let spec_path = directories.acceptance_test_dir.join(VERSION_SPEC_FILE);
    if !spec_path.exists() {
        info!(
            path = %spec_path.display(),
            "No versions spec found, defaulting to local HEAD only"
        );
        return Ok(vec![ResolvedVersion {
            source: VersionSource::LocalHead,
            migration_path: None,
            build_db_migration: false,
        }]);
    }

    let spec_contents = fs::read_to_string(&spec_path)
        .with_context(|| format!("failed to read versions spec {}", spec_path.display()))?;
    let spec: VersionSpecRoot = serde_yaml::from_str(&spec_contents)
        .with_context(|| format!("failed to parse versions spec {}", spec_path.display()))?;
    let spec_dir = spec_path
        .parent()
        .ok_or_else(|| anyhow!("versions spec has no parent path"))?;

    let mut versions = Vec::with_capacity(spec.rollup_versions.len().max(1));
    for entry in &spec.rollup_versions {
        let vars_path = if entry.vars_file.is_absolute() {
            entry.vars_file.clone()
        } else {
            spec_dir.join(&entry.vars_file)
        };
        let vars_contents = fs::read_to_string(&vars_path).with_context(|| {
            format!(
                "failed to read vars file for version {} at {}",
                entry.version_id,
                vars_path.display()
            )
        })?;
        let vars: VersionVarsFile = serde_yaml::from_str(&vars_contents).with_context(|| {
            format!(
                "failed to parse vars file for version {} at {}",
                entry.version_id,
                vars_path.display()
            )
        })?;

        if vars.rollup_commit_hash.trim().is_empty() {
            return Err(anyhow!(
                "vars file {} for version {} is missing non-empty {}",
                vars_path.display(),
                entry.version_id,
                VERSION_VARS_COMMIT_KEY
            ));
        }

        if entry.build_db_migration && entry.migration_path.is_some() {
            return Err(anyhow!(
                "version {} sets both migration_path and build_db_migration; \
                 use migration_path for a prebuilt binary or build_db_migration \
                 to build the version's own rollup-db-migration target",
                entry.version_id
            ));
        }

        versions.push(ResolvedVersion {
            source: VersionSource::RemoteCommit(vars.rollup_commit_hash),
            migration_path: entry.migration_path.clone(),
            build_db_migration: entry.build_db_migration,
        });
    }

    if versions.is_empty() {
        versions.push(ResolvedVersion {
            source: VersionSource::LocalHead,
            migration_path: None,
            build_db_migration: false,
        });
    } else if let Some(last) = versions.last_mut() {
        last.source = VersionSource::LocalHead;
    }

    Ok(versions)
}

/// Build one cargo target at the local HEAD. The target's constants manifest and dedicated
/// cargo target dir are honored via [`BuildTarget::apply_build_env`] — the exact same
/// environment the versioned artifact builder applies to remote-commit builds, so local and
/// historical binaries are always built identically.
fn build_local_target(rollup_root: &Path, target: &BuildTarget) -> Result<PathBuf, anyhow::Error> {
    tracing::info!(bin = %target.bin, "Building binary at local HEAD...");
    let mut cmd = StdCommand::new("cargo");
    cmd.current_dir(rollup_root);
    cmd.args(["build", "--release"]);
    if let Some(package) = &target.package {
        cmd.args(["--package", package]);
    }
    cmd.args(["--bin", &target.bin]);
    if target.no_default_features {
        cmd.arg("--no-default-features");
    }
    if !target.features.is_empty() {
        cmd.args(["--features", &target.features.join(",")]);
    }
    target.apply_build_env(&mut cmd, rollup_root);
    run_checked(&mut cmd, &format!("build local head {} binary", target.bin))?;

    let binary = target
        .cargo_target_dir(rollup_root)
        .join("release")
        .join(&target.bin);
    if !binary.exists() {
        return Err(anyhow!(
            "local {} binary not found at {}",
            target.bin,
            binary.display()
        ));
    }
    binary.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize local {} binary {}",
            target.bin,
            binary.display()
        )
    })
}

fn build_local_head_binaries(
    directories: &Directories,
    targets: &BuildTargets,
    build_db_migration: bool,
) -> Result<(PathBuf, PathBuf, Option<PathBuf>), anyhow::Error> {
    let constants_path = directories
        .acceptance_test_dir
        .join(ACCEPTANCE_CONSTANTS_FILENAME);
    if !constants_path.is_file() {
        return Err(anyhow!(
            "acceptance-test constants manifest not found at {}",
            constants_path.display()
        ));
    }

    let rollup_root = &directories.rollup_root;
    let rollup_bin = build_local_target(rollup_root, &targets.rollup)?;
    let soak_target = targets
        .soak
        .as_ref()
        .ok_or_else(|| anyhow!("acceptance build targets must include a soak target"))?;
    let soak_bin = build_local_target(rollup_root, soak_target)?;
    let migration_bin = if build_db_migration {
        let migration_target = targets.db_migration.as_ref().ok_or_else(|| {
            anyhow!("acceptance build targets must include a db migration target")
        })?;
        Some(build_local_target(rollup_root, migration_target)?)
    } else {
        None
    };

    Ok((rollup_bin, soak_bin, migration_bin))
}

fn render_config_template(
    config_content: &str,
    password: &str,
    directories: &Directories,
) -> String {
    let sqlite_path = directories.output_dir.join("mock_da.sqlite");
    let sqlite_connection_string = format!("sqlite://{}?mode=rwc", sqlite_path.display());

    config_content
        .replace("{password}", password)
        .replace("{sqlite_connection_string}", &sqlite_connection_string)
        .replace(
            "{rollup_data_path}",
            &directories.rollup_data_path.display().to_string(),
        )
}

/// Prepares the versioned run plan: per-version binaries (built against the acceptance-test
/// constants manifest, locally for the last version and from pinned commits for the rest),
/// rendered configs, version heights, and the soak configuration.
///
/// `recorded_data` carries the bounds of pre-existing recorded data when the run will replay
/// it (pass `None` when generating from scratch, so stale snapshots can't influence the plan).
pub fn prepare_acceptance_run_plan(
    directories: &Directories,
    password: &str,
    blocks_per_version: u64,
    recorded_data: Option<RecordedDataBounds>,
) -> Result<AcceptanceRunPlan, anyhow::Error> {
    let binary_cache_dir = &directories.rollup_build_cache_dir;
    fs::create_dir_all(binary_cache_dir).with_context(|| {
        format!(
            "failed to create rollup binary cache directory {}",
            binary_cache_dir.display()
        )
    })?;

    let resolved_versions = load_version_sources(directories)?;
    let remote_commits: Vec<(String, bool)> = resolved_versions
        .iter()
        .filter_map(|version| match &version.source {
            VersionSource::RemoteCommit(commit) => {
                Some((commit.clone(), version.build_db_migration))
            }
            VersionSource::LocalHead => None,
        })
        .collect();

    // The same target set drives both the remote-commit and local-HEAD builds, so every
    // version's binaries are built with identical features and constants manifest.
    let build_targets = default_build_targets();

    let (mut remote_artifacts, template_reader) = if remote_commits.is_empty() {
        (None, None)
    } else {
        let build_spec = BuildSpec {
            repo_url: Some(ROLLUP_REPO_URL.to_string()),
            targets: build_targets.clone(),
            versions: remote_commits
                .iter()
                .map(|(commit, build_db_migration)| VersionBuildSpec {
                    commit: commit.clone(),
                    build_soak: true,
                    build_db_migration: *build_db_migration,
                })
                .collect(),
        };
        let build_request = BuildRequest {
            cache_dir: binary_cache_dir.to_path_buf(),
            build_soak_binaries: true,
            build_mock_da_binary: false,
        };
        let prepared_artifacts = prepare_artifacts(&build_spec, &build_request)?;
        (
            Some(prepared_artifacts.versions.into_iter()),
            Some(RollupBuilder::with_repo_url(
                binary_cache_dir.to_path_buf(),
                ROLLUP_REPO_URL.to_string(),
            )),
        )
    };

    let local_head_builds_db_migration = resolved_versions.iter().any(|version| {
        matches!(version.source, VersionSource::LocalHead) && version.build_db_migration
    });
    let (local_rollup_bin, local_soak_bin, local_migration_bin) =
        build_local_head_binaries(directories, &build_targets, local_head_builds_db_migration)?;

    let versioned_configs_dir = directories.output_dir.join("versioned-configs");
    fs::create_dir_all(&versioned_configs_dir).with_context(|| {
        format!(
            "failed to create versioned config directory {}",
            versioned_configs_dir.display()
        )
    })?;

    let mut manager_versions = Vec::with_capacity(resolved_versions.len());
    let mut soak_versions = Vec::with_capacity(resolved_versions.len());

    // Version boundaries are exact `blocks_per_version` multiples in rollup-height space:
    // generation stops each version precisely at its stop height (only DA *slot* numbers are
    // fuzzy, e.g. due to warmup slots that carry no batches). When pre-existing recorded data
    // is present, validate that it ends exactly on a version boundary and covers all non-last
    // versions: it may extend into the last version's range (the state right after appending
    // that version's data), but never beyond it.
    if let Some(bounds) = recorded_data {
        info!(
            last_slot_number = bounds.last_slot_number,
            end_rollup_height = bounds.end_rollup_height,
            "Derived recorded data bounds from saved snapshots"
        );
        let num_versions = resolved_versions.len() as u64;
        let end = bounds.end_rollup_height;
        let is_version_boundary = end % blocks_per_version == 0;
        let covered_versions = end / blocks_per_version;
        if !is_version_boundary
            || covered_versions < num_versions.saturating_sub(1)
            || covered_versions > num_versions
        {
            return Err(anyhow!(
                "recorded data ends at rollup height {end}, which does not match the version \
                 spec ({num_versions} version(s) at {blocks_per_version} blocks each): the data \
                 must end exactly on a version boundary and cover all non-last versions. The \
                 acceptance data and versions.yaml are out of sync; regenerate the data or fix \
                 the spec."
            ));
        }
    }

    for (idx, resolved_version) in resolved_versions.iter().enumerate() {
        let stop_height = ((idx as u64) + 1) * blocks_per_version;
        let start_height = if idx == 0 {
            None
        } else {
            Some((idx as u64) * blocks_per_version + 1)
        };

        let (rollup_binary, soak_binary, config_template_content, migration_path) =
            match &resolved_version.source {
                VersionSource::RemoteCommit(commit) => {
                    let artifacts = remote_artifacts
                        .as_mut()
                        .and_then(|iter| iter.next())
                        .ok_or_else(|| {
                            anyhow!("missing prepared artifacts for remote commit {}", commit)
                        })?;
                    let template_reader = template_reader.as_ref().ok_or_else(|| {
                        anyhow!("missing template reader for remote commit {}", commit)
                    })?;
                    let soak_binary = artifacts.soak_binary.ok_or_else(|| {
                        anyhow!("missing soak binary artifact for remote commit {}", commit)
                    })?;

                    let config_template = template_reader.read_text_file_at_commit(
                        commit,
                        Path::new(VERSION_CONFIG_TEMPLATE_PATH),
                    )?;

                    let migration_path = if resolved_version.build_db_migration {
                        let db_migration_binary =
                            artifacts.db_migration_binary.ok_or_else(|| {
                                anyhow!(
                                    "missing db migration binary artifact for remote commit {}",
                                    commit
                                )
                            })?;
                        Some(db_migration_binary.canonicalize().with_context(|| {
                            format!(
                                "failed to canonicalize db migration binary artifact {} for remote commit {}",
                                db_migration_binary.display(),
                                commit
                            )
                        })?)
                    } else if let Some(path) = &resolved_version.migration_path {
                        // Relative migration paths resolve against the spec's own directory,
                        // like `vars_file` does.
                        let migration_path = if path.is_absolute() {
                            path.clone()
                        } else {
                            directories.acceptance_test_dir.join(path)
                        };
                        Some(migration_path.canonicalize().with_context(|| {
                            format!(
                                "failed to canonicalize migration path {} for remote commit {}",
                                migration_path.display(),
                                commit
                            )
                        })?)
                    } else {
                        None
                    };

                    (
                        artifacts.rollup_binary.canonicalize().with_context(|| {
                            format!(
                                "failed to canonicalize rollup binary artifact {} for remote commit {}",
                                artifacts.rollup_binary.display(),
                                commit
                            )
                        })?,
                        soak_binary.canonicalize().with_context(|| {
                            format!(
                                "failed to canonicalize soak binary artifact {} for remote commit {}",
                                soak_binary.display(),
                                commit
                            )
                        })?,
                        config_template,
                        migration_path,
                    )
                }
                VersionSource::LocalHead => {
                    let migration_path = if resolved_version.build_db_migration {
                        Some(local_migration_bin.clone().ok_or_else(|| {
                            anyhow!("local head db migration binary was requested but not built")
                        })?)
                    } else if let Some(path) = &resolved_version.migration_path {
                        // Relative migration paths resolve against the spec's own directory,
                        // like `vars_file` does.
                        let migration_path = if path.is_absolute() {
                            path.clone()
                        } else {
                            directories.acceptance_test_dir.join(path)
                        };
                        Some(migration_path.canonicalize().with_context(|| {
                            format!(
                                "failed to canonicalize local migration path {}",
                                migration_path.display()
                            )
                        })?)
                    } else {
                        None
                    };

                    let config_template_path =
                        directories.acceptance_test_dir.join("rollup_config.toml");
                    (
                        local_rollup_bin.clone(),
                        local_soak_bin.clone(),
                        fs::read_to_string(&config_template_path).with_context(|| {
                            format!(
                                "failed to read local rollup config template {}",
                                config_template_path.display()
                            )
                        })?,
                        migration_path,
                    )
                }
            };

        let interpolated = render_config_template(&config_template_content, password, directories);
        let config_path = versioned_configs_dir.join(format!("config_{}.toml", idx));
        fs::write(&config_path, interpolated).with_context(|| {
            format!(
                "failed to write rendered versioned rollup config {}",
                config_path.display()
            )
        })?;

        manager_versions.push(RollupVersion {
            rollup_binary,
            config_path,
            migration_path,
            start_height,
            stop_height: Some(stop_height),
        });
        soak_versions.push((soak_binary, stop_height));
    }

    let manager_binary =
        sov_rollup_versioned_setup::build_rollup_manager_binary(&directories.manager_build_dir)?;

    Ok(AcceptanceRunPlan {
        manager_binary,
        manager_versions,
        soak_config: SoakManagerConfig::new(
            SoakWorkerConfig {
                num_workers: SOAK_NUM_WORKERS,
                salt: SOAK_SALT,
            },
            soak_versions,
            SOAK_SAFETY_STOP_BLOCKS,
        ),
        recorded_data,
        blocks_per_version,
    })
}

/// Build a soak config that runs ONLY the last version, at its configured stop height, using the
/// salt that a full from-genesis run would have assigned to that version
/// (`base_salt + num_workers * (num_versions - 1)`).
///
/// Used by `setup` in append mode: the historical versions are resynced from the existing DA and
/// only the last (new) version is generated, so its soak worker accounts must use the next salt
/// segment to avoid colliding with the resynced historical accounts. This mirrors the external
/// [`SoakManagerConfig::for_resync`], except it keeps the last version's configured stop height
/// (no extension) and targets the last version itself rather than the segment after it.
///
/// Returns `None` if the config has no versions.
pub fn last_version_soak_config(soak_config: &SoakManagerConfig) -> Option<SoakManagerConfig> {
    let (last_binary, last_stop_height) = soak_config.versions.last()?;
    let num_versions = soak_config.versions.len() as u32;
    let mut config = soak_config.config.clone();
    config.salt += config.num_workers * num_versions.saturating_sub(1);
    Some(SoakManagerConfig::new(
        config,
        vec![(last_binary.clone(), *last_stop_height)],
        soak_config.safety_stop_blocks,
    ))
}

pub fn extend_last_stop_height(
    versions: &[RollupVersion],
    extra_blocks: u64,
) -> Vec<RollupVersion> {
    if extra_blocks == 0 {
        return versions.to_vec();
    }
    let mut extended = versions.to_vec();
    if let Some(last) = extended.last_mut() {
        let current_stop = last
            .stop_height
            .expect("acceptance-test rollup versions must have a stop height");
        last.stop_height = Some(current_stop + extra_blocks);
    }
    extended
}

pub fn write_manager_config(path: &Path, versions: &[RollupVersion]) -> Result<(), anyhow::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create rollup manager config directory {}",
                parent.display()
            )
        })?;
    }
    let manager_config = ManagerConfig {
        versions: versions.to_vec(),
    };
    fs::write(path, serde_json::to_string_pretty(&manager_config)?)
        .with_context(|| format!("failed to write rollup manager config {}", path.display()))?;
    Ok(())
}

pub fn spawn_rollup_manager(
    manager_binary: &Path,
    manager_config: &Path,
    directories: &Directories,
    stdout_log_path: Option<&Path>,
) -> Result<ManagedRollupProcess, anyhow::Error> {
    let manager_config_arg = manager_config.to_string_lossy().to_string();
    let genesis_arg = directories
        .acceptance_test_dir
        .join("genesis.json")
        .to_string_lossy()
        .to_string();

    let mut cmd = TokioCommand::new(manager_binary);
    cmd.args([
        "-c",
        &manager_config_arg,
        "--no-checkpoint-file",
        "--",
        "--genesis-path",
        &genesis_arg,
    ])
    .current_dir(&directories.rollup_root)
    .env("RUST_LOG", "info");
    // Create a dedicated process group for manager + its rollup children so signal-based cleanup
    // can always terminate the full subtree without orphaning the actual rollup binary.
    cmd.process_group(0);
    #[cfg(target_os = "linux")]
    {
        let parent_pid = std::process::id();
        // SAFETY: `pre_exec` runs in the freshly forked child before `exec`, so it must only call
        // async-signal-safe operations. `prctl` and `getppid` satisfy that requirement here.
        unsafe {
            cmd.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() as u32 != parent_pid {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "acceptance-test parent exited before manager exec",
                    ));
                }
                Ok(())
            });
        }
    }

    if let Some(path) = stdout_log_path {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create rollup manager log directory {}",
                    parent.display()
                )
            })?;
        }
        let log_file = std::fs::File::create(path)
            .with_context(|| format!("failed to create rollup manager log {}", path.display()))?;
        cmd.stdout(log_file.try_clone().with_context(|| {
            format!(
                "failed to clone rollup manager log handle {}",
                path.display()
            )
        })?)
        .stderr(log_file);
    }

    let child = cmd.spawn().with_context(|| {
        format!(
            "failed to spawn rollup manager {}",
            manager_binary.display()
        )
    })?;
    Ok(ManagedRollupProcess::new(child))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_version_soak_config_uses_last_version_and_offsets_salt() {
        let config = SoakManagerConfig::new(
            SoakWorkerConfig {
                num_workers: 20,
                salt: 3,
            },
            vec![
                (PathBuf::from("/bin/soak-v0"), 1000),
                (PathBuf::from("/bin/soak-v1"), 2000),
                (PathBuf::from("/bin/soak-v2"), 3000),
            ],
            5,
        );

        let appended = last_version_soak_config(&config).expect("config has versions");

        assert_eq!(appended.versions.len(), 1);
        assert_eq!(appended.versions[0].0, PathBuf::from("/bin/soak-v2"));
        // Last version keeps its configured stop height (no extension, unlike `for_resync`).
        assert_eq!(appended.versions[0].1, 3000);
        // base salt 3 + num_workers 20 * (3 versions - 1) = 43, matching the salt a from-genesis
        // run would assign to the third version (index 2), so worker accounts don't collide with
        // the resynced historical accounts.
        assert_eq!(appended.config.salt, 43);
        assert_eq!(appended.config.num_workers, 20);
        assert_eq!(appended.safety_stop_blocks, 5);
    }

    #[test]
    fn last_version_soak_config_two_versions_offsets_by_one_segment() {
        let config = SoakManagerConfig::new(
            SoakWorkerConfig {
                num_workers: 20,
                salt: 3,
            },
            vec![
                (PathBuf::from("/bin/soak-v0"), 1000),
                (PathBuf::from("/bin/soak-v1"), 2000),
            ],
            5,
        );

        let appended = last_version_soak_config(&config).expect("config has versions");

        assert_eq!(appended.versions[0].0, PathBuf::from("/bin/soak-v1"));
        // base salt 3 + 20 * (2 - 1) = 23.
        assert_eq!(appended.config.salt, 23);
    }

    #[test]
    fn last_version_soak_config_empty_is_none() {
        let config = SoakManagerConfig::new(
            SoakWorkerConfig {
                num_workers: 1,
                salt: 0,
            },
            vec![],
            0,
        );

        assert!(last_version_soak_config(&config).is_none());
    }
}
