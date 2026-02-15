//! Chronova CLI Library
//!
//! A high-performance, drop-in replacement for wakatime-cli written in Rust.

pub mod api;
pub mod cli;
pub mod collector;
pub mod config;
pub mod heartbeat;
pub mod logger;
pub mod queue;
pub mod sync;
pub mod user_agent;

// Re-export commonly used types for easier access
pub use api::ApiClient;
pub use cli::Cli;
pub use config::Config;
pub use heartbeat::HeartbeatManager;
pub use queue::Queue;
pub use sync::{ChronovaSyncManager, PerformanceMetrics, SyncResult, SyncConfig};
