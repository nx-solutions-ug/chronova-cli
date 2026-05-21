You MUST triage issue $ARGUMENTS right now. Do NOT ask for more information — execute all steps immediately.

## Step 1: Read the issue

Fetch the issue title, body, existing labels, and **all comments** using `gh`. Comments often contain critical context:

```bash
gh issue view $ARGUMENTS --json title,body,labels,comments --jq '{title: .title, body: .body, labels: [.labels[].name], comments: [.comments[] | {author: .author.login, body: .body}]}'
```

## Step 2: Skip check

Check the issue's current labels (from Step 1). If **any** of the following are true, **STOP immediately**:

- The issue already has **both** a type label (`bug`, `feature`, `enhancement`, `docs`, `chore`) **and** a priority label (`priority: critical`, `priority: high`, `priority: medium`, `priority: low`).
- The issue has the `accepted` status label.

Print a one-line skip message and exit:
```
Skipped issue #$ARGUMENTS: already has type + priority labels, or is accepted.
```

## Step 3: Ensure label taxonomy exists

Run `gh label create` for any labels that do not yet exist. Ignore `422` errors:

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
gh label create needs-triage --color 7c8f80 --description "Awaiting initial classification" || true
gh label create needs-info --color d876e3 --description "Needs more information from the reporter" || true
gh label create accepted --color 0e8a16 --description "Accepted by maintainers" || true
```

## Step 4: Classify the issue

Using the issue title, body, **and all comments**, determine:

**Type** (apply exactly one):

| Label | When to apply |
|---|---|
| `bug` | Existing behavior is broken or produces incorrect results |
| `feature` | Request for a new capability that does not yet exist |
| `enhancement` | Request to improve an existing feature |
| `docs` | Documentation is missing, incorrect, or unclear |
| `chore` | Maintenance, infrastructure, CI, tooling, or dependency work |

**Priority** (apply exactly one):

| Label | When to apply |
|---|---|
| `priority: critical` | Production is down, data loss, or security vulnerability |
| `priority: high` | Major feature broken, common workflow blocked, no workaround |
| `priority: medium` | Normal issue, workaround exists or impact is limited |
| `priority: low` | Edge case, cosmetic issue, minor inconvenience |

**Status** (apply exactly one):

- Apply `needs-triage` if the issue is a standard report needing maintainer review.
- Apply `needs-info` **instead** of `needs-triage` if the issue is clearly a question (how-to, clarification, support request) rather than a bug/feature report.
- Do **not** apply `accepted` — that is for maintainers to add later.

## Step 5: Apply labels

Apply labels using `gh issue edit`. **Never remove existing labels** — only add.

```bash
gh issue edit $ARGUMENTS --add-label "<type>,<priority>,<status>"
```

Example: `gh issue edit $ARGUMENTS --add-label "bug,priority: medium,needs-triage"`

## Step 6: Print summary

Print a single summary line:
```
Triaged issue #$ARGUMENTS: <type> + <priority> + <status>.
```

## Rules

- **MUST** skip if both a type label and a priority label are already present, or if the issue has `accepted`.
- **MUST NOT** remove any existing labels.
- **MUST** read **all** comments before classifying — the body alone is often insufficient.
- **MUST** apply exactly one type label, one priority label, and one status label.
- **MUST NOT** post comments to the issue.
- **MUST NOT** expand scope beyond triage (no code changes, no assignment suggestions, no duplicate linking).
- **MUST NOT** push commits or modify any files.