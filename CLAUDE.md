# Claude Code instructions

@AGENTS.md

## CI automation

The `.github/workflows/claude-*.yml` workflows run Claude Code through
`anthropics/claude-code-action@v1` and invoke the project commands in
`.claude/commands/` as slash commands, e.g. `/review-pr 42`. `$ARGUMENTS` is
expanded by Claude Code itself.

Tool permissions for those runs are declared centrally per job via
`claude_args: --allowedTools ...` in the workflow — deliberately not in command
frontmatter, so there is one place to look.

## Conventions

- `gh label create` is not idempotent: it exits 422 when the label already
  exists. Always append `|| true`.
- Commits follow conventional-commit prefixes and feed semantic-release, so a
  `feat:` subject triggers a minor release. Never hand-edit `version` in
  `Cargo.toml` — `release.yml:89-101` resolves the next version and rewrites
  that field in CI.

## Claude Code activity tracking

This repo implements the CLI half of the `claude-code-wakatime` plugin
contract, so the tool you are running is also the thing being instrumented.

The plugin (>= 4.1.0) does no transcript parsing of its own. It rate-limits to
60s and executes the CLI with exactly three arguments — `--sync-ai-activity`,
`--plugin "claude-code/<ver> claude-code-wakatime/<ver>"` and
`--project-folder <cwd>` — then **logs any stdout or stderr it receives as an
error**. A successful run must therefore be byte-silent; `main.rs:267` selects
file-only logging for this reason. If you add output to that path, every
session's `~/.wakatime/claude-code.log` fills with false errors.

`ai_sync.rs` reads `~/.claude/projects/**/*.jsonl` — the live transcripts of
your own sessions, including this one. Consequences worth knowing:

- Several concurrent sessions each fire the plugin on independent timers, so
  overlapping runs are normal. An advisory lock (`ai_sync.rs:271`) lets one run
  proceed and the rest exit quietly; treat a zero-heartbeat run as expected.
- Project attribution comes from each transcript line's own `cwd`, not from
  `--project-folder`, which is only a fallback. That is what keeps concurrent
  sessions in different repos labelled correctly.
- The API mints its own heartbeat ids and does not de-duplicate, so re-parsing
  an already-reported window double-counts it. This is why an unset cutoff
  starts from a short lookback rather than upstream's fixed 2025-02-24 date.

Upstream reference when changing the parser: `wakatime-cli`'s
`pkg/ai/claude.go` and `pkg/ai/ai.go`. Fetch them rather than inferring the
transcript schema — the deviations in `ai_sync.rs` are deliberate and are
commented with the reason.

## Structural search

`AGENTS.md` requires `ast-grep` over `grep`/`rg` for structural queries and
multi-file rewrites. When a query needs a real YAML rule rather than a
one-line pattern, invoke the `ast-grep:ast-grep` skill (rule syntax,
relational/composite rules, debugging checklist); `ast-grep:outline` gives a
file's structure. Both ship with the `ast-grep` plugin; when it isn't
installed (CI runners), fall back to the reference linked above rather than
reconstructing rule syntax from memory.
