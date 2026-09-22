# Agent Instructions for Chronova CLI

## Project Context

This is a Rust CLI application that tracks coding activity as a Wakatime-compatible alternative. It uses async patterns, SQLite for offline storage, and supports multiple authentication methods.

## Modules

`src/lib.rs` declares 11 modules; `src/main.rs` is the binary and holds only
flag dispatch.

| Module | Role |
|---|---|
| `cli.rs` | clap `Cli` struct; every flag and its help text |
| `config.rs` | `~/.chronova.cfg` parsing, precedence, API key/URL |
| `heartbeat.rs` | `Heartbeat`/`AiTelemetry` types, `HeartbeatManager`, queue flush |
| `queue.rs` | SQLite offline queue behind the `QueueOps` trait (`queue.rs:62`) |
| `api.rs` | `ApiClient`, auth variants, single and batch sends |
| `sync.rs` | retry/backoff policy, connectivity monitoring, metrics |
| `collector.rs` | project, git and language detection from a path |
| `ai_sync.rs` | `--sync-ai-activity`: Claude Code transcript → heartbeats |
| `updater.rs` | self-update from GitHub releases |
| `user_agent.rs` | user-agent string assembly |
| `logger.rs` | `tracing` setup; file and optional stdout layers |

## State on Disk

Every path derives from `dirs::home_dir()`, so `$HOME` fully isolates a run —
useful for testing against real data without touching live state.

| Path | Written by |
|---|---|
| `~/.chronova.cfg` | user/config; `--config` overrides (`cli.rs:54`) |
| `~/.chronova.log` | `logger.rs:90` |
| `~/.chronova/queue.db` | `queue.rs:716` (WAL mode) |
| `~/.chronova-internal.cfg` | `ai_sync.rs:277` — `[internal] ai_logs_last_parsed_at` |
| `~/.chronova/ai-sync.lock` | `ai_sync.rs:271` — advisory lock, released on drop |

## When Working on This Codebase

### Error Handling
- Use `thiserror` for defining custom error enums
- Use `anyhow::Result` for function returns
- Propagate errors with `?` operator
- Add tracing for error paths: `tracing::error!("...")`

### Async Operations
- Wrap blocking operations (SQLite) in `spawn_blocking`
- Use `tokio::sync::RwLock` for shared mutable state
- Prefer `tokio::spawn` for background tasks

### Database Operations
- All DB operations go through the `QueueOps` trait
- Use transactions for batch operations
- Always handle migration scenarios

### Configuration
- Respect config precedence: CLI > file > defaults
- Validate early, fail fast
- There is no `shellexpand` dependency. Paths are resolved with
  `dirs::home_dir()`, and `~` expansion in `config.rs:187` only matches the
  exact strings `~/.chronova.cfg` and `.chronova.cfg` — a `~` anywhere else in
  a config value is **not** expanded.

### API Compatibility
- Maintain Wakatime-compatible endpoints
- Support all auth methods (Bearer, Basic, X-API-Key)
- Handle rate limiting gracefully

### Testing
- Add unit tests for new functions
- Use `tempfile` for test isolation
- Mock API calls with wiremock for integration tests
- To exercise a whole command against real data without touching live state,
  run it under a throwaway `$HOME` (see State on Disk) and point `api_url` at
  an unroutable address to test the failure path, or at a local mock to test
  the success path. Every config, queue, log and state file follows `$HOME`.

## Common Tasks

### Adding a New CLI Flag
1. Add to `Cli` struct in `cli.rs` with appropriate attributes
2. Handle in `main.rs`. Dispatch is a sequence of ~17 `if cli.<flag>` guards
   that each `return`/`process::exit` — not `match` arms
3. Document in help text. The clap doc comment *is* the help text

If the flag must work without `--entity`, handle it **before** the guard at
`main.rs:295` (`cli.entity.is_none() && cli.sync_offline_activity.is_none()`),
which prints an error and exits. `--sync-ai-activity` sits directly above it
for that reason.

### Adding a New Config Option
1. Add field to appropriate config struct in `config.rs`
2. Add getter method
3. Update config parsing logic

### Adding a New Heartbeat Field
1. Update `Heartbeat` struct in `heartbeat.rs:16`
2. Mark it `#[serde(default, skip_serializing_if = "Option::is_none")]`
3. Update every struct literal — `ast-grep` finds them all, including
   fully-qualified ones (see Structural code search below)

**No database migration is needed.** The queue table is
`heartbeats(id, data, created_at, retry_count, last_attempt)` at
`queue.rs:518` — the heartbeat is one serialised JSON blob in `data`, not a
column per field. `#[serde(default)]` is what keeps rows queued by an older
build readable; without it they fail to deserialize on the next sync.
`apply_migration_v1` (`queue.rs:573`) exists for the queue's own bookkeeping
columns (`sync_status`, `sync_metadata`), not for heartbeat fields.

`AiTelemetry` (`heartbeat.rs:60`) shows the pattern: a grouped struct held as
one `#[serde(default, flatten)]` field, so it serialises flat into the API
payload while costing each literal a single line.

### Adding API Endpoints
1. Add method to `ApiClient` in `api.rs`
2. Handle all auth methods
3. Add proper error handling
4. Add retry logic if needed

## Landmines

Two behaviours that are easy to trip over and hard to notice:

- **The queue survives construction, but not forever.** `HeartbeatManager::new`
  (`heartbeat.rs:119`) delegates to `new_with_queue` (`heartbeat.rs:136`),
  the single construction path, and neither removes anything from the queue —
  a heartbeat queued by a previous invocation is still there when the next
  invocation constructs its own manager. Retention is enforced from two
  places, both driven by the same configured `sync_retention_days`
  (`config.rs:275`, default 7): `process_queue` calls `enforce_retention`
  (`heartbeat.rs:158`) on every sync/flush, and `new_with_queue` also calls
  `Queue::set_retention_days` so `Drop for Queue` (`queue.rs:758`) prunes at
  the same window if a queue is ever dropped without reaching
  `process_queue` at all — e.g. `--extra-heartbeats`, which only enqueues
  and never syncs. `max_age_days == 0` is still the special case that runs
  `DELETE FROM heartbeats` (`queue.rs:314-317`), so neither path may ever
  reach it with a literal `0`: a configured `0` is treated as "skip
  retention" instead (`enforce_retention`'s own guard on the sync/flush
  side; `set_retention_days` storing `None` on the drop side) — that
  conflation is the bug this landmine used to describe. An explicit "clear
  the queue" caller, and the test suite, may still call
  `cleanup_old_entries(0)` deliberately.

- **`tracing` at INFO goes to stdout, not just the log file.** `setup_logging`
  adds a stdout layer in normal mode (`logger.rs:63-65`) and the default level
  is INFO (`logger.rs:36`). For any flag whose caller parses or error-checks
  output, use `setup_logging_with_output_format(verbose, true)`, which keeps
  file logging and drops the stdout layer. `--sync-ai-activity` does this
  because the invoking plugin logs any output as an error.

## Code Style

- Follow Rust naming conventions
- Use `tracing` for logging (not println)
- Document public APIs with rustdoc
- Keep functions focused and small
- Prefer composition over inheritance (trait-based design)

## Critical Paths

1. **Heartbeat Flow:** CLI parse → Config load → Heartbeat create → Queue → API send
2. **Sync Flow:** Queue::process_queue → batch/individual → API → status update
3. **Error Flow:** Any error → tracing log → propagate up → user message
4. **AI Sync Flow** (`ai_sync.rs:65`): acquire lock → load `ai_logs_last_parsed_at`
   → walk `~/.claude/projects/**/*.jsonl` by mtime → parse each line's
   `toolUseResult` → build file + app heartbeats → enqueue → flush → advance the
   cutoff. On a total send failure the batch is removed again and the cutoff
   held, so the next run re-derives it from the transcripts, which are the real
   durable store (see Landmines).

## Dependencies to Know

- `clap` - CLI parsing with derive macros
- `tokio` - Async runtime
- `reqwest` - HTTP client
- `serde` - Serialization
- `rusqlite` - SQLite bindings
- `anyhow`/`thiserror` - Error handling
- `tracing` - Structured logging
- `configparser` - INI parsing for `~/.chronova.cfg` and the internal state file
- `dirs` - home-directory resolution for every on-disk path
- `chrono` - timestamps; RFC 3339 for the AI sync cutoff
- `uuid` - client-side heartbeat ids
- `git2` - branch/commit detection in `collector.rs`
- `gethostname` - the `machine` field

## Testing Checklist

Before submitting changes:
- [ ] Unit tests pass: `cargo test`
- [ ] Integration tests pass
- [ ] Clippy clean: `cargo clippy -- -D warnings`
- [ ] Formatted: `cargo fmt`
- [ ] No compiler warnings
- [ ] Manual test of changed functionality

## Structural code search (ast-grep)

Use `ast-grep` — not `grep`/`rg` — for anything **structural**: finding call
sites, function/class/JSX shapes, or code matching a pattern rather than a
string. Use it for **every multi-file rewrite**. Text search also hits
comments, strings and unrelated identifiers; ast-grep matches AST nodes.

Fall back to `rg` only for literal text, non-code files (Markdown, JSON, lock
files), or languages ast-grep cannot parse.

```bash
# Search — single-node patterns. Always single-quote: "$A" is shell-expanded.
ast-grep run -p 'console.log($ARG)' -l ts src/
ast-grep run -p 'useEffect($CB, $DEPS)' -l tsx --json src/ | jq -r '.[].file'

# Search — relational / composite queries
ast-grep scan --inline-rules 'id: await-in-loop
language: TypeScript
rule:
  kind: for_in_statement
  has:
    pattern: await $E
    stopBy: end' src/

# Rewrite — prints a diff by default; -i reviews each edit, -U applies all
ast-grep run -p 'var $N = $V' -r 'let $N = $V' -l ts -i src/
```

Non-obvious rules, in the order they bite:

- Invoke it as `ast-grep`, never the `sg` alias — `sg` collides with
  shadow-utils' setgid tool on Linux.
- **Single-quote patterns.** `"$PROP && $PROP()"` reaches ast-grep as `" && ()"`
  after shell expansion.
- In relational rules (`has`, `inside`, `precedes`, `follows`) set
  `stopBy: end`, or the search stops at the first non-matching node.
- **Write inline rules in block YAML, not flow maps.** `has: { pattern: f() { $$$B }, stopBy: end }`
  fails to parse — the pattern's `}` closes the flow mapping. Indented keys
  always work.
- **Zero matches ≠ code absent.** Patterns match whole AST nodes, so
  `-p 'log($MSG)'` does _not_ match `console.log("hello")`. Before concluding
  something isn't there, inspect the parse: `--debug-query=pattern` shows how
  ast-grep read your pattern, `--debug-query=ast` shows the named nodes.
- `--inline-rules` works in any directory; bare `ast-grep scan` (project rule
  dirs) requires an `sgconfig.yml` at the repo root.
- Not on `PATH` — CI runners included: `bun add -g @ast-grep/cli`.

Full reference: <https://ast-grep.github.io/llms-full.txt>

When a query needs a real YAML rule rather than a one-line pattern, invoke the
`ast-grep:ast-grep` skill (rule syntax, relational/composite rules, debugging
checklist); `ast-grep:outline` gives a file's structure. Both ship with the
`ast-grep` plugin; when it isn't installed (CI runners), fall back to the
reference linked above rather than reconstructing rule syntax from memory.

## Claude Code CI automation

The `.github/workflows/claude-*.yml` workflows run Claude Code through
`anthropics/claude-code-action@v1` and invoke the project commands in
`.claude/commands/` as slash commands, e.g. `/review-pr 42`. `$ARGUMENTS` is
expanded by Claude Code itself.

Tool permissions for those runs are declared centrally per job via
`claude_args: --allowedTools ...` in the workflow — deliberately not in command
frontmatter, so there is one place to look.

`gh label create` exits 422 when the label already exists. Always pass
`--force`: it upserts the label, updating colour and description in place,
and unlike `|| true` it still surfaces a real failure such as a bad token,
a rate limit, or the wrong repository.

## Releases and versioning

Commits follow conventional-commit prefixes and feed semantic-release, so a
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
