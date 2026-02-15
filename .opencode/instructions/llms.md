# Chronova CLI - LLM Instructions

## Project Overview

Chronova CLI is a high-performance, drop-in replacement for wakatime-cli written in Rust. It tracks coding activity by monitoring file changes and sending heartbeat data to a compatible backend.

## Architecture

- **Language:** Rust (Edition 2021)
- **Type:** CLI application with async runtime
- **Runtime:** Tokio for async operations
- **Storage:** SQLite for offline queue
- **Config:** INI format (~/.chronova.cfg)

## Key Modules

| Module | Purpose | Key Types/Traits |
|--------|---------|------------------|
| `main.rs` | Entry point, CLI dispatch | `Cli`, heartbeat processing |
| `cli.rs` | CLI argument definitions | `Cli` struct (40+ flags) |
| `config.rs` | Configuration loading | `Config`, `SyncConfig` |
| `heartbeat.rs` | Heartbeat creation & queueing | `Heartbeat`, `HeartbeatManager` |
| `api.rs` | HTTP client | `ApiClient`, auth methods |
| `sync.rs` | Sync management | `SyncManager` trait |
| `queue.rs` | SQLite queue operations | `QueueOps` trait |
| `collector.rs` | Project/language detection | Git integration |
| `logger.rs` | Logging setup | `tracing` integration |

## Common Patterns

### Error Handling
- Use `thiserror` for custom error types
- Use `anyhow` for ergonomic error propagation
- Custom errors: `ApiError`, `ConfigError`, `QueueError`, `SyncError`

### Async Patterns
- `#[tokio::main]` runtime
- `spawn_blocking` for SQLite operations
- Background sync with `tokio::spawn`
- `RwLock` for shared state

### Configuration
- Precedence: CLI args > config file > defaults
- INI format with [settings] section
- Path expansion for `~` and relative paths

## Testing

- Unit tests: `#[cfg(test)]` in each module
- Integration tests: `tests/` directory with wiremock
- Test isolation with tempfile

## Build Commands

```bash
# Development build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Cross-compilation (see Cross.toml)
cargo build --target x86_64-unknown-linux-musl
```

## Important Notes

- Always queue heartbeats first (offline-first design)
- Supports multiple auth: Bearer, Basic, X-API-Key
- Batch sending for efficiency
- Retry logic with exponential backoff
