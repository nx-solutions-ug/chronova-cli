# gh label create idempotency

When running `gh label create`, always append `|| true` to suppress exit code 422 (label already exists). Never fail the command if the label is already present.

Pattern: `gh label create`
Rule: If the command does not include `|| true` or `2>/dev/null`, inject `|| true` before the command is executed.