# Chronova CLI Architecture Documentation

**Purpose**: Documentation of the chronova-cli codebase architecture for LLM agents and developers.

**Last Updated**: February 2026

**Version**: 0.1.0

---

## 1. Project Overview

Chronova CLI is a high-performance, drop-in replacement for wakatime-cli written in Rust. It tracks coding activity ("heartbeats") and syncs them to the Chronova/WakaTime API. The application features offline-first architecture with SQLite persistence, retry logic with exponential backoff, and comprehensive data collection for projects, git metadata, and language detection.

**Key Characteristics**:
- Async-first design using tokio runtime
- Offline-first heartbeat queuing with SQLite
- WakaTime API compatibility with multiple auth methods
- Structured logging with tracing ecosystem
- Comprehensive CLI with clap derive

---

## 2. Module Architecture

### 2.1 Module Dependency Graph

```
main.rs (entry point)
    │
    ├── cli.rs (CLI arguments)
    ├── config.rs (configuration)
    ├── heartbeat.rs (heartbeat processing)
    │   ├── api.rs (HTTP client)
    │   ├── queue.rs (SQLite queue)
    │   │   └── sync.rs (sync management)
    │   ├── collector.rs (data collection)
    │   └── user_agent.rs (user agent)
    └── logger.rs (logging)
```

### 2.2 Module Responsibilities

**main.rs** (516 lines)
- Entry point with `#[tokio::main]` async runtime
- CLI argument parsing via `Cli::parse()`
- Command routing based on flags (--today, --sync-offline-activity, --extra-heartbeats, etc.)
- Logging setup with JSON output handling
- Configuration loading and HeartbeatManager initialization
- Helper functions: `fetch_today_activity()`, `handle_config_operations()`, `process_extra_heartbeats()`

**lib.rs** (22 lines)
- Library root exporting public API
- Re-exports commonly used types: ApiClient, Cli, Config, HeartbeatManager, Queue, ChronovaSyncManager, SyncResult, SyncConfig

**cli.rs** (249 lines)
- `Cli` struct using `clap::Parser` derive
- 60+ CLI arguments covering all wakatime-cli compatibility options
- Key flags: --entity, --project, --language, --category, --time, --verbose, --output, --sync-offline-activity, --extra-heartbeats, --today, --config-read, --config-write
- Version handling with custom --version flag (clap's disable_version_flag=true)

**config.rs** (337 lines)
- `Config` struct with 20+ configuration fields
- `ConfigError` enum with ParseError, NotFound, InvalidPath variants
- `Config::load()` reads INI files via configparser crate
- Path resolution: ~ expansion, relative paths, current directory resolution
- `get_api_key()` and `get_api_url()` helper methods
- `parse_sync_config()` extracts sync settings from INI
- Default config provides sensible values and ignore patterns

**heartbeat.rs** (676 lines)
- `Heartbeat` struct: 20+ fields including entity, time, project, language, git info, user_agent
- `HeartbeatManager` struct: orchestrates heartbeat processing
- `HeartbeatManagerExt` trait: extension trait for offline sync capabilities
- `HeartbeatManager::process()`: main async processing method
- `create_heartbeat()`: builds Heartbeat from CLI args and collected data
- `should_ignore_entity()`: pattern matching for ignore rules
- `process_queue()`: batch processing with rate limit handling
- Offline-first strategy: always queue first, then sync

**api.rs** (790 lines)
- `ApiClient`: base HTTP client with 30s timeout
- `AuthenticatedApiClient`: wraps ApiClient with auth methods
- `ApiError` enum: Network, Api, Auth, RateLimit variants
- Multiple auth methods: Bearer token, Basic auth, X-API-Key header
- Methods: `send_heartbeat()`, `send_heartbeats_batch()`, `get_today_stats()`, `get_today_statusbar()`, `check_connectivity()`
- Response types: StatsResponse, StatusBarResponse, LanguageStat, ProjectStat, EditorStat, etc.

**queue.rs** (1280 lines)
- `Queue` struct: SQLite-based persistent queue
- `QueueOps` trait: defines queue operations
- `QueueError` enum: 9 error variants including Database, Serialization, Io, QueueFull, DatabaseCorruption
- `QueueEntry` struct: heartbeat with sync metadata
- `QueueStats` struct: queue statistics
- Database schema with migrations (schema_version table)
- Indexes: sync_status, created_at, retry_count
- Methods: add(), get_pending(), remove(), update_sync_status(), count_by_status(), get_sync_stats(), cleanup_old_entries(), enforce_max_count(), vacuum(), deduplicate(), increment_retry(), get_retry_count(), count()
- Corruption handling with backup and recovery

**sync.rs** (1300+ lines)
- `SyncStatus` enum: Pending, Syncing, Synced, Failed, PermanentFailure
- `SyncResult` struct: sync operation results with metrics
- `SyncStatusSummary` struct: aggregate sync statistics
- `SyncError` enum: Network, Auth, RateLimit, Database, Serialization, Config, Unknown
- `RetryStrategy` struct: exponential backoff with jitter
- `SyncConfig` struct: sync behavior configuration
- `SyncManager` trait: async trait for sync operations
- `ChronovaSyncManager` struct: full sync implementation
- `PerformanceMetrics` struct: sync performance tracking
- Background sync with tokio::spawn
- Connectivity monitoring with cached state

**collector.rs** (671 lines)
- `DataCollector` struct: data detection utilities
- `ProjectInfo` struct: project name and root path
- `GitInfo` struct: branch, commit, author, message, repository URL
- Methods: detect_project(), detect_git_info(), detect_language()
- Project detection: git, package.json, Cargo.toml, pyproject.toml, .wakatime-project
- Git info via libgit2 with URL sanitization (removes credentials)
- Language detection: filename mapping, multi-part extensions, extension lookup
- Lazy static HashMaps: EXTENSION_MAP (100+ entries), FILENAME_MAP

**logger.rs** (122 lines)
- `setup_logging()` and `setup_logging_with_output_format()` functions
- `WorkerGuard` for non-blocking file logging
- tracing_subscriber with EnvFilter
- JSON output mode for clean stdout (used by --output json)
- Custom ChronoLocalTimer for timestamp formatting
- Log file: ~/.chronova.log

**user_agent.rs** (169 lines)
- `generate_user_agent()` function
- Format: `chronova/{version} ({os}-{core}-{platform}) {runtime} {plugin}`
- Uses sysinfo crate for OS information
- Plugin string parsing with sanitization
- WakaTime compatibility: duplicates plugin token

---

## 3. Design Patterns

### 3.1 Error Handling Pattern

**Custom Error Types with thiserror**:
```rust
// ConfigError pattern
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to parse config file: {0}")]
    ParseError(String),
    #[error("Config file not found: {0}")]
    NotFound(String),
    #[error("Invalid config path: {0}")]
    InvalidPath(String),
}

// ApiError pattern
#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("API error: {0} - {1}")]
    Api(String, String),
    #[error("Authentication error: {0}")]
    Auth(String),
    #[error("Rate limited: {0}")]
    RateLimit(String),
}
```

**Error Propagation with anyhow**:
- Main function uses `Result<(), anyhow::Error>` for error propagation
- `?` operator for early returns on errors
- `unwrap_or_else()` for user-friendly error messages with process::exit()

**Error Handling in main()**:
```rust
let config = Config::load(&cli.config).unwrap_or_else(|e| {
    eprintln!("Failed to load configuration: {}", e);
    process::exit(1);
});
```

### 3.2 Configuration Management Pattern

**INI-based Configuration**:
- Uses configparser crate for INI parsing
- Config file: ~/.chronova.cfg (default)
- Section-based: [settings] section
- Path resolution with ~ expansion and relative path handling

**Configuration Precedence**:
1. CLI arguments (highest priority)
2. Config file values
3. Default values (lowest priority)

**Example Config Loading**:
```rust
pub fn load(config_path: &str) -> Result<Self, ConfigError> {
    let config_path = Self::resolve_config_path(config_path)?;
    let mut ini = Ini::new();
    ini.set_multiline(true);
    let config_map = ini.load(&config_path)?;
    let settings = config_map.get("settings").cloned().unwrap_or_default();
    // Parse settings into Config struct
}
```

### 3.3 CLI Pattern

**Clap Derive Parser**:
```rust
#[derive(Parser, Debug)]
#[command(version, about, disable_version_flag = true)]
pub struct Cli {
    #[arg(long, alias = "file")]
    pub entity: Option<String>,
    #[arg(long)]
    pub key: Option<String>,
    // ... 60+ more fields
}
```

**Command Routing in main()**:
- Sequential if-chain checks for flags
- Each flag has dedicated handling code
- --entity required unless --sync-offline-activity is set
- --output flag triggers JSON mode (disables stdout logging)

### 3.4 Offline-First Queue Pattern

**Queue Architecture**:
```rust
pub trait QueueOps {
    fn add(&self, heartbeat: Heartbeat) -> Result<(), QueueError>;
    fn get_pending(&self, limit: Option<usize>, status_filter: Option<SyncStatus>) -> Result<Vec<Heartbeat>, QueueError>;
    fn update_sync_status(&self, id: &str, status: SyncStatus, metadata: Option<String>) -> Result<(), QueueError>;
    // ... more methods
}
```

**Sync Status Flow**:
```
Pending -> Syncing -> Synced (removed from queue)
                 -> Failed (retry up to 3 times)
                 -> PermanentFailure (manual intervention needed)
```

**Batch Processing**:
- Batch size: 50 heartbeats per network call
- Consolidate DB operations in spawn_blocking
- Rate limit handling with exponential backoff

### 3.5 Extension Trait Pattern

**HeartbeatManagerExt Trait**:
```rust
pub trait HeartbeatManagerExt {
    async fn process_offline_first(&self) -> Result<(), anyhow::Error>;
    fn get_queue_stats(&self) -> Result<SyncStatusSummary, anyhow::Error>;
    async fn manual_sync(&self) -> Result<SyncResult, anyhow::Error>;
}

impl HeartbeatManagerExt for HeartbeatManager {
    // Default implementations
}
```

### 3.6 Async/Await Patterns

**Tokio Runtime**:
```rust
#[tokio::main]
async fn main() {
    // Async code here
}
```

**Blocking Operations with spawn_blocking**:
```rust
tokio::task::spawn_blocking(move || -> Result<(), anyhow::Error> {
    let q = Queue::new()?;
    q.add(heartbeat)?;
    Ok(())
}).await??;
```

**Background Tasks**:
```rust
tokio::spawn(async move {
    loop {
        // Background work
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
});
```

**Atomic State with Arc**:
```rust
pub struct ChronovaSyncManager {
    pub connectivity_state: Arc<AtomicBool>,
    pub last_connectivity_check: Arc<RwLock<Option<SystemTime>>>,
    // ...
}
```

### 3.7 Retry Strategy Pattern

**Exponential Backoff with Jitter**:
```rust
impl RetryStrategy {
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::from_secs(0);
        }
        let exponent = attempt - 1;
        let mut delay = self.base_delay_seconds * 2u64.pow(exponent);
        
        if self.use_jitter {
            let jitter_factor = 0.5 + (rand::random::<f64>() * 1.0);
            delay = (delay as f64 * jitter_factor) as u64;
        }
        
        delay = delay.min(self.max_delay_seconds);
        Duration::from_secs(delay)
    }
}
```

**Retry Decision**:
```rust
pub fn is_retryable_error(error: &SyncError) -> bool {
    match error {
        SyncError::Network(_) => true,
        SyncError::RateLimit(_) => true,
        SyncError::Auth(_) => false,  // Don't retry auth errors
        SyncError::Config(_) => false, // Don't retry config errors
        _ => true,
    }
}
```

---

## 4. Core Data Structures

### 4.1 Heartbeat Struct

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heartbeat {
    pub id: String,                          // UUID
    pub entity: String,                      // File path or URL
    #[serde(rename = "type")]
    pub entity_type: String,                 // "file", "domain", "url", "app"
    pub time: f64,                           // Unix timestamp
    pub project: Option<String>,
    pub branch: Option<String>,
    pub language: Option<String>,
    pub is_write: bool,
    pub lines: Option<i32>,
    pub lineno: Option<i32>,
    pub cursorpos: Option<i32>,
    pub user_agent: Option<String>,
    pub category: Option<String>,            // "coding", "debugging", etc.
    pub machine: Option<String>,
    pub editor: Option<EditorInfo>,
    pub operating_system: Option<OsInfo>,
    pub commit_hash: Option<String>,
    pub commit_author: Option<String>,
    pub commit_message: Option<String>,
    pub repository_url: Option<String>,
    pub dependencies: Vec<String>,
}
```

### 4.2 Config Struct

```rust
#[derive(Debug, Clone)]
pub struct Config {
    pub api_key: Option<String>,
    pub api_url: Option<String>,
    pub debug: bool,
    pub proxy: Option<String>,
    pub ignore_patterns: Vec<String>,
    pub hide_file_names: bool,
    pub hide_project_names: bool,
    pub hide_branch_names: bool,
    pub hide_project_folder: bool,
    pub exclude_unknown_project: bool,
    pub include_patterns: Vec<String>,
    pub disable_offline: bool,
    pub guess_language: bool,
    pub hostname: Option<String>,
    pub log_file: Option<String>,
    pub no_ssl_verify: bool,
    pub ssl_certs_file: Option<String>,
    pub metrics: bool,
    pub include_only_with_project_file: bool,
    pub sync_config: SyncConfig,
}
```

### 4.3 SyncStatus Enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SyncStatus {
    #[default]
    Pending,          // Waiting to be synced
    Syncing,          // Currently being synced
    Synced,           // Successfully synced
    Failed,           // Sync failed (will retry)
    PermanentFailure, // Sync failed permanently
}
```

### 4.4 QueueEntry Struct

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueEntry {
    pub heartbeat: Heartbeat,
    pub sync_status: SyncStatus,
    pub sync_metadata: Option<String>,
    pub retry_count: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_attempt: Option<chrono::DateTime<chrono::Utc>>,
}
```

---

## 5. Entry Point and Command Structure

### 5.1 Main Entry Point Flow

```
main()
  │
  ├─> Cli::parse()                    // Parse CLI arguments
  │
  ├─> Check --version flag            // Print version and exit
  │
  ├─> Check --today flag              // Fetch today's activity
  │   ├─> setup_logging()
  │   ├─> Config::load()
  │   └─> fetch_today_activity()
  │
  ├─> Check --config-read/write       // Config operations
  │   └─> handle_config_operations()
  │
  ├─> Check --offline-count           // Show queue stats
  │   ├─> setup_logging()
  │   ├─> Config::load()
  │   └─> HeartbeatManager::get_queue_stats()
  │
  ├─> Check --extra-heartbeats        // Process STDIN heartbeats
  │   ├─> setup_logging()
  │   ├─> Config::load()
  │   └─> process_extra_heartbeats()
  │
  ├─> Check --sync-offline-activity   // Manual sync
  │   ├─> setup_logging()
  │   ├─> Config::load()
  │   └─> HeartbeatManager::manual_sync()
  │
  └─> Default: Process heartbeat      // Normal operation
      ├─> setup_logging()
      ├─> Config::load()
      ├─> HeartbeatManager::new()
      └─> HeartbeatManager::process()
```

### 5.2 Heartbeat Processing Flow

```
HeartbeatManager::process(cli)
  │
  ├─> should_ignore_entity()          // Check ignore patterns
  │
  ├─> create_heartbeat()              // Build Heartbeat from CLI
  │   ├─> DataCollector::detect_project()
  │   ├─> DataCollector::detect_git_info()
  │   └─> DataCollector::detect_language()
  │
  ├─> Queue::add()                    // Offline-first: queue first
  │
  └─> process_queue()                 // Then sync
      │
      ├─> Queue::get_pending()        // Fetch batch
      │
      ├─> ApiClient::send_heartbeats_batch()  // Try batch send
      │
      ├─> On success:
      │   └─> Queue::update_sync_status(Synced) + remove()
      │
      └─> On failure:
          ├─> Rate limit: backoff and retry
          └─> Other: per-heartbeat retry with backoff
```

---

## 6. Async and Concurrency Patterns

### 6.1 Tokio Runtime Configuration

```rust
#[tokio::main]
async fn main() {
    // Full tokio features enabled in Cargo.toml
    // tokio = { version = "1.0", features = ["full"] }
}
```

### 6.2 Blocking Operations

SQLite operations run in `tokio::task::spawn_blocking` to avoid blocking the async runtime:

```rust
tokio::task::spawn_blocking(move || -> Result<(), anyhow::Error> {
    let q = Queue::new()?;
    q.add(heartbeat)?;
    Ok(())
}).await??;
```

### 6.3 Thread-Safe State

**AtomicBool for Connectivity**:
```rust
pub struct ChronovaSyncManager {
    pub connectivity_state: Arc<AtomicBool>,
    // ...
}

pub async fn check_connectivity(&self) -> Result<bool, SyncError> {
    let result = self.api_client.check_connectivity().await?;
    self.connectivity_state.store(result, Ordering::SeqCst);
    Ok(result)
}
```

**RwLock for Cached Values**:
```rust
pub struct ChronovaSyncManager {
    pub last_connectivity_check: Arc<RwLock<Option<SystemTime>>>,
    // ...
}

pub async fn time_since_last_check(&self) -> Option<Duration> {
    let last_check_guard = self.last_connectivity_check.read().await;
    last_check_guard.map(|time| time.elapsed().unwrap_or(Duration::ZERO))
}
```

### 6.4 Background Tasks

**Connectivity Monitoring**:
```rust
pub async fn start_connectivity_monitoring(&self) -> Result<(), SyncError> {
    let connectivity_state = Arc::clone(&self.connectivity_state);
    let api_client = self.api_client.clone();
    
    tokio::spawn(async move {
        loop {
            match api_client.check_connectivity().await {
                Ok(is_connected) => {
                    connectivity_state.store(is_connected, Ordering::SeqCst);
                }
                Err(e) => {
                    tracing::warn!("Connectivity monitoring failed: {}", e);
                    connectivity_state.store(false, Ordering::SeqCst);
                }
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
    
    Ok(())
}
```

**Background Sync**:
```rust
pub async fn start_background_sync(&self) -> Result<(), SyncError> {
    let sync_manager = self.clone();
    let sync_interval = Duration::from_secs(self.config.sync_interval_seconds);
    
    tokio::spawn(async move {
        loop {
            if sync_manager.check_connectivity().await? {
                sync_manager.sync_pending().await?;
            }
            tokio::time::sleep(sync_interval).await;
        }
    });
    
    Ok(())
}
```

---

## 7. External Dependencies

### 7.1 Core Dependencies

| Dependency | Version | Purpose |
|------------|---------|---------|
| clap | 4.4 | CLI argument parsing with derive macros |
| reqwest | 0.11 | HTTP client with JSON support and TLS |
| tokio | 1.0 | Async runtime with full features |
| serde | 1.0 | Serialization/deserialization |
| serde_json | 1.0 | JSON parsing |
| rusqlite | 0.30 | SQLite bindings with bundled SQLite |
| thiserror | 1.0 | Custom error type derivation |
| anyhow | 1.0 | Error propagation context |
| tracing | 0.1 | Structured logging framework |
| tracing-subscriber | 0.3 | Logging subscriber with env filter |
| chrono | 0.4 | Date/time handling |

### 7.2 Supporting Dependencies

| Dependency | Version | Purpose |
|------------|---------|---------|
| configparser | 3.0 | INI file parsing |
| dirs | 5.0 | Home directory detection |
| git2 | 0.18 | Git repository operations |
| uuid | 1.0 | UUID generation |
| base64 | 0.21 | Base64 encoding for auth |
| lazy_static | 1.4 | Static lazy initialization |
| gethostname | 0.4 | Hostname retrieval |
| tracing-appender | 0.2 | Non-blocking file appending |
| rand | 0.8 | Random number generation |
| async-trait | 0.1 | Async trait methods |
| sysinfo | 0.37.2 | System information |

### 7.3 Dev Dependencies

| Dependency | Purpose |
|------------|---------|
| tempfile | Temporary file handling for tests |
| wiremock | HTTP mocking for tests |
| assert_cmd | Command assertion testing |
| predicates | Predicate-based assertions |
| tokio-test | Async test utilities |

### 7.4 Key Dependency Usage Patterns

**Reqwest HTTP Client**:
```rust
let client = Client::builder()
    .timeout(Duration::from_secs(30))
    .build()?;

let response = client
    .post(&url)
    .header("Authorization", format!("Bearer {}", self.api_key))
    .json(heartbeat)
    .send()
    .await?;
```

**Tracing for Structured Logging**:
```rust
tracing::info!("Processing {} queued heartbeats", queued.len());
tracing::debug!("Attempting batch send for ids: {:?}", queued_ids);
tracing::warn!("Rate limited on batch sync, sleeping {}s", backoff_secs);
tracing::error!("Failed to parse extra heartbeats: {}", e);
```

**SQLite with rusqlite**:
```rust
let conn = Connection::open(db_path)?;
conn.execute(
    "CREATE TABLE IF NOT EXISTS heartbeats (
        id TEXT PRIMARY KEY,
        data TEXT NOT NULL,
        sync_status TEXT DEFAULT 'pending'
    )",
    [],
)?;
```

---

## 8. Configuration Files

### 8.1 Main Config File

**Location**: ~/.chronova.cfg (default)

**Format**: INI with [settings] section

**Example**:
```ini
[settings]
api_key = your_api_key_here
api_url = https://chronova.dev/api/v1
debug = false
proxy = https://proxy.example.com:8080
hide_file_names = false
hide_project_names = false
exclude_unknown_project = false
offline = true
guess_language = false
metrics = false

; Sync configuration
sync_enabled = true
sync_max_queue_size = 1000
sync_interval = 300
sync_max_retries = 5
sync_retry_base_delay = 1
sync_retry_max_delay = 60
sync_retry_use_jitter = true
sync_retention_days = 7
sync_background = true

; Patterns (newline-separated)
exclude =
    COMMIT_EDITMSG$
    PULLREQ_EDITMSG$
    *.tmp
    *.log

include =
    *.rs
    *.py
    *.js
```

### 8.2 Queue Database

**Location**: ~/.chronova/queue.db

**Schema**:
```sql
CREATE TABLE heartbeats (
    id TEXT PRIMARY KEY,
    data TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    retry_count INTEGER DEFAULT 0,
    last_attempt DATETIME,
    sync_status TEXT DEFAULT 'pending',
    sync_metadata TEXT
);

CREATE TABLE schema_version (
    version INTEGER PRIMARY KEY,
    applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_heartbeats_sync_status ON heartbeats(sync_status);
CREATE INDEX idx_heartbeats_created_at ON heartbeats(created_at);
CREATE INDEX idx_heartbeats_retry_count ON heartbeats(retry_count);
```

### 8.3 Log File

**Location**: ~/.chronova.log

**Format**: Plain text with timestamps (when not in JSON output mode)

---

## 9. Testing Patterns

### 9.1 Test Organization

- Unit tests in `#[cfg(test)]` modules within each source file
- Tests use wiremock for HTTP mocking
- Tests use tempfile for isolated file operations
- Async tests use tokio_test

### 9.2 Example Test Pattern

```rust
#[tokio::test]
async fn test_send_heartbeat_success() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/users/current/heartbeats"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri());
    let heartbeat = create_test_heartbeat();

    let result = client.send_heartbeat(&heartbeat).await;
    assert!(result.is_ok());
}
```

### 9.3 Test Coverage Areas

- Config loading and parsing
- Queue operations (add, get, remove, update)
- Sync status transitions
- API client authentication methods
- Language detection
- Project detection
- Git info collection
- User agent generation
- Retry strategy calculations

---

## 10. Build and Release

### 10.1 Release Profile

```toml
[profile.release]
lto = true          # Link-time optimization
panic = 'abort'     # Smaller binaries
opt-level = 'z'     # Optimize for size
```

### 10.2 Cross-Compilation

```toml
[target.aarch64-apple-darwin]
image = "aarch64-apple-darwin-cross.local"
```

### 10.3 Binary Output

- Binary name: chronova-cli
- Output directory: target/release/chronova-cli

---

## 11. Common Patterns for LLM Agents

### 11.1 Adding a New CLI Flag

1. Add field to `Cli` struct in cli.rs with `#[arg(long)]` attribute
2. Add handling in main.rs if-then chain
3. Update help text with doc comment on field
4. Add tests if functionality is complex

### 11.2 Adding a New Config Option

1. Add field to `Config` struct in config.rs
2. Parse in `Config::load()` method
3. Add to `Default` implementation if applicable
4. Update config documentation

### 11.3 Adding a New API Endpoint

1. Add method to `ApiClient` or `AuthenticatedApiClient` in api.rs
2. Add response struct if needed
3. Handle multiple auth methods (Bearer, Basic, X-API-Key)
4. Add tests with wiremock

### 11.4 Modifying Queue Behavior

1. Understand `QueueOps` trait in queue.rs
2. Update `Queue` struct implementation
3. Consider migration for existing databases
4. Update heartbeat.rs if queue usage changes

### 11.5 Adding Sync Logic

1. Implement `SyncManager` trait or extend `ChronovaSyncManager`
2. Use `tokio::spawn` for background tasks
3. Use atomic types for shared state
4. Add retry logic with `RetryStrategy`

---

## 12. Key File Locations

| File | Purpose |
|------|---------|
| src/main.rs | Entry point, CLI routing |
| src/cli.rs | CLI argument definitions |
| src/config.rs | Configuration management |
| src/heartbeat.rs | Heartbeat processing |
| src/api.rs | HTTP client, API calls |
| src/queue.rs | SQLite queue operations |
| src/sync.rs | Sync management, retry logic |
| src/collector.rs | Data collection |
| src/logger.rs | Logging setup |
| src/user_agent.rs | User agent generation |
| Cargo.toml | Dependencies, build config |
| tests/ | Integration tests |

---

## 13. Important Constants and Defaults

| Constant | Value | Location |
|----------|-------|----------|
| Default API URL | https://chronova.dev/api/v1 | config.rs |
| Default timeout | 30 seconds | api.rs |
| Batch size | 50 | sync.rs |
| Max retries | 3 | heartbeat.rs |
| Retry base delay | 1 second | sync.rs |
| Retry max delay | 60 seconds | sync.rs |
| Sync interval | 300 seconds (5 min) | sync.rs |
| Queue retention | 7 days | queue.rs |
| Default config path | ~/.chronova.cfg | cli.rs |
| Default log path | ~/.chronova.log | logger.rs |
| Default queue path | ~/.chronova/queue.db | queue.rs |

---

## 14. Troubleshooting Guide

### 14.1 Common Errors

**ConfigError::ParseError**: Invalid INI format in config file
**ConfigError::NotFound**: Config file doesn't exist (uses defaults)
**ApiError::Auth**: Invalid API key or authentication method
**ApiError::RateLimit**: Too many requests, implement backoff
**QueueError::DatabaseCorruption**: SQLite database corrupted, automatic recovery attempted
**SyncError::Network**: Network connectivity issues

### 14.2 Debug Mode

Use `--verbose` flag to enable debug logging:
```bash
chronova-cli --entity /path/to/file.rs --verbose
```

### 14.3 Log File Location

Logs are written to ~/.chronova.log by default. Check this file for detailed error information.

---

## 15. Future Considerations

- Background sync daemon mode
- Plugin system for data collection
- Additional API endpoints
- Metrics and analytics
- Team features integration
- Web dashboard integration