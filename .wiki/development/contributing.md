---
type: guide
title: Contributing
description: How to contribute to Chronova CLI, including the lightweight vouch system that gates pull requests.
tags: [contributing, vouch, pr, development]
---

# Contributing

Pull requests are welcome from vouched contributors. This repository uses a lightweight vouch system to gate PRs while keeping the process simple.

## Quick contribution path

1. Open a discussion in the [Discussions tab](https://github.com/nx-solutions-ug/chronova-cli/discussions) describing what you want to contribute.
2. A maintainer will review the discussion and vouch you by commenting `!vouch`.
3. Once vouched, open pull requests normally.

## Who can open PRs without being vouched

- **Bots** whose account name ends with `[bot]` (for example, `renovate[bot]` and `chronova-agent[bot]`).
- **Collaborators with write access** to the repository.

These accounts bypass the vouch gate automatically.

## Maintainer vouch commands

Maintainers can manage the vouch list by commenting on a discussion. Available commands (maintainers with admin, maintain, or write roles only):

| Command | Effect |
| --- | --- |
| `!vouch` | Vouch the author of the discussion |
| `!vouch @user [reason]` | Vouch a specific user |
| `!denounce @user [reason]` | Block a user from contributing |
| `!unvouch @user` | Remove a user from the vouched list |

The vouch list itself lives in `.github/VOUCHED.td`.

## How PRs are gated

The workflow in `.github/workflows/vouch-pr.yml` runs on `pull_request_target` events. It checks whether the PR author is vouched, allowed, or a bot/collaborator. PRs from unvouched or denounced users are auto-closed. Vouched PRs receive the `vouched` label.

## See also

- `.github/VOUCHED.td` — the canonical vouched-users list
- `.github/workflows/vouch-pr.yml` — PR gate workflow
- `.github/workflows/vouch-manage.yml` — discussion-based vouch management workflow
