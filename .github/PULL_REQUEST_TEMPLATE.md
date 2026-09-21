<!--
Title: use a Conventional Commits subject (`feat:`, `fix:`, `docs:`, `chore:`, `ci:`, …).
PRs are squash-merged, so this title becomes the commit subject and is what the
release tooling reads — see `AGENTS.md` for how this repo cuts releases.
Base branch: `main`.
-->

## Summary

<!-- 1–2 sentences: what this PR does and why. Link the issue it closes, e.g. "Fixes #42". -->

## Type

<!-- Tick exactly one. The `label-pr` workflow classifies from the title, this body and the
     diff, so ticking the right box here makes the label it applies deterministic.
     Priority is the bot's call — don't set it. -->

- [ ] `bug` — fixes broken behavior or incorrect results
- [ ] `feature` — adds a capability that did not exist before
- [ ] `enhancement` — improves an existing feature or behavior
- [ ] `docs` — documentation only
- [ ] `chore` — maintenance, infrastructure, CI, tooling, or dependencies

## Changes

<!-- What actually changed, grouped by area. Name the file or module a reviewer should read
     first, and call out anything renamed, moved or deleted. -->

## Testing

<!-- How you verified this. List the commands you actually ran and what they reported —
     not the ones you intended to run. If a gate was skipped, say so and why. -->

- [ ] The quality gates in `AGENTS.md` pass locally (that file lists this repo's commands)
- [ ] New or changed behavior is covered by tests, or this PR explains why it isn't

## Repository conventions

<!-- `AGENTS.md` is the contract for this repo — architecture, conventions, commands and lint
     rules. Claude Code loads it through the built-in `agents-md@builtin` plugin, which is why
     there is no `CLAUDE.md`. Read it before opening a PR; update it as part of one. -->

- [ ] Follows the conventions in `AGENTS.md` (naming, imports, error handling, validation)
- [ ] `AGENTS.md` is updated if this PR changes architecture, commands, conventions or lint rules

## Notes for reviewers

<!-- Trade-offs taken, deliberate omissions, follow-up work. Delete this section if empty. -->
