<!-- These comments never render. Keep the body short: someone with no context should
     understand what changes, and why, in under a minute. -->

# Replace this heading with a one-line description of the issue

<!-- ↑ The issue, not the fix. e.g. "# The review bot judges tooling it cannot run"

     ↓ Then two or three sentences on what this PR covers, for someone who reads no
     further: the areas it touches, and what it deliberately leaves alone.
     Flowing prose, not bullets — the bullets start at Why.                       -->

## Intent

### Why

<!-- One bullet per reason, each with evidence a reviewer can check: a finding, an
     incident, an issue, a version, a link. The diff shows what changed and never why,
     so this is the half that decides whether the change is right.
     e.g. - #812 reports a 422 on any value outside the nine chips; both the issue and
            the endpoint's Zod schema specify free text.                          -->

-

### What

<!-- One bullet per change: what it DOES and where (app, area, module). Not a file list —
     the diff already has that, and not a restatement of Why. Last bullet: anything
     deliberately left out.
     e.g. - `src/lib/profile`: `profession` becomes free text; the nine values stay as
            suggestions.
          - Not included: moderation of the typed text — that is #824.            -->

-

## Verification

<!-- The real commands and the real numbers. Name the gates you ran from `AGENTS.md`
     (lint, format:check, type-check, test) and what they actually reported — if you ran
     a subset, say which and why. No E2E is configured in this repo, so a UI change says
     how it was exercised by hand instead. Close with what you did NOT verify: a gap
     named beats a claim unchecked. -->

## Test Plan

<!-- Boxes a reviewer ticks before merging; an unticked one is a question worth asking. -->

- [ ] CI passes
- [ ]
