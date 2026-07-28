---
type: reference
title: Development Automation
description: CI workflows, bot commands, release tooling, and contribution gating for the Chronova CLI repository.
tags: [development, automation, ci, workflows, bots]
---

# Development Automation

The repository uses GitHub Actions and agent prompts under `.omp/` to automate issue triage, PR review, releases, and contribution gating. This automation is separate from the Chronova CLI runtime.

## OMP CI

`.github/workflows/omp-ci.yml` runs three jobs on issues and PRs:

- **`triage-issue`** — triggered when an issue is opened, runs the `triage-issue` OMP command, then dispatches an `issue-triaged` event for the fix-issue workflow.
- **`label-pr`** — runs on new and updated PRs to apply type and priority labels via the `label-pr` OMP command.
- **`review-pr`** — runs on PRs to perform automated code review via the `review-pr` OMP command.

All three jobs authenticate with a GitHub App token, install the OMP CLI, and call an `ollama-cloud/minimax-m3` model.

## Automated PR review

The review prompt is defined in `.omp/commands/review-pr.md`. It:

1. Checks whether the bot (`chronova-agent` or `omp-agent`) has already reviewed the PR under the pulls API.
2. Fetches unresolved inline threads and deduplicates new findings against existing ones.
3. Maps findings to actual diff lines and submits reviews via the `gh-pr-review` extension, including code suggestions.

The deduplication query covers reviews in the states `CHANGES_REQUESTED`, `COMMENTED`, and `APPROVED` so that threads from prior approvals are also considered before posting new comments.

## Contribution gating

Only vouched contributors can open pull requests. The gating system is implemented by:

- `.github/VOUCHED.td` — the vouched/denounced user list.
- `.github/workflows/vouch-pr.yml` — gates PRs using `mitchellh/vouch` and labels vouched PRs.
- `.github/workflows/vouch-manage.yml` — updates the vouched list from discussion comments (`!vouch`, `!denounce`, `!unvouch`).

See `CONTRIBUTING.md` in the repository root for the contributor-facing process.

## Other workflows

- `.github/workflows/test.yml` — runs `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test --verbose` on Ubuntu, macOS, and Windows.
- `.github/workflows/release.yml` — uses semantic-release tooling in `.github/release-tooling/` to determine the next version, bump `Cargo.toml`, build cross-platform binaries, and publish GitHub releases.
- `.github/workflows/update-wiki.yml` — runs the Wiki Agent on pushes to `main` and on a daily schedule to update this documentation.
- `.github/workflows/auto-manage.yml` — adds `needs-triage` to new/reopened issues and auto-assigns new issues and PRs to `niklasschaeffer`.

## Agent conventions

`AGENTS.md` in the repository root defines coding conventions for agents and human contributors, including error handling, async patterns, database operations, configuration precedence, API compatibility, and the testing checklist.

## Related pages

- [Development index](index.md)
- [Architecture Overview](../architecture/overview.md)
