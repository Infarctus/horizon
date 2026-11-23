# Horizon Fork CI (`fork-ci`)

This orphan branch is the default branch of `Infarctus/horizon` solely to host fork-only GitHub Actions workflows without adding fork-specific commits to `main` or to upstream PR branches targeting `horizon-rust/horizon:main`.

## Parallel Job Architecture

Every dispatch runs a lightweight `scope` job (~15s), then fans out across parallel `ubuntu-latest` runners:

- **`fast-checks`** (~60s, always): public tree, layers, addresses, nested locks, rustfmt on changed files, Python unit tests.
- **`pr-ready`** (`pr-ready`, `full`): runs `tools/pr_ready.py --base upstream/main`, writes the scrubbed `## Self-check` Markdown block to Job Summary, uploads `pr-ready-summary.md`, and warms the shared `fork-pr-checks-v1` Cargo cache.
- **`clippy`** + **`tests`** + **`launcher`** (`verify`): splits workspace/server clippy, touched crates' tests, and launcher checks across separate parallel runners for faster iteration.
- **`tests`** (`quick`, `full` with `--crates`/`--filter`): targeted `cargo check`/`cargo test` using the warm cache without overwriting it.
- **`mod-api`** (`--mod-api`, or auto in `verify`/`full` when mod API is touched): runs `tools/check_mod_api_unchanged.py --allow-added` in parallel with its own compact cache.
- **`build-fast`** (`--build-fast`, any mode): runs `cargo build --locked --profile fast -p horizon-game` in parallel on its own runner.

## Usage

```sh
# Standard PR self-check (fast-checks + pr-ready in parallel; emits ready-to-paste "## Self-check")
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch>

# Fast parallel iteration (fast-checks + clippy + tests + mod-api + launcher all in parallel)
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f mode=verify

# Quick targeted check (fast-checks + targeted crate check/test in parallel)
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f mode=quick -f crates="horizon-client-lib" -f test_filter="my_test"

# Full check (fast-checks + pr-ready + mod-api + optional --profile fast build, all in parallel)
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f mode=full -f build_fast=true
```
