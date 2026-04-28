# Release Process

## Tag scheme

Releases are tagged `<chain>-YYYY-MM-DD`, with a `.N` suffix for multiple releases on the same day:

- `relay-chain-2026-04-24`
- `relay-chain-2026-04-24.1` (second release that day)

The chain prefix scopes the release to a specific deployment, so the same repo can produce `relay-chain-*`, `some-other-chain-*`, etc. without conflicts.

## What gets released

Each release publishes:

1. A Docker image with the compiled `rollup` binary, pushed to GHCR at `ghcr.io/sovereign-labs/rollup-starter/<chain>`.
2. A GitHub Release with hand-written notes, linking to the image.

## Steps

### 1. Pick the tag

```bash
TAG=relay-chain-2026-04-24
IMAGE=ghcr.io/sovereign-labs/rollup-starter/relay-chain
```

Verify the working tree is clean and on the commit you want to release.

### 2. Build the Docker image

The build runs entirely inside Docker for reproducibility. From the repo root:

```bash
DOCKER_BUILDKIT=1 docker build \
  -t "$IMAGE:$TAG" \
  -t "$IMAGE:latest" \
  .
```

Uses default cargo features (`celestia_da,mock_zkvm`) and `cargo build --locked` so the release uses the checked-in `Cargo.lock`.

### 3. Smoke-test the image

Verify that the image starts and that the release binary is present before publishing anything:

```bash
docker run --rm "$IMAGE:$TAG" --help >/dev/null
```

If you are testing full node startup locally, remember that NOMT relies on `io_uring`. On hosts whose Docker daemon uses a restrictive seccomp profile, run with `--security-opt seccomp=unconfined` or an equivalent profile that allows `io_uring_setup`, `io_uring_enter`, and `io_uring_register`.

### 4. Tag the commit and push

Only push the git tag after the image has built and passed the smoke test:

```bash
git tag "$TAG"
git push origin "$TAG"
```

### 5. Push to GHCR

One-time auth setup — create a GitHub Personal Access Token (classic) with `write:packages` and `read:packages` scopes at https://github.com/settings/tokens, then:

```bash
echo "$GHCR_TOKEN" | docker login ghcr.io -u <your-github-username> --password-stdin
```

Push both tags:

```bash
docker push "$IMAGE:$TAG"
docker push "$IMAGE:latest"
```

After the **first** push of a new chain, the GHCR package is created as private. Set it to public at https://github.com/orgs/Sovereign-Labs/packages if external operators need to pull it. One-time per chain.

### 6. Create the GitHub Release

Write the release notes manually — auto-generated notes get confused across the chain branches.

```bash
gh release create "$TAG" \
  --title "$TAG" \
  --notes-file RELEASE_NOTES.md
```

Or create it in the GitHub UI at https://github.com/Sovereign-Labs/rollup-starter/releases/new, picking the tag you just pushed.

Suggested release-notes content:

- What changed since the previous release of this chain
- Breaking config changes operators need to apply
- Pull command:
  ````
  ```
  docker pull ghcr.io/sovereign-labs/rollup-starter/<chain>:<tag>
  ```
  ````

## Image layout

What's inside the image:

- `WORKDIR /rollup`
- `/usr/local/bin/rollup` — the binary
- `/rollup/configs/celestia/genesis.json` — baked in (chain-specific, immutable per release)
- `/rollup/rollup-state/` — declared as a volume; persistent state lives here
- Port `12346` exposed for the rollup HTTP API

What's **not** baked in:

- `rollup.toml` — holds secrets (Celestia signer key, RPC tokens) and deployment-specific values (RPC URLs, ports). Mounted at runtime.

The Docker build context excludes `configs/**/rollup.toml`, `.env` files, logs, and git metadata so operator secrets do not enter builder layers or cache.

## Running the released image

```bash
docker run --rm \
  --security-opt seccomp=unconfined \
  -v "$(pwd)/configs/celestia/rollup.toml:/rollup/configs/celestia/rollup.toml:ro" \
  -v rollup-data:/rollup/rollup-state \
  -p 12346:12346 \
  ghcr.io/sovereign-labs/rollup-starter/relay-chain:relay-chain-2026-04-24
```

Notes:

- `--security-opt seccomp=unconfined` is needed on hosts whose Docker seccomp profile blocks `io_uring`. If the daemon already runs containers with an unconfined profile, this flag is redundant but harmless.
- `rollup-data` is a Docker named volume. To put state at a known host path instead, use an absolute path as the source: `-v /var/lib/rollup-data:/rollup/rollup-state`.
- The bundled `configs/celestia/rollup.toml` works as-is for local builds, but production deployments should provide their own with real Celestia credentials.

## Reproducing a release locally

The Dockerfile and `Cargo.lock` are the source of truth for rebuilding a release image from a tagged commit:

```bash
git checkout "$TAG"
DOCKER_BUILDKIT=1 docker build -t rollup-starter:local .
```

This rebuild is not guaranteed to be bit-for-bit identical because the base image tags and Debian package indexes can move. For exact artifact identity, record and compare the published image digest:

```bash
docker pull "$IMAGE:$TAG"
docker inspect --format='{{index .RepoDigests 0}}' "$IMAGE:$TAG"
```

## Adding a new chain

No workflow or Dockerfile changes needed — pick a new chain prefix and tag with it (e.g., `some-other-chain-2026-05-11`). The build commands and image layout above apply unchanged; the image just lands at `ghcr.io/sovereign-labs/rollup-starter/some-other-chain` instead. Update the GHCR package visibility for the new chain after the first push.
