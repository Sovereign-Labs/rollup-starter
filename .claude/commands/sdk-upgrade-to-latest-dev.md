# SDK Upgrade to Latest Dev

Prepare `sdk-upgrade` branch and port SDK changes to the starter repo, so acceptance tests can promote the dev branch.

SDK repo: https://github.com/Sovereign-Labs/sovereign-sdk

## Steps

### 1. Prepare git state

1. Check that the working directory is clean (no uncommitted changes). If not clean, abort and inform the user.
2. Fetch latest from origin and sync local main branch.
3. Checkout the `main` branch.
4. Check if the `sdk-upgrade` branch (local OR remote) has unmerged commits ahead of main. Ask the user whether to proceed if it does.
5. Delete the `sdk-upgrade` branch if it exists and all changes are merged (both local and remote tracking).
6. Create a new `sdk-upgrade` branch from `main`.

### 2. Get SDK revisions

1. **Current revision**: Read `Cargo.toml` and extract the `rev` from any `sov-*` dependency using `https://github.com/Sovereign-Labs/sovereign-sdk.git`. This is the "prev" revision.
2. **Latest dev revision**: Fetch the latest commit SHA from the `dev` branch of sovereign-sdk using `gh api repos/Sovereign-Labs/sovereign-sdk/commits/dev --jq '.sha'`. This is the "new" revision.

Store both revisions — they're needed throughout.

### 3. Review SDK changes (before building)

Before making changes, understand what's new in the SDK:

1. **Fetch CHANGELOG**: Review `CHANGELOG.md` from the SDK repo between prev and new revisions for breaking changes.
2. **Check demo-rollup diff**: This is the reference implementation — changes here usually need to be ported.
   ```bash
   gh api repos/Sovereign-Labs/sovereign-sdk/compare/{prev}...{new} --jq '.files[] | select(.filename | startswith("examples/demo-rollup/")) | .filename'
   ```
3. **Check for new constants**: Look for changes to the SDK's constants files that might require new entries in our `constants.toml`.

Report findings to the user before proceeding.

### 4. Update SDK revision

Run `./scripts/update_rev.sh <NEW_REV>` to update all relevant Cargo.toml files.

### 5. Verify configs and constants

Before building, check that configs and constants are up to date with the SDK changes identified in step 3.

1. **`constants.toml`**: Compare against SDK's demo-rollup constants. Add any new constants with appropriate values and comments.
2. **`configs/.*/rollup.toml`**: Verify fields still match the SDK's config struct definitions. Add/remove fields as needed.

### 6. Build, fix, repeat

Run these in order. If any step fails, investigate and fix before moving on. Loop until all pass.

1. **`make lint`** — Runs `cargo fmt --check`, `cargo check`, `cargo clippy`, and `zepter`. Fixes formatting, compilation, and lint issues.
2. **`make check`** — Updates the root `Cargo.lock`.
3. **`cargo nextest run`** — Run the full test suite.

#### How to fix breaking changes

1. Use the SDK diff from step 3 to understand the new API.
2. Look at `examples/demo-rollup` in the SDK for the reference implementation.
3. Port the equivalent changes to our codebase.
4. Re-run the failing step and repeat until clean.

### 7. Review READMEs

Check if any README files need updating to reflect SDK changes (new configuration options, changed commands, updated examples), especially those:
- `README.md`
- `GETTING_STARTED_WITH_CELESTIA.md`
- `GETTING_STARTED_WITH_HYPERLANE.md`

### 8. Commit and push

If all steps succeed:

1. Commit with this message format:
   ```
   Update to the latest dev YYYY-MM-DD

   Upgrading to latest dev on YYYY-MM-DD NEW_REV

   [List specific breaking changes and how they were resolved, if any]
   ```
2. Ask the user whether to push and create a PR.

### 9. Sync schema to `relay-chain-upgrade` (resync compatibility)

The `relay-chain-upgrade` branch carries Relay-specific values for the production resync job against `https://rpc.chain.relay.link`. After the SDK upgrade lands, port any **structural** schema changes (added / removed / renamed keys) so that branch keeps building. Never copy values — the Relay branch deliberately diverges on `CHAIN_ID`, namespaces, `GAS_TOKEN_ID`, prod admin & sequencer addresses, `genesis_da_height`, contract allowlist, etc.

1. Compute the delta — look at key-level differences only, ignore value-only differences:
   ```bash
   git diff sdk-upgrade..relay-chain-upgrade -- constants.toml configs/celestia/genesis.json
   ```
2. If there is a structural delta, **ask the user before porting**: "Schema changed in this SDK upgrade. Port the delta to `relay-chain-upgrade` now?"
3. On confirmation:
   - `git checkout relay-chain-upgrade` — use the local branch as-is. Do not rebase or fast-forward; it may carry unpushed work.
   - Apply only structural changes (add new keys with their default values & comments from `sdk-upgrade`, delete removed keys, rename where the SDK renamed). Preserve every Relay value.
   - Commit on `relay-chain-upgrade` with a message like `Port SDK schema additions from sdk-upgrade`.
   - `git checkout sdk-upgrade`.
4. If declined, skip without comment.
