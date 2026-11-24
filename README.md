# Horizon Fork CI (`fork-ci`)

This orphan branch is the default branch of `Infarctus/horizon` solely to host fork-only GitHub Actions workflows without adding fork-specific commits to `main` or to upstream PR branches targeting `horizon-rust/horizon:main`.

## Parallel Job & Caching Architecture

Every dispatch runs a lightweight `scope` job (~15s), then fans out across `ubuntu-latest` runners:

- **`fast-checks`** (~2m, always): public tree, repository hygiene, layers, addresses, export lists (`check_lists.py`), test stand-ins (`check_stand_ins.py`), nested locks, rustfmt on changed files, and parallel Python unit tests.
- **`pr-ready`** (`pr-ready`, `full`): gated on `fast-checks` by default (`fail_fast=true`, override with `-f fail_fast=false`) so formatting/hygiene/Python failures abort in ~2m instead of burning 30m. Runs `tools/pr_ready.py --base upstream/main`, writes the scrubbed `## Self-check` Markdown block to Job Summary, uploads `pr-ready-summary.md`, and warms its dedicated `fork-pr-checks-pr-ready-v1` Cargo cache (`cache-on-failure: true`).
- **`clippy`** + **`tests`** + **`launcher`** (`verify`): runs in parallel with `fast-checks` across separate runners for fastest iteration, with dedicated `fork-pr-checks-{clippy,tests,launcher}-v1` caches (`cache-on-failure: true`).
- **`tests`** (`quick`, `full` with `--crates`/`--filter`): targeted `cargo check`/`cargo test` using the warm `tests` cache without overwriting it.
- **`mod-api`** (`--mod-api`, or auto in `verify`/`full` when mod API is touched): runs `tools/check_mod_api_unchanged.py --allow-added` in parallel with its own `fork-pr-checks-mod-api-v1` cache.
- **`build-fast`** (`--build-fast`, any mode): runs `cargo build --locked --profile fast -p horizon-game` in parallel on its own runner.

### Two-Tier Caching

1. **`Swatinem/rust-cache`** restores `~/.cargo/registry` and compiled third-party crates as a single tarball in ~10s per job.
2. **`mozilla-actions/sccache-action` (`SCCACHE_GHA_ENABLED=true`) via `tools/ci/compiler-cache.sh`** caches individual workspace and `third_party/*` crate compilations across branches and jobs using content hashes, while bypassing `clippy-driver` cleanly and falling back to uncached compilation on transient cache errors.

## Usage

```sh
# Standard PR self-check (fast-checks gate -> pr-ready; emits ready-to-paste "## Self-check")
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch>

# Bypass fast-checks gate if debugging pr-ready output on a branch with fast-check failures
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f fail_fast=false

# Fast parallel iteration (fast-checks + clippy + tests + mod-api + launcher all in parallel)
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f mode=verify

# Quick targeted check (fast-checks + targeted crate check/test in parallel)
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f mode=quick -f crates="horizon-client-lib" -f test_filter="my_test"

# Full check (fast-checks -> pr-ready + parallel mod-api + optional --profile fast build)
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f mode=full -f build_fast=true
```
