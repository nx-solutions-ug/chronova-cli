You MUST fix issue $ARGUMENTS right Now. Do NOT ask for more information — execute all steps immediately. You are inside an isolated Github-Runner environment usual common tools are not accessible. Use the tools that you know and are accessible to you.

## Step 0: Resolve repository, configure git, prepare build

Determine the full owner/repo slug. Use the GH_REPO environment variable if available, otherwise detect it:

```bash
REPO_SLUG="${GH_REPO:-$(gh repo view --json nameWithOwner --jq .nameWithOwner)}"
echo "Repository: $REPO_SLUG"
```

Configure git to use the authenticated gh CLI as a credential helper so push operations work:

```bash
gh auth setup-git
```

This is a Rust project — dependencies are resolved by Cargo, no `npm`/`package.json` exists. Verify the toolchain is available:

```bash
cargo --version
rustc --version
```

Use $REPO_SLUG in all subsequent gh api calls instead of {owner}/{repo}.

## Step 1: Read the issue

Fetch the issue title, body, existing labels, issue type, priority field, and **all comments** using `gh`. Comments often contain critical reproduction details:

```bash
gh issue view $ARGUMENTS --json title,body,labels --jq '{title: .title, body: .body, labels: [.labels[].name]}'
```

Read the issue type and priority field values:

```bash
gh api repos/$REPO_SLUG/issues/$ARGUMENTS -H "X-GitHub-Api-Version: 2026-03-10" --jq '{type: .type.name, priority: (.priority // null)}'
```

Read the issue field values (for priority):

```bash
gh api repos/$REPO_SLUG/issues/$ARGUMENTS/issue-field-values -H "X-GitHub-Api-Version: 2026-03-10" --jq '[.[] | {field_id: .issue_field_id, value: .single_select_option.name}]'
```

Read all comments:

```bash
gh issue view $ARGUMENTS --json comments --jq '[.comments[] | {author: .author.login, body: .body}]'
```

## Step 2: Assess actionability

Using the issue title, body, **and all comments**, determine if this issue is actionable. An issue is **NOT actionable** if **any** of the following are true:

- It is a question, support request, or discussion — no code change is needed.
- It is a feature request with no concrete specification (missing acceptance criteria, no defined behavior).
- It is missing reproduction steps or critical information needed to reproduce the problem.
- It requires design decisions or architectural changes beyond what a single PR can address.

If the issue is **not actionable**, post a comment explaining why and **STOP**:

```bash
gh issue comment $ARGUMENTS --body "Thank you for reporting. After reviewing this issue, it does not appear actionable as-is because: <reason>. <Specific guidance on what is needed, e.g. 'Please add reproduction steps', 'This needs a design RFC before implementation', etc.>. Closing for now — feel free to reopen with the missing details."
```

Then exit. Do **not** proceed to the remaining steps.

## Step 3: Mark issue as accepted

Since the issue is actionable, add the `accepted` label to signal that work has begun:

```bash
gh issue edit $ARGUMENTS --add-label accepted
```

This marks the issue as accepted by maintainers and signals to other workflows that this issue is being worked on.

## Step 4: Create a branch

Determine the branch prefix based on the issue type:
- Use `fix/issue-$ARGUMENTS` for bugs.
- Use `feat/issue-$ARGUMENTS` for features or enhancements.

Create the branch from `main` and push it:

```bash
git checkout main
git pull origin main
git checkout -b <prefix>/issue-$ARGUMENTS
git push -u origin <prefix>/issue-$ARGUMENTS
```

## Step 5: Reproduce and understand

Read the relevant source files. Identify the root cause of the bug or the scope of the feature. Look at existing patterns in the codebase for how similar things are done:

- Search for similar bug fixes or feature implementations in `src/`.
- Read `AGENTS.md` for project conventions.
- Examine existing tests under `tests/` and `src/` (module-level `#[cfg(test)]` blocks) for patterns to follow.
- If the issue references specific files, read those files first.

Do **not** modify files yet — only read and understand.

## Step 6: Implement the fix

Make the **minimal, correct** code change that addresses the issue. Follow all project conventions from `AGENTS.md`:

- Use `anyhow::Result` for function returns and `thiserror` for custom error enums.
- Propagate errors with the `?` operator.
- Use `tracing` for logging (not `println!`): `tracing::error!("...")`, `tracing::info!("...")`.
- Use `tokio` async runtime — wrap blocking SQLite operations in `spawn_blocking`, use `tokio::sync::RwLock` for shared mutable state.
- All database operations go through the `QueueOps` trait; use transactions for batch operations and handle migrations.
- HTTP via `reqwest`; serialization via `serde`; CLI parsing via `clap` derive macros.
- Respect config precedence: CLI > file > defaults. Use `shellexpand` for path expansion. Validate early, fail fast.
- Maintain Wakatime-compatible endpoints and all auth methods (Bearer, Basic, X-API-Key).
- Follow Rust naming conventions; document public APIs with rustdoc; keep functions focused and small.
- Prefer trait-based composition over inheritance.

Make only the changes needed to fix the issue. Do **not** refactor unrelated code, add unrelated improvements, or modify files that are not part of the fix.

## Step 7: Write tests

Add or update tests that verify the fix. Tests MUST cover:

- The specific bug case described in the issue (for bugs).
- The new feature behavior (for features).
- Edge cases and error paths where applicable.

Place unit tests as `#[cfg(test)]` modules in the relevant `src/` files, or integration tests in `tests/`. Use `tempfile` for filesystem test isolation and `wiremock` to mock API calls in integration tests. Follow existing test patterns found in Step 5.

## Step 8: Run quality gates

Run all quality gates in order. Fix any failures before proceeding:

```bash
cargo test
```

```bash
cargo clippy -- -D warnings
```

```bash
cargo fmt --check
```

If any gate fails, fix the failure and re-run that gate. Do **not** proceed to Step 9 until all three gates pass cleanly.

## Step 9: Commit and push

Determine the commit prefix based on the issue type:
- Use `fix(issue-$ARGUMENTS)` for bugs.
- Use `feat(issue-$ARGUMENTS)` for features or enhancements.

Write a short, descriptive summary of the change:

```bash
git add -A
git commit -m "<prefix>: <short description>"
git push
```

Example: `fix(issue-42): correct off-by-one error in duration calculation`

## Step 10: Create a pull request

Use `gh pr create` with a complete description:

```bash
gh pr create \
  --base main \
  --head <prefix>/issue-$ARGUMENTS \
  --title "<prefix>: <short description>" \
  --body "## Summary

<1-2 sentence summary of what this PR does>

Fixes #$ARGUMENTS

## Root Cause

<Description of what was wrong>

## Fix

<Description of the change made>

## Testing

- <Test 1>
- <Test 2>
- All quality gates pass (cargo test, cargo clippy, cargo fmt --check)"
```

## Step 11: Label the PR

Add appropriate labels to the PR. Use the issue type and priority to select labels:

```bash
gh pr edit <PR-NUMBER> --add-label "<type-label>,<priority-label>"
```

Where:
- Type labels: `bug` for bugs, `feature` for features, `enhancement` for improvements.
- Priority labels: `priority: critical`, `priority: high`, `priority: medium`, or `priority: low` — match the priority from the issue.

## Rules

- **MUST NOT** modify files unrelated to the fix.
- **MUST NOT** skip writing tests.
- **MUST NOT** skip running quality gates.
- **MUST NOT** push directly to `main`.
- **MUST** add the `accepted` label before creating a branch (Step 3).
- **MUST NOT** post comments on the issue except when the issue is not actionable (Step 2).
- **MUST** create a pull request — do not merge directly.
- **MUST** follow all project conventions from `AGENTS.md`.
- **MUST** reference the issue number in commit messages and PR description.
- If quality gates fail, **MUST** fix failures before creating the PR.
- **MUST** resolve the repository slug before any gh api calls. Use the GH_REPO environment variable if available.
- **MUST** run `gh auth setup-git` before any git push to ensure authentication works.
- **MUST NOT** start a dev server, long-running daemon, or REPL. The CI environment is isolated.
- **MUST NOT** open a browser or attempt visual verification. There is no display or user session available.
- **MUST NOT** attempt to connect to external services or databases beyond what the tests mock. The CI environment is isolated.
- **MUST NOT** use `npm`, `node`, `pnpm`, or any JavaScript/TypeScript tooling — this is a Cargo/Rust project.
- When using `find` or `search` tools, ALWAYS pass `paths` as an array, not a string. Example: `find(paths=["src/**"])`, NOT `find(paths="src/**")`.
- When reading large files, use the `:raw` selector or specific line ranges. Do NOT repeatedly read the same lines — if a read returns elided content, use the suggested range selector to get the missing parts.