## Acceptance Test

This crate runs a test which syncs the rollup against a known set of block and asserts that all
of the *ledger* state responses are as expected. This guarantees the correct state roots are being
calculated - which transitively guarantees that the state is correct. After resyncing, we run a 
soak test for a fixed length of time and ensure that (1) there are no errors and (2) the throughput
is within the expected range.

To run the test simply `cargo run --bin acceptance-test`. All data should have been prepopulated.
`build.rs` copies checked-in EVM contract artifacts into `OUT_DIR` by default. If the Solidity source changes
and the checked-in artifacts are stale or missing, it will regenerate them via `solc`, so `solc` is only needed
when updating the contract itself.

The default rollup state directory is treated as transient. After a successful `setup` or
`acceptance-test` run it is emptied automatically, so repeated runs work without extra flags.

If you provide an explicit `--rollup-state-dir`, the binaries preserve it by default and will exit
if it is non-empty on the next run. To clear an existing state directory automatically, pass
`--on-existing-rollup-state=clobber`.

If your local default state dir is already populated from an older run, clear it once with
`cargo run --bin setup -- --on-existing-rollup-state=clobber` or remove
`acceptance-test-data/<profile>/rollup-starter-data` manually.

However, in case of errors it can sometimes be the case that docker containers haven't been shut down
from the previous run. To fix, simply `docker rm -f postgres-acceptance-test`.

The binaries support a default `full` profile and an optional `--short` profile. `full` uses
`blocks-per-version=1000` and `full-slot-save-interval=25`; `short` uses
`blocks-per-version=30` and `full-slot-save-interval=5`.

`acceptance-test` builds the local rollup and soak binaries with `constants.testing.toml`
from this directory so historical transactions can be replayed with their original chain
hash. `setup` uses the root `constants.toml`, since it generates fresh transactions.

The acceptance data and throughput roots are profile-scoped. By default they are stored under:

- `acceptance-test-data/full` or `acceptance-test-data/short`
- `acceptance-throughput/full` or `acceptance-throughput/short`

If you already have an older local dataset in the legacy flat layout, move it into the `full`
subdirectories or regenerate it with `cargo run --bin setup`.

Useful examples:

- `cargo run --bin setup -- --short`
- `cargo run --bin setup -- --on-existing-rollup-state=clobber`
- `cargo run --bin acceptance-test -- --short --no-throughput-check`
- `cargo run --bin acceptance-test -- --acceptance-data-dir /tmp/acceptance-data`

### Rollup versioning (hard forks)

If a multi-version spec (`versions.yaml` at the repo root) is present, the test builds a binary per
version, resyncs the historical versions from their pinned commits, and only puts the latest version
(local `HEAD`, with the SDK commit under test) through the post-resync soak. Per-version block ranges
are derived from `blocks_per_version` (`stop_height = (idx + 1) * blocks_per_version`).

`setup` is version-aware and auto-detects how to run:

- **From-genesis (default):** with no persistent MockDA yet, or a single-version spec, it regenerates
  everything from genesis (the original behavior).
- **Append (auto):** when a persistent MockDA already exists *and* the spec has more than one version,
  `setup` restores that MockDA, resyncs the historical versions (verifying them against the existing
  snapshots), then generates only the new last version's data — appending one version's worth of
  blocks and snapshots and re-persisting the extended MockDA. This is the one-shot run to perform
  right after a hard fork adds a version.

Append requires the existing data to end exactly at the fork boundary — i.e. the previous version's
`stop_height`, which is where the new version's `start_height` begins. `setup` validates this and
refuses to run if the data already contains the new version's range (regenerating an existing version
would require pruning the MockDA, which is not yet supported) or if it covers fewer versions than the
spec. In those cases, clear `acceptance-test-data` and regenerate from genesis.

### Resetting the Test

If you need to generate a new test, simply run
`rm -r acceptance-test-data acceptance-throughput && cargo run --bin setup`.
This will generate all of the needed files, including a fresh mockDA. Note that setup may take an
hour or more to run, since we have to generate a full history for the rollup.
