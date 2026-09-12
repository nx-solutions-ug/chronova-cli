---
description: "Review a Renovate/Dependabot pull request: research changelogs and assess the impact of the update"
argument-hint: <pr-number>
---

You MUST review dependency PR $ARGUMENTS right now. Do NOT ask for more information — execute all steps immediately.

## Step 0: Resolve repository

Determine the full owner/repo slug. Use the GH_REPO environment variable if available, otherwise detect it:

```bash
REPO_SLUG="${GH_REPO:-$(gh repo view --json nameWithOwner --jq .nameWithOwner)}"
echo "Repository: $REPO_SLUG"
```

Use $REPO_SLUG in all subsequent gh api calls instead of {owner}/{repo}.

## Step 1: Read the PR and Diff

Fetch PR details:

```bash
gh pr view $ARGUMENTS --json title,body,author,headRefOid --jq '{title: .title, body: .body, author: .author.login, headSha: .headRefOid}'
```

Run `gh pr diff $ARGUMENTS` to determine:
- Which packages or images were updated
- Old and new versions
- The update type (patch / minor / major)

Focus on `Cargo.toml`, `Cargo.lock`, and GitHub Actions workflow files.

## Step 2: Research Release Notes

For EACH updated dependency, find the actual changelog or release notes:
- **crates.io crates**: Check GitHub releases via `gh api /repos/{owner}/{repo}/releases` or inspect `CHANGELOG.md`.
- **GitHub Actions**: Check the action repository's releases via `gh api /repos/{owner}/{repo}/releases`.

If you cannot find release notes, state so explicitly. Do NOT fabricate changes.

## Step 3: Assess Impact on Chronova CLI

- Check project standards in `AGENTS.md` and `.wiki/` for dependency guidelines.
- Check whether version constraints in `Cargo.toml` are compatible.
- For library updates: check if any deprecated or removed APIs are used in `src/` (scan `use` statements and call sites across `src/`), and note breaking changes per SemVer in the Rust ecosystem.
- Note any new features or performance improvements we might leverage.

## Step 4: Check for Renovate Dashboard

If the PR author is `renovate[bot]`, find the Renovate Dashboard issue:

```bash
DASHBOARD_ISSUE=$(gh issue list --search "Renovate Dashboard" --json number --jq '.[0].number')
```

If found, include a reference line at the bottom:
`> 📋 Tracked in #$DASHBOARD_ISSUE`

## Step 5: Post Review

Submit a GitHub review via the pulls API:

```markdown
## Dependency Update Summary

### Changes
| Package | From | To | Type |
|---------|------|----|------|
| [package-name] | [old-version] | [new-version] | [patch/minor/major] |

### Release Highlights
- **Security fixes**: CVEs or security patches (if any)
- **Bug fixes**: Notable fixes relevant to our usage
- **Breaking changes**: Anything that could affect us
- **Deprecations**: New deprecations to be aware of
- **New features**: Anything we might want to leverage

### Impact Assessment
- [ ] No breaking changes detected
- [ ] Version constraints are compatible
- [ ] No deprecated API usage found in codebase

### Recommendation
[SAFE TO MERGE / REVIEW RECOMMENDED / ACTION REQUIRED] with reasoning
```

Submit using the GitHub API:
- For safe patches and minor updates with no breaking changes:
  ```bash
  HEAD_SHA=$(gh pr view $ARGUMENTS --json headRefOid --jq .headRefOid)
  gh api --method POST /repos/$REPO_SLUG/pulls/$ARGUMENTS/reviews \
    -f event=APPROVE \
    -f commit_id="$HEAD_SHA" \
    -f body="[Review content here]"
  ```
- If review is recommended or uncertain:
  ```bash
  gh api --method POST /repos/$REPO_SLUG/pulls/$ARGUMENTS/reviews \
    -f event=COMMENT \
    -f commit_id="$HEAD_SHA" \
    -f body="[Review content here]"
  ```
- If breaking changes or regressions are identified:
  ```bash
  gh api --method POST /repos/$REPO_SLUG/pulls/$ARGUMENTS/reviews \
    -f event=REQUEST_CHANGES \
    -f commit_id="$HEAD_SHA" \
    -f body="[Review content here]"
  ```

## Rules
- Do NOT push commits or modify repository files.
- Do NOT merge the PR.
- Always use $REPO_SLUG for API calls.
- Ground all claims in real changelogs; never fabricate version changes.
