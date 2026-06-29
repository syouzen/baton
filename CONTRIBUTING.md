# Contributing

Baton uses a two-branch flow with GitHub pull requests.

## Branches

- `main`: stable release branch. Changes land here through release or stabilization PRs only.
- `develop`: integration branch for completed slice work.
- Feature branches: short-lived branches created from `develop`, for example `feat/issue-8-tauri-shell`, `fix/parser-resize`, or `docs/workflow`.

## PR flow

1. Start from a clean, current `develop` branch.

   ```bash
   git fetch origin
   git switch develop
   git pull --ff-only origin develop
   git switch -c feat/<topic>
   ```

2. Keep each branch focused on one issue or tightly scoped change.
3. Verify locally before opening a PR:

   ```bash
   cargo fmt --check
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test --all -- --nocapture
   cargo check
   ```

4. Open the PR against `develop` and wait for the required `Rust core` CI check.
5. Squash-merge green PRs and delete the feature branch.
6. For releases or stabilization, open a PR from `develop` to `main`. `main` is protected and requires PR-based changes plus the `Rust core` check.

## Direct pushes

Do not push directly to `main`. Use PRs so CI and review gates stay visible. Direct pushes to `develop` should also be avoided for normal work; use feature branches and PRs unless repository recovery requires otherwise.

## Issue closure

When PRs merge into `develop`, explicitly close the related GitHub issue if the `Closes #N` keyword does not close it automatically.
