# Horizon Fork CI (`fork-ci`)

This orphan branch is the default branch of `Infarctus/horizon` solely to host fork-only GitHub Actions workflows without adding fork-specific commits to `main` or to upstream PR branches targeting `horizon-rust/horizon:main`.

## Usage

Push your PR branch to this fork, then dispatch `.github/workflows/fork-pr-checks.yml`:

```sh
# Standard PR self-check (runs tools/pr_ready.py --base upstream/main)
# Writes the ready-to-paste "## Self-check" block to the Job Summary and uploads pr-ready-summary.md
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch>

# Quick check (public tree, layers, addresses, nested locks, rustfmt on changed files, cargo check + targeted tests)
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f mode=quick -f crates="horizon-client-lib" -f test_filter="my_test"

# Full check (pr_ready + automatic mod API check when mod API is touched; optional --profile fast build)
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f mode=full
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f mode=full -f build_fast=true

# PR self-check + forced mod API check
gh workflow run fork-pr-checks.yml --repo Infarctus/horizon -f ref=<branch> -f run_mod_api=true
```

## Notes

- Runs on standard free GitHub-hosted `ubuntu-latest` runners (no Blacksmith runners required).
- Uses `Swatinem/rust-cache@v2` (`shared-key: fork-pr-checks-v1`, `cache-on-failure: false`, saving only on `pr-ready`/`full` runs that touch Rust) so PR branches share one warm dependency cache without partial-build poisoning.
- Upstream PR branches never merge or reference `fork-ci`.
- Parity harnesses requiring private Mojang `.local/` vanilla data cannot run here; run them locally or on upstream PR CI.
