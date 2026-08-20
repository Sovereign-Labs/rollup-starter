use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use anyhow::{anyhow, Context};
use sov_rollup_manager::{ManagerConfig, RollupVersion};
use sov_soak_manager::{SoakManagerConfig, SoakWorkerConfig};
use sov_versioned_artifact_builder::{
    prepare_artifacts, BuildRequest, BuildSpec, BuildTargets, RollupBuilder, VersionBuildSpec,
};
use tokio::process::Command as TokioCommand;
use tracing::info;

use crate::{Directories, ManagedRollupProcess};

pub const ROLLUP_REPO_URL: &str = "https://github.com/Sovereign-Labs/rollup-starter.git";
pub const ROLLUP_MANAGER_REPO_URL: &str = "https://github.com/Sovereign-Labs/sov-rollup-manager";
pub const ROLLUP_MANAGER_BRANCH: &str = "master";
pub const VERSION_SPEC_FILE: &str = "versions.yaml";
pub const VERSION_VARS_COMMIT_KEY: &str = "rollup_commit_hash";
pub const VERSION_CONFIG_TEMPLATE_PATH: &str = "scripts/acceptance-test/rollup_config.toml";
pub const SOAK_NUM_WORKERS: u32 = 20;
pub const SOAK_SALT: u32 = 3; // existing acceptance-test-data started from 3 for some reason
pub const SOAK_SAFETY_STOP_BLOCKS: u64 = 5;
const ACCEPTANCE_TEST_FEATURES: [&str; 3] = ["acceptance-testing", "mock_da", "mock_zkvm"];
const ACCEPTANCE_CONSTANTS_FILENAME: &str = "constants.testing.toml";
/// Repo-relative directory whose `constants.testing.toml` all versioned builds must use.
const ACCEPTANCE_CONSTANTS_REPO_DIR: &str = "scripts/acceptance-test";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalConstantsManifest {
    Default,
    AcceptanceTest,
}

fn acceptance_test_features() -> Vec<String> {
    ACCEPTANCE_TEST_FEATURES
        .iter()
        .map(|feature| feature.to_string())
        .collect()
}

fn acceptance_test_feature_list() -> String {
    ACCEPTANCE_TEST_FEATURES.join(",")
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
        soak.test_constants_manifest_dir = constants_dir;
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
    let spec_path = directories.rollup_root.join(VERSION_SPEC_FILE);
    if !spec_path.exists() {
        info!(
            path = %spec_path.display(),
            "No versions spec found, defaulting to local HEAD only"
        );
        return Ok(vec![ResolvedVersion {
            source: VersionSource::LocalHead,
            migration_path: None,
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

        versions.push(ResolvedVersion {
            source: VersionSource::RemoteCommit(vars.rollup_commit_hash),
            migration_path: entry.migration_path.clone(),
        });
    }

    if versions.is_empty() {
        versions.push(ResolvedVersion {
            source: VersionSource::LocalHead,
            migration_path: None,
        });
    } else if let Some(last) = versions.last_mut() {
        last.source = VersionSource::LocalHead;
    }

    Ok(versions)
}

fn build_local_head_binaries(
    directories: &Directories,
    constants_manifest: LocalConstantsManifest,
) -> Result<(PathBuf, PathBuf), anyhow::Error> {
    let feature_list = acceptance_test_feature_list();
    let rollup_root = &directories.rollup_root;
    let target_dir = match constants_manifest {
        LocalConstantsManifest::Default => rollup_root.join("target"),
        LocalConstantsManifest::AcceptanceTest => {
            let constants_path = directories
                .acceptance_test_dir
                .join(ACCEPTANCE_CONSTANTS_FILENAME);
            if !constants_path.is_file() {
                return Err(anyhow!(
                    "acceptance-test constants manifest not found at {}",
                    constants_path.display()
                ));
            }
            // Keep this build in a dedicated target dir so the proc-macro crate is compiled
            // with the acceptance-test constants manifest instead of reusing the default one.
            rollup_root.join("target").join("acceptance-test")
        }
    };

    let configure_constants_manifest = |cmd: &mut StdCommand| {
        if constants_manifest == LocalConstantsManifest::AcceptanceTest {
            cmd.env("SOV_TEST_MODE_CONST_MANIFEST", "1")
                .env("CONSTANTS_MANIFEST", &directories.acceptance_test_dir)
                .env("CARGO_TARGET_DIR", &target_dir);
        }
    };

    tracing::info!("Building rollup at local HEAD...");
    let mut rollup_build = StdCommand::new("cargo");
    rollup_build.current_dir(rollup_root).args([
        "build",
        "--release",
        "--package",
        "rollup-starter",
        "--bin",
        "rollup",
        "--no-default-features",
        "--features",
        &feature_list,
    ]);
    configure_constants_manifest(&mut rollup_build);
    run_checked(&mut rollup_build, "build local head rollup binary")?;

    tracing::info!("Building soak test at local HEAD...");
    let mut soak_build = StdCommand::new("cargo");
    soak_build.current_dir(rollup_root).args([
        "build",
        "--release",
        "--package",
        "rollup-starter-soak-test",
        "--bin",
        "rollup-starter-soak-test",
        "--no-default-features",
        "--features",
        &feature_list,
    ]);
    configure_constants_manifest(&mut soak_build);
    run_checked(&mut soak_build, "build local head soak binary")?;

    let release_dir = target_dir.join("release");
    let rollup_bin = release_dir.join("rollup");
    if !rollup_bin.exists() {
        return Err(anyhow!(
            "local rollup binary not found at {}",
            rollup_bin.display()
        ));
    }

    let soak_bin_default = release_dir.join("rollup-starter-soak-test");
    let soak_bin = if soak_bin_default.exists() {
        soak_bin_default
    } else {
        let subscriber_bin = release_dir.join("subscriber");
        if subscriber_bin.exists() {
            subscriber_bin
        } else {
            return Err(anyhow!(
                "local soak binary not found at {} or {}",
                soak_bin_default.display(),
                subscriber_bin.display()
            ));
        }
    };

    Ok((
        rollup_bin.canonicalize().with_context(|| {
            format!(
                "failed to canonicalize rollup binary {}",
                rollup_bin.display()
            )
        })?,
        soak_bin.canonicalize().with_context(|| {
            format!("failed to canonicalize soak binary {}", soak_bin.display())
        })?,
    ))
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

fn build_rollup_manager_binary(manager_build_root: &Path) -> Result<PathBuf, anyhow::Error> {
    if manager_build_root.exists() {
        fs::remove_dir_all(manager_build_root).with_context(|| {
            format!(
                "failed to remove existing rollup manager build directory {}",
                manager_build_root.display()
            )
        })?;
    }
    fs::create_dir_all(manager_build_root).with_context(|| {
        format!(
            "failed to create rollup manager build directory {}",
            manager_build_root.display()
        )
    })?;
    let manager_repo = manager_build_root.join("repo");
    let manager_repo_arg = manager_repo.to_string_lossy().to_string();

    run_checked(
        StdCommand::new("git").args([
            "clone",
            "--depth",
            "1",
            "--branch",
            ROLLUP_MANAGER_BRANCH,
            ROLLUP_MANAGER_REPO_URL,
            &manager_repo_arg,
        ]),
        "clone sov-rollup-manager",
    )?;

    run_checked(
        StdCommand::new("cargo").current_dir(&manager_repo).args([
            "build",
            "--release",
            "--bin",
            "sov-rollup-manager",
        ]),
        "build sov-rollup-manager",
    )?;

    let manager_bin = manager_repo.join("target/release/sov-rollup-manager");
    if !manager_bin.exists() {
        return Err(anyhow!(
            "built manager binary not found at {}",
            manager_bin.display()
        ));
    }
    manager_bin.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize built manager binary {}",
            manager_bin.display()
        )
    })
}

pub fn prepare_acceptance_run_plan(
    directories: &Directories,
    password: &str,
    blocks_per_version: u64,
) -> Result<AcceptanceRunPlan, anyhow::Error> {
    prepare_acceptance_run_plan_with_constants(
        directories,
        password,
        blocks_per_version,
        LocalConstantsManifest::Default,
    )
}

pub fn prepare_acceptance_run_plan_with_constants(
    directories: &Directories,
    password: &str,
    blocks_per_version: u64,
    local_constants_manifest: LocalConstantsManifest,
) -> Result<AcceptanceRunPlan, anyhow::Error> {
    let binary_cache_dir = &directories.rollup_build_cache_dir;
    fs::create_dir_all(binary_cache_dir).with_context(|| {
        format!(
            "failed to create rollup binary cache directory {}",
            binary_cache_dir.display()
        )
    })?;

    let resolved_versions = load_version_sources(directories)?;
    let remote_commits: Vec<String> = resolved_versions
        .iter()
        .filter_map(|version| match &version.source {
            VersionSource::RemoteCommit(commit) => Some(commit.clone()),
            VersionSource::LocalHead => None,
        })
        .collect();

    let (mut remote_artifacts, template_reader) = if remote_commits.is_empty() {
        (None, None)
    } else {
        let build_spec = BuildSpec {
            repo_url: Some(ROLLUP_REPO_URL.to_string()),
            targets: default_build_targets(),
            versions: remote_commits
                .iter()
                .map(|commit| VersionBuildSpec {
                    commit: commit.clone(),
                    build_soak: true,
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

    let (local_rollup_bin, local_soak_bin) =
        build_local_head_binaries(directories, local_constants_manifest)?;

    let versioned_configs_dir = directories.output_dir.join("versioned-configs");
    fs::create_dir_all(&versioned_configs_dir).with_context(|| {
        format!(
            "failed to create versioned config directory {}",
            versioned_configs_dir.display()
        )
    })?;

    let mut manager_versions = Vec::with_capacity(resolved_versions.len());
    let mut soak_versions = Vec::with_capacity(resolved_versions.len());

    for (idx, resolved_version) in resolved_versions.iter().enumerate() {
        let stop_height = ((idx as u64) + 1) * blocks_per_version;
        let start_height = if idx == 0 {
            None
        } else {
            Some((idx as u64 * blocks_per_version) + 1)
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

                    let migration_path = if let Some(path) = &resolved_version.migration_path {
                        let migration_path = if path.is_absolute() {
                            path.clone()
                        } else {
                            directories.rollup_root.join(path)
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
                    let migration_path = if let Some(path) = &resolved_version.migration_path {
                        let migration_path = if path.is_absolute() {
                            path.clone()
                        } else {
                            directories.rollup_root.join(path)
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

    let manager_binary = build_rollup_manager_binary(&directories.manager_build_dir)?;

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
    })
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
