## Execution requirements

You are inside an isolated GitHub Runner environment with git and the `gh` CLI authenticated via an app token. Common project tools (databases, dev servers, browsers) are NOT available.

### Workflow

1. Use `gh pr view __PR_NUMBER__ --json headRefName,headRepositoryOwner,headRepository,number` to identify the PR branch (the PR number is __PR_NUMBER__; use `$GH_REPO` or `gh repo view --json nameWithOwner --jq .nameWithOwner` to resolve the repo slug).
2. Check out the PR branch so your edits land on it: `git fetch origin && git checkout -B <headRefName> origin/<headRefName>`.
3. Make the requested changes using your tools.
4. Run the available quality gates (`npm run lint`, `npm run type-check`) if they are relevant and fast. Skip `npm test` if it needs a database or external services.
5. Commit and push the changes back to the PR branch:

```bash
git add -A
git commit -m "fix: apply requested changes from PR comment"
git push origin HEAD:<headRefName>
```

### Rules

- **MUST** commit and push all changes back to the PR branch before finishing. Staging without committing and pushing is a failure.
- **MUST NOT** push directly to `main` or `develop`.
- **MUST NOT** merge the PR.
- **MUST NOT** start a dev server, open a browser, or connect to external services.
- **MUST** commit and push via git; `gh auth setup-git` has already been run, so git push uses the app token credential helper.