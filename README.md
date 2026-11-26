# Horizon Fork CI (`fork-ci`)

This orphan branch is the default branch of `Infarctus/horizon` solely to host fork-only GitHub Actions workflows without adding fork-specific commits to `main` or to upstream PR branches targeting `horizon-rust/horizon:main`.

## Parallel Job & Caching Architecture

Every dispatch runs a lightweight `scope` job (~15s), then fans out across `ubuntu-latest` runners. Concurrency is grouped per `ref` (`fork-pr-checks-<ref>`), so overlapping dispatches on the same branch cancel earlier runs instead of duplicating work or contending for GitHub's cache API.

- **`fast-checks`** (~1m, non-short-circuiting named steps with a Job Summary table): `Cargo.lock`-without-`Cargo.toml`, public tree (working tree + commit range), repository hygiene, layers, addresses, root & launcher rustfmt on changed files (respecting per-crate manifest editions), nested locks, hotspot lists (`check_lists.py`), hotspot stand-ins (`check_stand_ins.py`), and parallel Python unit tests. Skip in any mode with `-f skip_fast_checks=true`.
- **`verify`** (`clippy` + `tests` + `launcher` + `mod-api` in parallel, ~12–15m): recommended while iterating. `clippy` checks both the full workspace and the `--no-default-features` server crates; `tests` runs lib/bin/integration (parallel via `run_test_binaries.py --jobs 2` on `--workspace`), doc tests, and conditional `tokens.css`, `dispatch-tests`, and `horizon-mod-registry` checks.
- **`pr-ready`** (`pr-ready`, `full`, ~30–40m serial): gated on `fast-checks` by default (`fail_fast=true`). Note the difference between bypass flags:
  - `-f skip_fast_checks=true`: skips `fast-checks` in **any** mode (`verify`, `quick`, `pr-ready`, `full`) and starts downstream jobs immediately.
  - `-f force_pr_ready=true`: skips `fast-checks` in `pr-ready`/`full` mode and starts `pr-ready` immediately.
  - `-f fail_fast=false`: still runs `fast-checks` first (~1–2m), then runs `pr-ready` even if `fast-checks` failed.
  Runs `tools/pr_ready.py --base upstream/main` under an 80m soft timeout, writes the scrubbed `## Self-check` Markdown block to Job Summary, and uploads `pr-ready-summary.md`.
- **`tests`** (`quick`, `full` with `--crates`/`--filter`): targeted check/test using the warm root cache without overwriting it (skips `--all-targets` check when a test filter is given).
- **`mod-api`** (`--mod-api`, or auto in `verify`/`full` when mod API is touched): runs `tools/check_mod_api_unchanged.py --allow-added` in parallel, reading the shared root cache without writing.
- **`build-fast`** (`--build-fast`, any mode): runs `cargo build --locked --profile fast -p horizon-game` in parallel on its own runner.

### Upstream Base Fallback & Deterministic Pinning

`scope` tries to fetch `horizon-rust/horizon` (`base`, default `main`) first and automatically falls back to fork `origin/<base>` with a prominent warning banner in the Job Summary if upstream cannot be fetched (`allow_fork_base_fallback=true` by default). Pass `-f allow_fork_base_fallback=false` (`--strict-upstream`) to fail closed instead. Downstream jobs pin `refs/remotes/upstream/<base>` directly to `scope`'s `merge_base` SHA so there are no duplicate network fetches or mid-run base drift.

### Caching (2 `Swatinem/rust-cache` Keys, No `sccache` GHA Quota Thrashing)

To stay comfortably inside GitHub Actions' 10 GiB repository cache limit and maximize reuse between `verify` and `pr-ready`:

1. **`fork-pr-checks-root-v2`**: shared by `pr-ready`, `clippy`, `tests`, and read-only by `mod-api` and `build-fast` (`cache-on-failure: true` on all writers). Uniform `RUSTFLAGS` (`-C linker-features=-lld -C link-arg=-fuse-ld=mold`) without `-Zthreads`/`RUSTC_BOOTSTRAP` keeps cache keys identical across jobs and keeps Python tests green.
2. **`fork-pr-checks-launcher-v2`**: shared by `launcher` and `pr-ready` when launcher dependencies change (`cache-bin: false`, `cache-on-failure: true`).
3. **`SCCACHE_GHA_ENABLED` is intentionally off** so hundreds of small per-crate objects do not evict the multi-GiB `Swatinem/rust-cache` tarballs via GitHub's 10 GiB LRU limit.

## Usage

```sh
# Fast parallel iteration (fast-checks + clippy + tests + mod-api + launcher all in parallel)
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f mode=verify

# Skip only fast-checks in any mode (e.g. verify or full)
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f mode=verify -f skip_fast_checks=true

# Standard PR self-check (fast-checks gate -> pr-ready; run once at the end for "## Self-check")
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch>

# Fail closed if upstream horizon-rust/horizon cannot be fetched
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f mode=verify -f allow_fork_base_fallback=false

# Quick targeted test (fast-checks + single filtered test)
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f mode=quick -f crates="horizon-client-lib" -f test_filter="my_test"

# Full check (pr-ready + mod-api + optional --profile fast build)
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f mode=full -f build_fast=true
```
