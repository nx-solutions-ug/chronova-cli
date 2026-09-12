# Agent Instructions for Chronova CLI

## Project Context

This is a Rust CLI application that tracks coding activity as a Wakatime-compatible alternative. It uses async patterns, SQLite for offline storage, and supports multiple authentication methods.

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
- Use `shellexpand` for path expansion
- Validate early, fail fast

### API Compatibility
- Maintain Wakatime-compatible endpoints
- Support all auth methods (Bearer, Basic, X-API-Key)
- Handle rate limiting gracefully

### Testing
- Add unit tests for new functions
- Use `tempfile` for test isolation
- Mock API calls with wiremock for integration tests

## Common Tasks

### Adding a New CLI Flag
1. Add to `Cli` struct in `cli.rs` with appropriate attributes
2. Handle in `main.rs` match arms
3. Document in help text

### Adding a New Config Option
1. Add field to appropriate config struct in `config.rs`
2. Add getter method
3. Update config parsing logic

### Adding a New Heartbeat Field
1. Update `Heartbeat` struct in `heartbeat.rs`
2. Update database schema in `queue.rs`
3. Add migration if needed
4. Update serialization

### Adding API Endpoints
1. Add method to `ApiClient` in `api.rs`
2. Handle all auth methods
3. Add proper error handling
4. Add retry logic if needed

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

## Dependencies to Know

- `clap` - CLI parsing with derive macros
- `tokio` - Async runtime
- `reqwest` - HTTP client
- `serde` - Serialization
- `rusqlite` - SQLite bindings
- `anyhow`/`thiserror` - Error handling
- `tracing` - Structured logging

## Testing Checklist

Before submitting changes:
- [ ] Unit tests pass: `cargo test`
- [ ] Integration tests pass
- [ ] Clippy clean: `cargo clippy -- -D warnings`
- [ ] Formatted: `cargo fmt`
- [ ] No compiler warnings
- [ ] Manual test of changed functionality
