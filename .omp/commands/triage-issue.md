You MUST triage issue $ARGUMENTS right now. Do NOT ask for more information — execute all steps immediately. React with the 👀 emoji to the triggering comment.

## Step 1: Read the issue

Fetch the issue title, body, existing labels, issue type, priority field, and **all comments** using `gh`. Comments often contain critical context:

```bash
gh issue view $ARGUMENTS --json title,body,labels --jq '{title: .title, body: .body, labels: [.labels[].name]}'
```

Read the issue type and priority field values:

```bash
gh api repos/{owner}/{repo}/issues/$ARGUMENTS -H "X-GitHub-Api-Version: 2026-03-10" --jq '{type: .type.name, priority: (.priority // null)}'
```

Read the issue field values (for priority):

```bash
gh api repos/{owner}/{repo}/issues/$ARGUMENTS/issue-field-values -H "X-GitHub-Api-Version: 2026-03-10" --jq '[.[] | {field_id: .issue_field_id, value: .single_select_option.name}]'
```

Read all comments:

```bash
gh issue view $ARGUMENTS --json comments --jq '[.comments[] | {author: .author.login, body: .body}]'
```

## Step 2: Skip check

Check the issue's current type field, priority field value, and labels (from Step 1). If **any** of the following are true, **STOP immediately**:

- The issue has an **issue type** set (e.g. Bug, Feature, Task) **and** a **priority field value** set (Urgent, High, Medium, Low).
- The issue has the `accepted` status label.

Print a one-line skip message and exit:
```
Skipped issue #$ARGUMENTS: already has type + priority fields, or is accepted.
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

**Type** — choose one issue type AND one label:

| Issue Type | Label | When to apply |
|---|---|---|
| `Bug` | `bug` | Existing behavior is broken or produces incorrect results |
| `Feature` | `feature` | Request for a new capability that does not yet exist |
| `Task` | `enhancement` | Request to improve an existing feature |
| `Task` | `docs` | Documentation is missing, incorrect, or unclear |
| `Task` | `chore` | Maintenance, infrastructure, CI, tooling, or dependency work |

**Priority** — choose one priority field value AND one label:

| Priority Value | Label | When to apply |
|---|---|---|
| `Urgent` | `priority: critical` | Production is down, data loss, or security vulnerability |
| `High` | `priority: high` | Major feature broken, common workflow blocked, no workaround |
| `Medium` | `priority: medium` | Normal issue, workaround exists or impact is limited |
| `Low` | `priority: low` | Edge case, cosmetic issue, minor inconvenience |

**Status** — apply exactly one label:
- Apply `needs-triage` if the issue is a standard report needing maintainer review.
- Apply `needs-info` **instead** of `needs-triage` if the issue is clearly a question (how-to, clarification, support request) rather than a bug/feature report.
- Do **not** apply `accepted` — that is for maintainers to add later.

## Step 5: Set the issue type

Set the issue type using the GitHub API:

```bash
gh api repos/{owner}/{repo}/issues/$ARGUMENTS -X PATCH -H "X-GitHub-Api-Version: 2026-03-10" --input - <<'EOF'
{"type": "<TYPE>"}
EOF
```

Where `<TYPE>` is one of: `Bug`, `Feature`, `Task`.

## Step 6: Set the priority field

Set the priority field value using the issue field values API:

```bash
gh api repos/{owner}/{repo}/issues/$ARGUMENTS/issue-field-values -X PUT -H "X-GitHub-Api-Version: 2026-03-10" --input - <<'EOF'
{"issue_field_values": [{"field_id": 37534002, "value": "<PRIORITY>"}]}
EOF
```

Where `<PRIORITY>` is one of: `Urgent`, `High`, `Medium`, `Low`.

## Step 7: Apply labels

Apply labels using `gh issue edit`. **Never remove existing labels** — only add.

```bash
gh issue edit $ARGUMENTS --add-label "<type-label>,<priority-label>,<status-label>"
```

Example: `gh issue edit $ARGUMENTS --add-label "bug,priority: medium,needs-triage"`

## Step 8: Print summary

Print a single summary line:
```
Triaged issue #$ARGUMENTS: <issue-type> + <priority-value> + <status>.
```

## Rules

- **MUST** skip if both the issue type and priority field are already set, or if the issue has the `accepted` label.
- **MUST NOT** remove any existing labels.
- **MUST** read **all** comments before classifying — the body alone is often insufficient.
- **MUST** set exactly one issue type (via API), one priority value (via API), one type label, one priority label, and one status label.
- **MUST NOT** post comments to the issue.
- **MUST NOT** expand scope beyond triage (no code changes, no assignment suggestions, no duplicate linking).
- **MUST NOT** push commits or modify any files.