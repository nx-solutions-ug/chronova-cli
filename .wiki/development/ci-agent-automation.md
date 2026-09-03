---
type: guide
title: "CI and Agent Automation"
description: "GitHub Actions workflows and the OMP-based agent tooling that
  tests, reviews, triages, and releases Chronova CLI."
tags: [ ci, github-actions, omp, code-review, automation, development ]
last_updated: "2026-09-03T14:04:14.026Z"
updated_by: "wiki-agent"
---

# CI and Agent Automation

Chronova CLI uses GitHub Actions for two distinct layers of automation:

1. **Deterministic CI** — formatting, linting, testing, releases, and repo housekeeping.
2. **OMP agent automation** — AI-driven PR review, dependency review, issue triage, labeling, and issue fixing, all executed by [OMP](https://omp.sh) agents configured in [`.omp/`](https://github.com/nx-solutions-ug/chronova-cli/tree/main/.omp).

All workflow files live in [`.github/workflows/`](https://github.com/nx-solutions-ug/chronova-cli/tree/main/.github/workflows).

## Deterministic workflows

| Workflow | File | Trigger | Purpose |
|----------|------|---------|---------|
| Test | `test.yml` | Push/PR to `main`/`master` | Runs `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test` on a matrix of `ubuntu-latest`, `macos-latest`, and `windows-latest`. Mirrors the checklist in `AGENTS.md`. |
| Build and Release | `release.yml` | `workflow_dispatch` | Version determination via semantic-release tooling in `.github/release-tooling/`, then builds and publishes releases. Optional `force_version` and `prerelease` inputs. |
| Auto Manage | `auto-manage.yml` | Issue opened/reopened, PR opened | Tags new issues with `needs-triage` and auto-assigns new issues/PRs. |
| Vouch | `vouch-manage.yml`, `vouch-pr.yml` | Issue/PR events | Vouch-based contributor trust management (see `.github/VOUCHED.td`). |
| Update Wiki | `update-wiki.yml` | Repo events | Runs wiki-agent to regenerate the documentation under `.wiki/`; produces the staging PRs reviewed before publication. |

## OMP agent workflows

The AI automation is built on OMP with Ollama Cloud models. Every OMP workflow follows the same boot sequence: install OMP from `https://omp.sh/install`, authenticate by inserting an `ollama-cloud` API key (from the `OLLAMA_API_KEY` secret) into OMP's SQLite credentials store, refresh the model list with `omp models refresh ollama-cloud`, then run a prompt from `.omp/commands/` with streaming logs rendered by `.omp/stream-log.py`.

### Code review: `omp-code-review.yml`

The code review lives in its own dedicated workflow (split out from the other OMP automation) with two jobs:

- **`dependency-review`** — fires only for `renovate[bot]` / `dependabot[bot]` PRs. Runs the `.omp/commands/dependency-review.md` prompt to research changelogs and assess breaking changes, then posts a review or comment. The job fails if no review or comment was posted, so silent agent failures surface as red CI.
- **`code-review`** — fires for human- and agent-authored PRs on `opened`, `synchronize`, `ready_for_review`, and `review_requested`, plus follow-ups to reviews/comments from Google's Jules bot. Uses the `.omp/commands/review-pr.md` prompt. Notable behaviors:
  - Skips re-review when the pushed commits were authored by automation (`opencode`, `github-actions`, `omp-agent`, `chronova-agent`) — an explicit `review_requested` from the GitHub UI always retriggers.
  - Detects Jules involvement (authored PRs, body markers, submitted reviews, suggestion comments) and passes that context to the review prompt via `IS_JULES` / `JULES_CONTEXT`.
  - Verifies a review or comment was actually posted and fails the job otherwise — except when the PR modifies `omp-code-review.yml` itself, which is skipped by design.

Both jobs use `ollama-cloud/glm-5.3-flash:max` and share a workflow-level per-PR concurrency group with `cancel-in-progress` only for `pull_request`/`workflow_dispatch` events; review-event runs queue behind an in-flight run instead of cancelling it.

### Triage, labeling, and fixing

- **`omp-ci.yml`** — `triage-issue` runs the `.omp/commands/triage-issue.md` prompt on newly opened issues and dispatches an `issue-triaged` repository event; `label-pr` runs `.omp/commands/label-pr.md` on opened/ready-for-review PRs, skipping PRs that already carry a type label (`bug`, `feature`, ...) and a priority label.
- **`omp-fix-issue.yml`** — triggered by the `issue-triaged` dispatch or manually; runs `.omp/commands/fix-issue.md` to implement a fix and open a PR.
- **`omp.yml`** — comment-driven agent: any non-bot comment containing `/omp` on an issue or PR review thread invokes the agent on that thread.

### Model configuration

Model selection is version-controlled in two files:

- `.omp/config.yml` — top-level model (`ollama-cloud/devstral-2:123b`) plus per-agent overrides and instruction files (`AGENTS.md`).
- `.omp/agent/config.yml` — per-role model map (`default`, `task`, `commit`, `plan`, `designer`, `vision`, `smol`, `slow`).

Prompt templates for each command are in `.omp/commands/`, and shared agent rules (e.g. idempotent label handling, tool path conventions) are in `.omp/rules/`.

## Where to make changes

- Changing review behavior: edit `.omp/commands/review-pr.md` (prompt) and/or the `code-review` job gating in `omp-code-review.yml`.
- Changing which models the agents use: edit `.omp/config.yml` and `.omp/agent/config.yml`.
- Adding a new deterministic check: extend `test.yml`; keep it aligned with the `AGENTS.md` testing checklist (`cargo test`, `cargo clippy -- -D warnings`, `cargo fmt`).
