# Intro

This tutorial describes the process of upgrading rollup starter to the latest SDK version.

Please take your time to read the whole tutorial first and familiarize yourself with the overall picture.

There are many small, but important details, so it is recommended to take it slow.

# Upgrading rollup starter to the new version of Sovereign SDK.

Rollup starter supports 2 Data Availability layers: Mock and Celestia.

|          | Local          |
|----------|----------------|
| Mock DA  | SQlite         |
| Celestia | Docker compose |

The following order is recommended:

1. [Local MockDa](#local-mockda): Make sure project compiles, tests are passing, node is running.
2. [Local Celestia](#local-celestia): Same as previous, but it works with Celestia

# Steps

After completing each step, please make sure always check for any warnings or oddities.
Each step describes what changes needs to be done. After completing all steps, please run rollup in given configuration
and run some requests against it.

## Basic

- [ ] Clean up previous build data if available: `make clean`.
  Also clean up `~/.sov_cli_wallet` if there are problems during deployment and are no important keys there.
- [ ] Replace git commit hash in main `Cargo.toml` and in provers. Make sure that other 3rd party dependencies have
  correct versions. Helper script ` ./scripts/update_rev.sh NEW_REV` can be used for upgrading SDK revision in all
  Cargo.toml files
- [ ] Identify all changes from Sovereign SDK. In Sovereign SDK repo:
  `git diff EXISTING_COMMIT_IN_CARGO_TOML COMMIT_UPDATE_TO -- CHANGELOG.md`
- [ ] [`Cargo.toml`](./Cargo.toml)
    - [ ] [`risc0/guest-celestia/Cargo.toml`](crates/provers/risc0/guest-celestia/Cargo.toml)
    - [ ] [`risc0/guest-mock/Cargo.toml`](crates/provers/risc0/guest-mock/Cargo.toml)
- [ ] Adjust sample requests in [`test-data/requests`](./test-data/requests)
- [ ] Adjust [`constants.toml`](./constants.toml). This file is used for all configurations: local and remote
- [ ] Make sure `make lint` and `cargo test` are passing.

## If Rust toolchains has been upgraded

If a Rust native or zkVM toolchain version has changed:

- [ ] Update local [`rust-toolchain.toml`](./rust-toolchain.toml)
- [ ] Update risc0 toolchain in [`Makefile`](./Makefile)

## Local MockDa

- [ ] Rollup config [`rollup.toml`](./configs/mock/rollup.toml) for Mock DA and [`rollup.toml`](./configs/celestia/rollup.toml)
- [ ] Genesis params for Mock DA in [`configs/mock/genesis.json)`](./configs/mock/genesis.json) and for Celestia DA in [`configs/celestia/genesis.json)`](./configs/celestia/genesis.json)

Steps to take testing are described in the main [README.md](./README.md)

## Local Celestia

- [ ] Rollup config [celestia_rollup_config.toml](./celestia_rollup_config.toml)
- [ ] Genesis params in folder [`test-data/genesis/celestia`](./test-data/genesis/celestia)

Steps to take testing are described in the
main [README.md](./README.md#alternative-configurationsa) in the Celestia section.