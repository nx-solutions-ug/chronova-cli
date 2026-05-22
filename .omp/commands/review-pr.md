You MUST review PR $ARGUMENTS right now. Do NOT ask for more information — execute all steps immediately.

## Step 0: Dedup check

Before doing anything else, check whether this bot has already posted a review or comment on this PR:

```bash
gh api /repos/{owner}/{repo}/issues/$ARGUMENTS/comments --jq '.[] | select(.user.login | test("chronova-agent|omp-agent")) | "\(.id) \(.body[:80])"'
```

If a review from this bot already exists, print `Skipped PR #$ARGUMENTS: review already posted.` and stop.

If any comments starting with `## Dependency Update Summary` from this bot exist, delete them so a fresh summary can be posted:

```bash
COMMENT_IDS=$(gh api /repos/{owner}/{repo}/issues/$ARGUMENTS/comments --jq '.[] | select(.user.login | test("chronova-agent|omp-agent")) | select(.body | startswith("## Dependency Update Summary")) | .id')
for id in $COMMENT_IDS; do
  gh api -X DELETE /repos/{owner}/{repo}/issues/comments/$id
done
```

Then continue with the review.

## Step 1: Read the PR

```bash
gh pr view $ARGUMENTS --json title,body,labels,author,comments --jq '{title: .title, body: .body, labels: [.labels[].name], author: .author.login, comments: [.comments[] | {author: .author.login, body: .body}]}'
```

## Step 2: Read the diff

```bash
gh pr diff $ARGUMENTS
```

## Step 3: Determine review type from the PR author

- If author is `renovate[bot]` or `dependabot[bot]` → **dependency PR**: follow Step 4a
- If author contains `[bot]` → **bot-authored PR**: follow Step 4b
- Otherwise → **human-authored PR**: follow Step 4c

## Step 4: Conduct the review

### 4a. Dependency PR review

1. From the diff, list every package version change (old → new). Focus on `package.json`, `package-lock.json`, `yarn.lock`, or `pnpm-lock.yaml` changes.
2. For each changed package, research its changelog for: breaking changes, security fixes, deprecations, and peer dependency changes.
3. Check whether changed APIs or exports are used in `src/`. Search for imports of the changed packages.
4. Check if peer dependency changes affect other installed packages.
5. **If the PR author is `renovate[bot]`**, find the Renovate Dashboard issue and include a link in the summary:

```bash
DASHBOARD_ISSUE=$(gh issue list --search "Renovate Dashboard" --json number --jq '.[0].number')
```

If found, include a line at the bottom of the summary comment:

> 📋 Tracked in #<issue_number>

6. Post a **single** Dependency Update Summary comment using `gh pr comment $ARGUMENTS --body "..."` with this format:

```markdown
## Dependency Update Summary

| Package | Change | Type | Recommendation |
|---------|--------|------|----------------|
| pkg-name | 1.2.3 → 1.2.4 | patch / minor / major | SAFE / REVIEW / ACTION REQUIRED |

### Notes
- [Per-package notes on breaking changes, security fixes, deprecations, peer deps, or usage in src/]
```

Assign recommendation per package:
- **SAFE**: Patch or minor update with no breaking changes and no usage of changed APIs in `src/`.
- **REVIEW**: Minor update with deprecations, or changed APIs are used in `src/` but no known breakage.
- **ACTION REQUIRED**: Major version with breaking changes, or a security vulnerability.

### 4b. Bot-authored PR review

1. Read the PR description and diff. Summarize the change intent in one paragraph.
2. Review for: bugs, type safety (`as any`, `@ts-ignore`), security issues, convention violations.
3. Deduplicate against existing unresolved review threads.
4. Post review via `gh pr review $ARGUMENTS` — `REQUEST_CHANGES` for bugs/security, `APPROVE` for minor nits.

### 4c. Human-authored PR review

For chronova-cli (Rust CLI tool): Review for unsafe usage, error handling (no unwrap in production code, proper Result propagation), security, missing tests, hardcoded values.

1. Read the PR description and diff. Summarize the change in one paragraph.
2. Review for: unsafe usage, error handling (no unwrap in production code, proper Result propagation), security, missing tests, hardcoded values.
3. Deduplicate against existing unresolved review threads.
4. Post review via `gh pr review $ARGUMENTS` — `REQUEST_CHANGES` for bugs/security, `APPROVE` for minor nits only.

## Step 5: Common checks (all review types)

- **Rust safety**: No unnecessary unsafe blocks, proper error handling with Result/Option.
- **Security**: No exposed secrets, proper input validation.

## Step 6: Print summary

Print a single summary line:
```
Reviewed PR #$ARGUMENTS (<type>): <APPROVE / REQUEST_CHANGES / COMMENT> — <one-line summary>.
```

## Rules

- Do NOT push commits or modify any files.
- Do NOT apply labels.
- Do NOT merge the PR.
- Deduplicate findings against existing unresolved review threads before posting.
- Use `gh pr review $ARGUMENTS` for code reviews.
- Use `gh pr comment $ARGUMENTS` for dependency update tables — delete older summary comments before posting a fresh one (Step 0 handles this).

- You MUST perform the dedup check in Step 0 before any other action. If a review from this bot already exists, stop immediately. Old dependency summary comments are deleted in Step 0 so a fresh one can be posted.