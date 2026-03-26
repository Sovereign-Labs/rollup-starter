use anyhow::{anyhow, bail};
use clap::{Args, ValueEnum};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_POSTGRES_CONTAINER_NAME: &str = "postgres-acceptance-test";
pub const DEFAULT_BLOCKS_PER_VERSION: u64 = 1000;
pub const DEFAULT_FULL_SLOT_SAVE_INTERVAL: u64 = 25;
pub const SHORT_BLOCKS_PER_VERSION: u64 = 30;
pub const SHORT_FULL_SLOT_SAVE_INTERVAL: u64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunProfile {
    Full,
    Short,
}

impl RunProfile {
    pub fn subdir(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Short => "short",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum ExistingRollupState {
    Clobber,
    #[default]
    Error,
    Ignore,
}

#[derive(Args, Debug, Clone, Default)]
pub struct CommonArgs {
    /// Use the short acceptance profile. The default profile is full.
    #[arg(long)]
    pub short: bool,

    /// Base directory for acceptance-test data. The selected profile is appended as a subdirectory.
    #[arg(long)]
    pub acceptance_data_dir: Option<PathBuf>,

    /// Base directory for throughput data. The selected profile is appended as a subdirectory.
    #[arg(long)]
    pub acceptance_throughput_dir: Option<PathBuf>,

    /// Rollup state directory. If omitted, defaults to <acceptance-data-dir>/<profile>/rollup-starter-data.
    #[arg(long)]
    pub rollup_state_dir: Option<PathBuf>,

    /// How to handle an existing rollup state directory before running.
    #[arg(long, value_enum, default_value_t = ExistingRollupState::Error)]
    pub on_existing_rollup_state: ExistingRollupState,

    /// Name of the postgres docker container used during the run.
    #[arg(long)]
    pub postgres_docker_container_name: Option<String>,

    /// Directory used to cache commit-built binaries across runs.
    #[arg(long)]
    pub binary_cache_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRunSettings {
    pub profile: RunProfile,
    pub blocks_per_version: u64,
    pub full_slot_save_interval: u64,
    pub acceptance_data_dir: Option<PathBuf>,
    pub acceptance_throughput_dir: Option<PathBuf>,
    pub rollup_state_dir: Option<PathBuf>,
    pub on_existing_rollup_state: ExistingRollupState,
    pub postgres_docker_container_name: String,
    pub binary_cache_dir: Option<PathBuf>,
}

impl ResolvedRunSettings {
    pub fn from_common_args(args: CommonArgs) -> Self {
        let profile = if args.short {
            RunProfile::Short
        } else {
            RunProfile::Full
        };

        let (default_blocks_per_version, default_full_slot_save_interval) = match profile {
            RunProfile::Full => (DEFAULT_BLOCKS_PER_VERSION, DEFAULT_FULL_SLOT_SAVE_INTERVAL),
            RunProfile::Short => (SHORT_BLOCKS_PER_VERSION, SHORT_FULL_SLOT_SAVE_INTERVAL),
        };

        Self {
            profile,
            blocks_per_version: default_blocks_per_version,
            full_slot_save_interval: default_full_slot_save_interval,
            acceptance_data_dir: args.acceptance_data_dir,
            acceptance_throughput_dir: args.acceptance_throughput_dir,
            rollup_state_dir: args.rollup_state_dir,
            on_existing_rollup_state: args.on_existing_rollup_state,
            postgres_docker_container_name: args
                .postgres_docker_container_name
                .unwrap_or_else(|| DEFAULT_POSTGRES_CONTAINER_NAME.to_owned()),
            binary_cache_dir: args.binary_cache_dir,
        }
    }

    pub fn cleanup_rollup_state_on_success(&self) -> bool {
        self.rollup_state_dir.is_none()
    }
}

pub fn prepare_rollup_state_dir(
    path: &Path,
    policy: ExistingRollupState,
) -> Result<(), anyhow::Error> {
    match fs::metadata(path) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                bail!(
                    "Rollup state path {} exists and is not a directory",
                    path.display()
                );
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path)?;
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    }

    match policy {
        ExistingRollupState::Clobber => clear_directory(path)?,
        ExistingRollupState::Error => ensure_directory_empty(path)?,
        ExistingRollupState::Ignore => {}
    }

    fs::create_dir_all(path)?;
    Ok(())
}

pub fn cleanup_rollup_state_dir(path: &Path) -> Result<(), anyhow::Error> {
    match fs::metadata(path) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                bail!(
                    "Rollup state path {} exists and is not a directory",
                    path.display()
                );
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    }

    clear_directory(path)
}

fn clear_directory(path: &Path) -> Result<(), anyhow::Error> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        if entry.file_type()?.is_dir() {
            fs::remove_dir_all(&entry_path)?;
        } else {
            fs::remove_file(&entry_path)?;
        }
    }
    Ok(())
}

fn ensure_directory_empty(path: &Path) -> Result<(), anyhow::Error> {
    if fs::read_dir(path)?.next().is_some() {
        return Err(anyhow!(
            "Rollup state directory {} is not empty. Re-run with --on-existing-rollup-state=clobber or --on-existing-rollup-state=ignore to proceed.",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use tempfile::tempdir;

    #[derive(Parser, Debug)]
    struct TestParser {
        #[command(flatten)]
        common: CommonArgs,
    }

    #[test]
    fn default_resolution_uses_full_profile_defaults() {
        let resolved = ResolvedRunSettings::from_common_args(CommonArgs::default());

        assert_eq!(resolved.profile, RunProfile::Full);
        assert_eq!(resolved.blocks_per_version, DEFAULT_BLOCKS_PER_VERSION);
        assert_eq!(
            resolved.full_slot_save_interval,
            DEFAULT_FULL_SLOT_SAVE_INTERVAL
        );
        assert_eq!(
            resolved.postgres_docker_container_name,
            DEFAULT_POSTGRES_CONTAINER_NAME
        );
        assert_eq!(
            resolved.on_existing_rollup_state,
            ExistingRollupState::Error
        );
    }

    #[test]
    fn short_profile_uses_short_defaults() {
        let resolved = ResolvedRunSettings::from_common_args(CommonArgs {
            short: true,
            ..CommonArgs::default()
        });

        assert_eq!(resolved.profile, RunProfile::Short);
        assert_eq!(resolved.blocks_per_version, SHORT_BLOCKS_PER_VERSION);
        assert_eq!(
            resolved.full_slot_save_interval,
            SHORT_FULL_SLOT_SAVE_INTERVAL
        );
    }

    #[test]
    fn parser_allows_selecting_existing_state_policy() {
        let parsed =
            TestParser::try_parse_from(["acceptance-test", "--on-existing-rollup-state=ignore"])
                .expect("parser should accept an existing state policy");

        assert_eq!(
            parsed.common.on_existing_rollup_state,
            ExistingRollupState::Ignore
        );
    }

    #[test]
    fn parser_defaults_existing_state_policy_to_error() {
        let parsed =
            TestParser::try_parse_from(["acceptance-test"]).expect("parser should use defaults");

        assert_eq!(
            parsed.common.on_existing_rollup_state,
            ExistingRollupState::Error
        );
    }

    #[test]
    fn parser_allows_short_profile_flag() {
        let parsed = TestParser::try_parse_from(["acceptance-test", "--short"])
            .expect("parser should accept the short profile flag");

        assert!(parsed.common.short);
    }

    #[test]
    fn prepare_rollup_state_dir_creates_missing_directory() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("state");

        prepare_rollup_state_dir(&path, ExistingRollupState::Error).unwrap();

        assert!(path.is_dir());
    }

    #[test]
    fn clobber_clears_non_empty_state_directory() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("state");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("leftover"), b"stale").unwrap();

        prepare_rollup_state_dir(&path, ExistingRollupState::Clobber).unwrap();

        assert_eq!(fs::read_dir(&path).unwrap().count(), 0);
    }

    #[test]
    fn error_policy_rejects_non_empty_state_directory() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("state");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("leftover"), b"stale").unwrap();

        let err = prepare_rollup_state_dir(&path, ExistingRollupState::Error)
            .expect_err("error policy should reject non-empty directories");

        assert!(err.to_string().contains("not empty"));
    }

    #[test]
    fn ignore_policy_preserves_non_empty_state_directory() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("state");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("leftover"), b"stale").unwrap();

        prepare_rollup_state_dir(&path, ExistingRollupState::Ignore).unwrap();

        assert!(path.join("leftover").exists());
    }

    #[test]
    fn cleanup_rollup_state_dir_empties_existing_directory() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("state");
        fs::create_dir_all(path.join("nested")).unwrap();
        fs::write(path.join("leftover"), b"stale").unwrap();
        fs::write(path.join("nested/leftover"), b"stale").unwrap();

        cleanup_rollup_state_dir(&path).unwrap();

        assert_eq!(fs::read_dir(&path).unwrap().count(), 0);
    }

    #[test]
    fn cleanup_rollup_state_dir_ignores_missing_directory() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("state");

        cleanup_rollup_state_dir(&path).unwrap();

        assert!(!path.exists());
    }

    #[test]
    fn explicit_rollup_state_dir_is_preserved_on_success() {
        let resolved = ResolvedRunSettings::from_common_args(CommonArgs {
            rollup_state_dir: Some(PathBuf::from("/tmp/custom-state")),
            ..CommonArgs::default()
        });

        assert!(!resolved.cleanup_rollup_state_on_success());
    }

    #[test]
    fn default_rollup_state_dir_is_cleaned_on_success() {
        let resolved = ResolvedRunSettings::from_common_args(CommonArgs::default());

        assert!(resolved.cleanup_rollup_state_on_success());
    }
}
