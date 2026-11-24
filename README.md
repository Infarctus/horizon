# Horizon Fork CI (`fork-ci`)

This orphan branch is the default branch of `Infarctus/horizon` solely to host fork-only GitHub Actions workflows without adding fork-specific commits to `main` or to upstream PR branches targeting `horizon-rust/horizon:main`.

## Parallel Job & Caching Architecture

Every dispatch runs a lightweight `scope` job (~15s), then fans out across `ubuntu-latest` runners. Concurrency is grouped per `ref` (`fork-pr-checks-<ref>`), so overlapping dispatches on the same branch cancel earlier runs instead of duplicating work or contending for GitHub's cache API.

- **`fast-checks`** (~1m, runs in separate named steps): public tree, repository hygiene, layers, addresses, hotspot lists (`check_lists.py`), hotspot stand-ins (`check_stand_ins.py`), nested locks, root & launcher rustfmt on changed files, and parallel Python unit tests.
- **`verify`** (`clippy` + `tests` + `launcher` + `mod-api` in parallel, ~12–15m): recommended while iterating.
- **`pr-ready`** (`pr-ready`, `full`, ~35m serial): gated on `fast-checks` by default (`fail_fast=true`, override with `-f fail_fast=false` or `-f force_pr_ready=true`). Runs `tools/pr_ready.py --base upstream/main`, writes the scrubbed `## Self-check` Markdown block to Job Summary, and uploads `pr-ready-summary.md`. Run once at the end when ready to open or finalize a PR.
- **`tests`** (`quick`, `full` with `--crates`/`--filter`): targeted `cargo check`/`cargo test` using the warm `tests` cache without overwriting it.
- **`mod-api`** (`--mod-api`, or auto in `verify`/`full` when mod API is touched): runs `tools/check_mod_api_unchanged.py --allow-added` in parallel, reading the shared `tests` Cargo cache without writing a separate key.
- **`build-fast`** (`--build-fast`, any mode): runs `cargo build --locked --profile fast -p horizon-game` in parallel on its own runner.

### Upstream Base Safety

`scope` fetches `horizon-rust/horizon` (`base`, default `main`) and fails closed if upstream is unreachable so diffs, touched-crate selection, and layer/mod-API baselines are never silently taken against a stale fork branch. Pass `-f allow_fork_base_fallback=true` (`--allow-fork-base`) only when upstream is known to be temporarily unavailable and `fork/<base>` is up to date; fallback runs print a prominent warning banner in the Job Summary.

### Caching (3 `rust-cache` Keys + Selective `sccache`)

To stay comfortably inside GitHub Actions' 10 GiB repository cache limit:

1. **`Swatinem/rust-cache`** uses three shared keys:
   - `fork-pr-checks-tests-v1` (shared by `tests`, `pr-ready`, and read-only by `mod-api` and `build-fast`)
   - `fork-pr-checks-clippy-v1` (`clippy`)
   - `fork-pr-checks-launcher-v1` (`launcher` and `pr-ready` when launcher changes)
2. **`mozilla-actions/sccache-action` (`SCCACHE_GHA_ENABLED=true`) via `tools/ci/compiler-cache.sh`** is enabled only on `tests`, `pr-ready`, `mod-api`, and `build-fast` (omitted on `clippy` and `launcher` where hit rates are near 0%).

## Usage

```sh
# Fast parallel iteration (fast-checks + clippy + tests + mod-api + launcher all in parallel)
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f mode=verify

# Standard PR self-check (fast-checks gate -> pr-ready; run once at the end for "## Self-check")
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch>

# Bypass fast-checks gate if debugging pr-ready output on a branch with fast-check failures
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f fail_fast=false

# Explicitly allow fallback to fork origin/main during an upstream GitHub outage
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f mode=verify -f allow_fork_base_fallback=true

# Quick targeted check (fast-checks + targeted crate check/test in parallel)
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f mode=quick -f crates="horizon-client-lib" -f test_filter="my_test"

# Full check (fast-checks -> pr-ready + parallel mod-api + optional --profile fast build)
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f mode=full -f build_fast=true
```
