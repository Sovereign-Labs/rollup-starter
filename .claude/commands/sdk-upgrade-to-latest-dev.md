# SDK Upgrade to Latest Dev

Prepare `sdk-upgrade` branch and port SDK changes to the starter repo, so acceptance tests can promote the dev branch.

SDK repo: https://github.com/Sovereign-Labs/sovereign-sdk

## Steps

### 1. Prepare git state

1. Check that the working directory is clean (no uncommitted changes). If not clean, abort and inform the user.
2. Checkout the `main` branch.
3. Check if the existing `sdk-upgrade` branch (local OR remote) for unmerged commits ahead of main, and ask the user whether to proceed if there is.
4. Delete the `sdk-upgrade` branch if it exists and all changes are commited (both local and remote tracking).
5. Create a new `sdk-upgrade` branch on top of the current `main`.

### 2. Get SDK revisions

1. **Get the current (prev) revision**: Read `Cargo.toml` and extract the revision from a `sov-*` dependency that uses `https://github.com/Sovereign-Labs/sovereign-sdk.git`. This is the "prev" or "start" revision.
2. **Get the latest dev revision**: Fetch the latest commit SHA from the `dev` branch of `https://github.com/Sovereign-Labs/sovereign-sdk`. This is the "next" or "new" revision.

Store both revisions for later use.

### 3. Update SDK revision

Execute `./scripts/update_rev.sh <NEW_REV>` where `<NEW_REV>` is the latest dev revision.

### 4. Build and verify

Run the following in order, stopping if any step fails:

1. `make lint` - Check compilation and update cargo.locks
2. `make check` - Update cargo.lock for ZK guests
3. `cargo test` - Ensure tests compile and pass

### 5. Review READMEs

Check the following files to ensure they are still up-to-date with any SDK changes:
- `README.md`
- `GETTING_STARTED_WITH_CELESTIA.md`
- `GETTING_STARTED_WITH_HYPERLANE.md`

Inform the user about any potential issues or outdated information.

### 6. Commit changes (happy path)

If all steps succeed, commit the changes with the message format:

```
Update to the latest dev YYYY-MM-DD

Upgrading to latest dev on YYYY-MM-DD NEW_REV
```

Where:
- `YYYY-MM-DD` is the current date
- `NEW_REV` is the new SDK revision

---

## If Compilation or Tests Fail

If any step in section 4 fails, perform the following investigation:

### 1. Check SDK CHANGELOG

Fetch and review `CHANGELOG.md` from the SDK repo root. Look for changes between the prev revision and the new revision that might explain the failure.

### 2. Explore demo-rollup changes

Explore the changes in SDK's `examples/demo-rollup` directory between the prev and new revisions:

```bash
# In the SDK repo or via GitHub API
git diff <PREV_REV>..<NEW_REV> -- examples/demo-rollup/
```

Often changes in demo-rollup can be understood and ported to the starter repo.

### 3. Report findings

Present the findings to the user with:
- The specific error messages
- Relevant CHANGELOG entries
- Changes from demo-rollup that might need to be ported
- Suggestions for fixing the issues

Do not commit if there are failures - wait for user guidance on how to proceed.
