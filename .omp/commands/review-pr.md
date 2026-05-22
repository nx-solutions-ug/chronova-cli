You MUST review PR $ARGUMENTS right now. Do NOT ask for more information — execute all steps immediately.

## Step 0: Dedup check

Before doing anything else, check whether this bot has already posted a review or comment on this PR:

```bash
gh api /repos/{owner}/{repo}/issues/$ARGUMENTS/comments --jq '.[] | select(.user.login | test("chronova-agent|omp-agent")) | .body[:50]'
```

If a comment starting with `## Dependency Update Summary` from this bot already exists, print `Skipped PR #$ARGUMENTS: dependency summary already posted.` and stop.
If a review from this bot already exists, print `Skipped PR #$ARGUMENTS: review already posted.` and stop.

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
5. Post a **single** Dependency Update Summary comment using `gh pr comment $ARGUMENTS --body "..."` with this format:

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
2. Review for: bugs, type safety (`as any`, `@ts-ignore`), security issues, convention violations per AGENTS.md.
3. Deduplicate against existing unresolved review threads.
4. Post review via `gh pr review $ARGUMENTS` — `REQUEST_CHANGES` for bugs/security, `APPROVE` for minor nits.

### 4c. Human-authored PR review

1. Read the PR description and diff. Summarize the change in one paragraph.
2. Review for: bugs, type safety, security, AGENTS.md conventions (imports, Prisma, Redis, Zod, error handling, null semantics), missing tests, hardcoded values.
3. Deduplicate against existing unresolved review threads.
4. Post review via `gh pr review $ARGUMENTS` — `REQUEST_CHANGES` for bugs/security/type safety, `APPROVE` for minor nits only.

## Step 5: Common checks (all review types)

- **Type safety**: No `as any`, no `@ts-ignore` / `@ts-expect-error` outside test files.
- **Zod validation**: All API route inputs are validated with Zod v4 schemas.
- **Prisma imports**: All Prisma usage imports from `@/lib/prisma`, never `new PrismaClient()`.
- **Redis imports**: All Redis usage imports from `@/lib/redis`, never raw `ioredis`.
- **Security**: No exposed secrets, no SQL injection, proper auth checks, CSRF on state-changing endpoints.

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
- Use `gh pr comment $ARGUMENTS` for dependency update tables — exactly ONE comment per review, never post a second if one already exists.
- You MUST perform the dedup check in Step 0 before any other action. If a matching comment or review already exists, stop immediately and do not post anything.
