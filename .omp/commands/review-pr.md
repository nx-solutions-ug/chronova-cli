You MUST review PR $ARGUMENTS right now. Do NOT ask for more information — execute all steps immediately.

## Step 0: Resolve repository and install extension

Determine the full owner/repo slug. Use the GH_REPO environment variable if available, otherwise detect it:

```bash
REPO_SLUG="${GH_REPO:-$(gh repo view --json nameWithOwner --jq .nameWithOwner)}"
echo "Repository: $REPO_SLUG"
```

Use $REPO_SLUG in all subsequent gh api calls instead of {owner}/{repo}.

Ensure the `gh-pr-review` extension is installed (it provides the inline review comment workflow):

```bash
gh extension install agynio/gh-pr-review 2>/dev/null || true
```

All inline review comments are posted via `gh pr-review` subcommands, NOT `gh pr review` or `gh api`. The `gh pr review` CLI command cannot attach inline comments to specific diff lines.

## Step 1: Dedup check

Check whether this bot has already reviewed this PR. Reviews live under the pulls API, NOT the issues API:

```bash
gh api /repos/$REPO_SLUG/pulls/$ARGUMENTS/reviews --jq '.[] | select(.user.login | test("chronova-agent|omp-agent")) | "\(.id) \(.state) \(.body[:80])"'
```

If no prior review from this bot exists, skip to the dependency summary cleanup below and continue with the review.

If a prior review exists, you MUST determine whether the findings are still relevant at the current PR head. Do NOT blanket-skip just because a review exists — the author may have pushed fixes. Fetch the bot's unresolved inline threads:

```bash
gh pr-review review view --reviewer chronova-agent --unresolved --not_outdated -R $REPO_SLUG $ARGUMENTS
```

Then compare each unresolved thread's `path` + `line` against the current diff (Step 3):
- If ALL unresolved threads are now resolved or the code at those lines has changed to address the findings, print `Skipped PR #$ARGUMENTS: review already posted and all findings addressed.` and stop.
- If some threads are still unresolved and the code hasn't changed, do NOT stop — proceed with the review. Step 6.4 will ensure you only post NEW findings not already raised in an unresolved thread. This allows the bot to re-review when the author pushes new changes that introduce new issues, while avoiding duplicate comments on unchanged lines.

Also check issue-level comments from this bot (dependency summaries, general notes):

```bash
gh api /repos/$REPO_SLUG/issues/$ARGUMENTS/comments --jq '.[] | select(.user.login | test("chronova-agent|omp-agent")) | "\(.id) \(.body[:80])"'
```

If any comments starting with `## Dependency Update Summary` from this bot exist, delete them so a fresh summary can be posted:

```bash
COMMENT_IDS=$(gh api /repos/$REPO_SLUG/issues/$ARGUMENTS/comments --jq '.[] | select(.user.login | test("chronova-agent|omp-agent")) | select(.body | startswith("## Dependency Update Summary")) | .id')
for id in $COMMENT_IDS; do
  gh api -X DELETE /repos/$REPO_SLUG/issues/comments/$id
done
```

Then continue with the review.

## Step 2: Read the PR

```bash
gh pr view $ARGUMENTS --json title,body,labels,author,headRefOid --jq '{title: .title, body: .body, labels: [.labels[].name], author: .author.login, headSha: .headRefOid}'
```

Store the `headSha` value — you will pass it as `--commit` when starting the pending review so inline comments anchor to the correct commit.

## Step 3: Read the diff

```bash
gh pr diff $ARGUMENTS
```

You MUST parse the diff to map each finding to a specific file path and line number. Inline review comments require a `--path` and `--line` that exist in the PR diff. See **Step 6: Mapping findings to diff lines** for the exact rules.

## Step 4: Determine review type from the PR author

- If author is `renovate[bot]` or `dependabot[bot]` → **dependency PR**: follow Step 5a
- If author contains `[bot]` → **bot-authored PR**: follow Step 5b
- Otherwise → **human-authored PR**: follow Step 5c

## Step 5: Conduct the review

### 5a. Dependency PR review

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
- [Per-package notes on breaking changes, security fixes, deprecations, peer deps, or usage in src]
```

Assign recommendation per package:
- **SAFE**: Patch or minor update with no breaking changes and no usage of changed APIs in `src/`.
- **REVIEW**: Minor update with deprecations, or changed APIs are used in `src/` but no known breakage.
- **ACTION REQUIRED**: Major version with breaking changes, or a security vulnerability.

Dependency PRs do NOT use the inline-review submission in Step 7. Stop after posting the summary comment.

### 5b. Bot-authored PR review

1. Read the PR description and diff. Summarize the change intent in one paragraph.
2. Review for: bugs, type safety (`as any`, `@ts-ignore`), security issues, convention violations per AGENTS.md.
3. Deduplicate against existing unresolved review threads (see Step 6.4).
4. Submit the review per Step 7 — `REQUEST_CHANGES` for bugs/security, `APPROVE` for clean changes.

### 5c. Human-authored PR review

1. Read the PR description and diff. Summarize the change in one paragraph.
2. Review for: bugs, type safety, security, AGENTS.md conventions (imports, Prisma, Redis, Zod, error handling, null semantics), missing tests, hardcoded values.
3. Deduplicate against existing unresolved review threads (see Step 6.4).
4. Submit the review per Step 7 — `REQUEST_CHANGES` for bugs/security/type safety, `APPROVE` for clean changes or minor nits only.

## Step 6: Mapping findings to diff lines

GitHub inline review comments MUST reference a line that exists in the PR diff. A comment that points at a line not in the diff will be rejected with an error. Follow these rules exactly.

### 6.1 Parse the diff structure

Each diff hunk looks like:

```diff
@@ -40,7 +40,12 @@ function foo() {
   const a = 1;
   const b = 2;
-  const c = 3;
+  const c = 4;
+  const d = 5;
```

- The `@@ -OLD_START,OLD_LEN +NEW_START,NEW_LEN @@` header tells you the starting line in both the old (left) and new (right) file.
- Lines starting with ` ` (space) are context lines — present in both sides.
- Lines starting with `-` are removed lines — they exist on the LEFT side only.
- Lines starting with `+` are added lines — they exist on the RIGHT side only.

### 6.2 Compute the line number for a finding

For a finding on an **added or context line** (RIGHT side):
- `--side RIGHT` (this is the default if omitted, but pass it explicitly for clarity)
- `--line`: the line number in the new (post-change) file. Compute it by counting from `NEW_START` in the hunk header: the first line after the `@@` header is `NEW_START`, the next is `NEW_START + 1`, etc. Context lines and `+` lines both count; `-` lines do NOT count toward the RIGHT side.

For a finding on a **removed line** (LEFT side):
- `--side LEFT`
- `--line`: the line number in the old (pre-change) file. Compute it by counting from `OLD_START` in the hunk header: the first line after the `@@` header is `OLD_START`, the next is `OLD_START + 1`, etc. Context lines and `-` lines both count; `+` lines do NOT count toward the LEFT side.

### 6.3 Multi-line range comments

To comment on a range of lines (e.g. a multi-line block), set:
- `--start-line`: the first line of the range.
- `--start-side`: same as `--side`.
- `--line`: the last line of the range.
- `--side`: `RIGHT` for added lines, `LEFT` for removed lines.

Both `--start-line` and `--line` MUST be on the same side and MUST both exist in the diff. A single-line comment omits `--start-line`/`--start-side`.

### 6.4 Deduplicate against existing inline threads

Before adding comments, fetch existing unresolved inline review threads from this bot:

```bash
gh pr-review review view --reviewer chronova-agent --states CHANGES_REQUESTED,COMMENTED --unresolved --not_outdated -R $REPO_SLUG $ARGUMENTS
```

This returns a JSON report of reviews with their inline comment threads. For each finding you plan to post, check if an existing comment already covers the same `path` + `line` with the same concern. If so, skip it — do not post duplicates. Only post NEW findings not already raised in an unresolved thread.

### 6.5 Findings that cannot be mapped to a line

Some findings are general (e.g. "missing tests", "architecture concern", "naming convention") and do not map to a specific diff line. Put these in the review `--body` (the review summary) as bullet points under a `## Summary` heading when you submit — do NOT force them into inline comments with guessed line numbers. Inline comments with a wrong line will cause the `--add-comment` command to fail.

## Step 7: Common checks (all review types)

- **Type safety**: No `as any`, no `@ts-ignore` / `@ts-expect-error` outside test files.
- **Zod validation**: All API route inputs are validated with Zod v4 schemas.
- **Prisma imports**: All Prisma usage imports from `@/lib/prisma`, never `new PrismaClient()`.
- **Redis imports**: All Redis usage imports from `@/lib/redis`, never raw `ioredis`.
- **Security**: No exposed secrets, no SQL injection, proper auth checks, CSRF on state-changing endpoints.

## Step 8: Submit the review with inline comments

You MUST submit the review via the `gh pr-review` extension's pending review workflow — NOT via `gh pr review`. The workflow is: start a pending review, add inline comments one at a time, then submit with APPROVE/REQUEST_CHANGES/COMMENT.

### 8.1 Start a pending review

Open a pending review anchored to the PR head commit:

```bash
REVIEW_JSON=$(gh pr-review review --start --commit <HEAD_SHA> -R $REPO_SLUG $ARGUMENTS)
REVIEW_ID=$(echo "$REVIEW_JSON" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")
echo "Pending review: $REVIEW_ID"
```

The `--commit` value is the head SHA from Step 2. The returned `id` is a GraphQL node ID starting with `PRR_` — you MUST capture it for the subsequent commands.

### 8.2 Add inline comments

For each line-specific finding, add an inline comment to the pending review:

````
gh pr-review review --add-comment \
  --review-id "$REVIEW_ID" \
  --path "src/lib/foo.ts" \
  --line 42 \
  --side RIGHT \
  --body "**[P2]** The variable name `usr` is unclear — use the full name for readability.

```suggestion
const user = await getUserById(id);
```" \
  -R $REPO_SLUG $ARGUMENTS
````

For multi-line ranges:

````
gh pr-review review --add-comment \
  --review-id "$REVIEW_ID" \
  --path "src/lib/foo.ts" \
  --start-line 40 \
  --start-side RIGHT \
  --line 45 \
  --side RIGHT \
  --body "**[P2]** These lines can be simplified — use early return.

```suggestion
if (!user) return null;
return user;
```" \
  -R $REPO_SLUG $ARGUMENTS
````

Comment body conventions:
- Start each inline `--body` with a severity tag: `[P0]` critical, `[P1]` high-impact bug/security, `[P2]` moderate defect, `[P3]` low-risk nit. Then state the issue concisely and what to change.
- **Include a `suggestion` block whenever you can propose a concrete code fix.** GitHub renders `` ```suggestion `` fenced blocks inside inline review comments as apply-able "Commit suggestion" buttons. This is the primary mechanism for code suggestions — the PR author can apply the fix with one click. Format:

  ````
  **[P2]** The variable name `usr` is unclear.

  ```suggestion
  const user = await getUserById(id);
  ```
  ````

  The suggestion block content MUST be valid replacement code for the commented line(s). For multi-line ranges (when using `--start-line`), the suggestion MUST cover the entire range from `start_line` to `line`. Do NOT include diff markers (`+`/`-`) in the suggestion — only the replacement code.

  Only omit the `suggestion` block when the finding is purely observational (e.g. "this function is too complex, consider refactoring") and no concrete replacement can be proposed. In that case, describe the issue and the recommended approach in prose.
- Every `--path` + `--line` MUST exist in the PR diff (Step 3). If `--add-comment` fails with an error, the most likely cause is a wrong `--path`/`--line`. Re-read the diff for that file, recompute the correct line number per Step 6, and retry once. If it still fails, skip that comment and continue with the rest — do not lose the entire review over one bad line.

If there are no line-specific findings, skip this step — a body-only review is valid (submit with `--event COMMENT` or `--event APPROVE` and no inline comments).

### 8.3 Submit the review

Finalize the pending review:

```bash
gh pr-review review --submit \
  --review-id "$REVIEW_ID" \
  --event "REQUEST_CHANGES" \
  --body "## Code Review

<one-paragraph summary of the change>

## Summary
- <general findings as bullet points, if any — these are findings that do not map to a specific diff line>" \
  -R $REPO_SLUG $ARGUMENTS
```

Event types:
- `APPROVE`: clean review, no blocking issues. `--body` is optional.
- `REQUEST_CHANGES`: bugs, security issues, type safety violations. `--body` is required.
- `COMMENT`: non-blocking observations, suggestions. `--body` is required.

If the submit fails with GraphQL errors, the JSON response will contain `"status": "Review submission failed"` and an `errors` array. Read the errors, fix the issue (most commonly a bad line number in an inline comment — remove that comment's `--add-comment` call), and resubmit. If inline comments are the problem and you cannot resolve them, you can submit the pending review as `COMMENT` with just the `--body` summary so the review is not lost.

## Step 9: Print summary

Print a single summary line:

```
Reviewed PR #$ARGUMENTS (<type>): <APPROVE / REQUEST_CHANGES / COMMENT> — <one-line summary>. <N> inline comments posted.
```

## Rules

- Do NOT push commits or modify any files.
- Do NOT apply labels.
- Do NOT merge the PR.
- Deduplicate findings against existing unresolved review threads before posting (Step 6.4).
- Use `gh pr-review` subcommands for code reviews with inline comments — NEVER use `gh pr review` for reviews that need inline comments. `gh pr review` only posts a body and cannot attach comments to diff lines.
- Use `gh pr comment $ARGUMENTS` for dependency update tables — delete older summary comments before posting a fresh one (Step 1 handles this).
- You MUST perform the dedup check in Step 1 before any other action. Only skip if ALL prior review findings are addressed at the current head — do NOT blanket-skip just because a review exists. Old dependency summary comments are deleted in Step 1 so a fresh one can be posted.
- MUST resolve the repository slug before any gh api calls. Use the GH_REPO environment variable if available.
- Every inline comment `--path` + `--line` MUST exist in the PR diff. If you are not certain a line exists in the diff, put the finding in the review `--body` instead — a body-only finding is always safe; a wrong inline line fails the `--add-comment` command.