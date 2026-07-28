---
type: reference
title: "Automation & CI"
description: "GitHub Actions, OMP agent workflows, release tooling, and repository automation that drive Chronova CLI development."
tags: [ci, github-actions, omp, automation, development]
---

# Automation & CI

Chronova CLI uses GitHub Actions for testing, releases, and repository hygiene. It also runs an OMP (OpenCode/OMP) agent that reacts to `/omp` comments, triages issues, labels pull requests, and performs code reviews.

## Test workflow

`.github/workflows/test.yml` runs on every push to `main`/`master` and on pull requests against those branches. It executes on Ubuntu, macOS, and Windows:

1. `cargo fmt -- --check`
2. `cargo clippy -- -D warnings`
3. `cargo test --verbose`

Pull requests must pass these gates before they can be merged.

## OMP agent trigger

`.github/workflows/omp.yml` fires when a non-bot user posts an issue or PR comment that starts with `/omp`. The job:

- Adds an 👀 `eyes` reaction to the comment to acknowledge the trigger.
- Parses the comment body:
  - If the prompt matches a command name in `.omp/commands/<name>.md`, that command file is loaded and `$ARGUMENTS` is expanded.
  - Otherwise it treats the prompt as freeform. For PR comments, the prompt is appended with `_pr-commit-push.md` instructions so the agent commits and pushes changes to the PR branch.
- Installs the OMP CLI, authenticates against `ollama-cloud`, and runs `omp -p --model ollama-cloud/minimax-m3 --mode json`.
- Streams the agent output through `.omp/stream-log.py`.

The workflow uses a GitHub App token (`secrets.APP_CLIENT_ID` and `secrets.APP_PRIVATE_KEY`) so it can push branches, post comments, and react.

## OMP CI jobs

`.github/workflows/omp-ci.yml` runs automatically on new issues and pull requests. It contains three jobs:

| Job | Trigger | Purpose |
|-----|---------|---------|
| `triage-issue` | `issues: opened` | Labels and classifies the issue, then dispatches `omp-fix-issue.yml` via `repository_dispatch` event `issue-triaged`. |
| `label-pr` | `pull_request: opened, synchronize, ready_for_review` | Skips if the PR already has a type and priority label; otherwise runs `.omp/commands/label-pr.md` to apply labels. |
| `review-pr` | `pull_request: opened, synchronize, ready_for_review` | Skips re-review when the latest synchronize commit is from a known agent or bot. Otherwise runs `.omp/commands/review-pr.md`. |

All three jobs also authenticate with `ollama-cloud/minimax-m3` and add an 👀 reaction to the issue or PR before processing.

## Issue fix workflow

`.github/workflows/omp-fix-issue.yml` is triggered by the `issue-triaged` repository dispatch event (or manually via `workflow_dispatch`). It checks out the repository, reads `.omp/commands/fix-issue.md`, expands `$ARGUMENTS` with the issue number, and runs the OMP agent to produce a fix.

## Repository management

`.github/workflows/auto-manage.yml` handles routine project housekeeping:

- New or reopened issues are tagged with `needs-triage`.
- New issues and PRs are auto-assigned to `niklasschaeffer`.

## Vouch gate

Pull requests are gated by `mitchellh/vouch`:

- `.github/workflows/vouch-pr.yml` runs on `pull_request_target` and adds a `vouched` label when the PR passes the gate. Only vouched users, collaborators with write access, and bots can open PRs.
- `.github/workflows/vouch-manage.yml` watches discussion comments for `!vouch`, `!denounce`, and `!unvouch` commands from maintainers and updates `.github/VOUCHED.td`, the canonical list of vouched users.

## Release and wiki automation

- `.github/workflows/release.yml` builds cross-platform binaries and publishes GitHub releases. It is driven by the semantic-release configuration in `.github/release-tooling/release.config.js`.
- `.github/workflows/update-wiki.yml` runs the `@chronova/wiki-agent` on pushes to `main` and on a daily schedule. When wiki content changes, it flattens `.wiki/` into the wiki repository and opens a staging pull request on `wiki/staging-<timestamp>`.

## Local agent configuration

`.omp/config.yml` configures the OMP agent:

- Default model provider: `ollama-cloud/devstral-2:123b`
- Build/explore/general agent models are also pinned there.
- Project instructions are loaded from `AGENTS.md`.

Command prompts live under `.omp/commands/` and shared rules live under `.omp/rules/`. The agent logs are streamed by `.omp/stream-log.py`, which reads JSON lines from OMP and emits GitHub workflow log groups.

## Related pages

- [Development index](./index.md)
- [Architecture Overview](../architecture/overview.md)
- [Operations](../operations/index.md)
