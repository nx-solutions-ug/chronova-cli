You MUST label PR $ARGUMENTS right now. Do NOT ask for more information — execute all steps immediately.

## Step 1: Read the PR

```bash
gh pr view $ARGUMENTS --json title,body,labels,author --jq '{title: .title, body: .body, labels: [.labels[].name], author: .author.login}'
```

## Step 2: Read the diff

```bash
gh pr diff $ARGUMENTS
```

## Step 3: Skip check

Check the PR's current labels (from Step 1). If the PR already has **both** a type label (`bug`, `feature`, `enhancement`, `docs`, `chore`) **and** a priority label (`priority: critical`, `priority: high`, `priority: medium`, `priority: low`), **STOP immediately** — do not apply labels, do not post any comment, do not proceed further.

Print a one-line skip message and exit:
```
Skipped PR #$ARGUMENTS: already has type and priority labels.
```

## Step 4: Ensure label taxonomy exists

Run `gh label create` for any labels that do not yet exist in the repository. Ignore `422` errors (label already exists):

```bash
gh label create bug --color d73a4a --description "Something isn't working" || true
gh label create feature --color a2eeef --description "New feature or request" || true
gh label create enhancement --color 84b6eb --description "Improvement to an existing feature" || true
gh label create docs --color 0075ca --description "Improvements or additions to documentation" || true
gh label create chore --color fef2c0 --description "Maintenance, infra, or tooling" || true
gh label create "priority: critical" --color e11d48 --description "Production down or security vulnerability" || true
gh label create "priority: high" --color fb923c --description "Major impact, common workflow broken" || true
gh label create "priority: medium" --color fbbf24 --description "Normal priority, workaround exists" || true
gh label create "priority: low" --color 22c55e --description "Edge case or minor impact" || true
```

## Step 5: Classify the PR

Using the PR title, description, and diff, determine:

**Type** (apply exactly one):

| Label | When to apply |
|---|---|
| `bug` | Fixes broken behavior or produces incorrect results |
| `feature` | Adds a new capability that did not exist before |
| `enhancement` | Improves an existing feature or behavior |
| `docs` | Documentation changes only |
| `chore` | Maintenance, infrastructure, CI, tooling, or dependency updates |

**Priority** (apply exactly one):

| Label | When to apply |
|---|---|
| `priority: critical` | Production is down, data loss, or security vulnerability |
| `priority: high` | Major feature broken, significant user impact, no workaround |
| `priority: medium` | Normal change, workaround exists or impact is limited |
| `priority: low` | Nice-to-have, cosmetic, or minor improvement |

## Step 6: Apply labels

Apply labels using `gh pr edit`. **Never remove existing labels** — only add.

```bash
gh pr edit $ARGUMENTS --add-label "<type>,<priority>"
```

Example: `gh pr edit $ARGUMENTS --add-label "bug,priority: medium"`

## Step 7: Print summary

Print a single summary line:
```
Labeled PR #$ARGUMENTS with <type> and <priority>.
```

## Rules

- **MUST** skip if both a type label and a priority label are already present.
- **MUST NOT** remove any existing labels.
- **MUST** apply exactly one type label and one priority label.
- **MUST NOT** post comments to the PR — not when skipping, not when labeling, not ever.
- **MUST NOT** expand scope beyond labeling (no code changes, no assignment suggestions, no review comments).
- **MUST NOT** push commits or modify any files.