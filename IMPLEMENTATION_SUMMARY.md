# Planned: Expose sync flags via CLI / config

Goal
- Provide user-configurable sync controls (batch size, retry/backoff) without changing runtime defaults.
- Document required code changes and how to configure at runtime.

Files to modify
- [`chronova-cli/src/main.rs`](chronova-cli/src/main.rs:1) — add CLI flags and wire them into config loading.
- [`chronova-cli/src/config.rs`](chronova-cli/src/config.rs:1) — extend Config with new fields and parsing from ~/.chronova.cfg.
- [`chronova-cli/src/sync.rs`](chronova-cli/src/sync.rs:1) — already wired to use SyncConfig/RetryStrategy; ensure values are read from Config.

New CLI flags (clap)
- --sync-batch-size <N>
- --retry-max-attempts <N>
- --retry-base-delay <SECONDS>
- --retry-max-delay <SECONDS>
- --retry-jitter / --no-retry-jitter (bool)

Config file keys (chronova.cfg)
- [sync]
  - batch_size = 50
  - max_retry_attempts = 5
  - retry_base_delay_seconds = 1
  - retry_max_delay_seconds = 60
  - retry_use_jitter = true

Defaults
- batch_size: 50
- max_retry_attempts: 5
- retry_base_delay_seconds: 1
- retry_max_delay_seconds: 60
- retry_use_jitter: true

Behavior
- CLI flags override config file values.
- Config file overrides hard-coded defaults.
- Values feed into `SyncConfig` and `RetryStrategy` at startup, e.g.:
```bash
# apply sensible defaults, then override
sync_config.batch_size = cli.sync_batch_size.unwrap_or(config.sync.batch_size.unwrap_or(50))
```

Notes for implementer
- Validate numeric ranges (batch_size > 0, max_retry_attempts reasonable).
- Add unit tests for parsing and for boundary conditions (0/negative values).
- Add README note and update `chronova-cli/.chronova.cfg.example` with the new [sync] section.

Quick example (CLI)
```bash
chronova-cli --sync-batch-size 100 --retry-max-attempts 8 --retry-jitter
```

Quick example (~/.chronova.cfg)
```ini
[sync]
batch_size = 100
max_retry_attempts = 8
retry_base_delay_seconds = 2
retry_max_delay_seconds = 120
retry_use_jitter = true
```

Next steps (if you want me to implement)
- Modify the two files above to parse flags and config and propagate into `ChronovaSyncManager::with_config`.
- Add unit tests for config parsing and integration tests simulating rate-limits.
