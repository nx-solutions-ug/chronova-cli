use rusqlite::{Connection, params, OptionalExtension};
use std::path::PathBuf;
use thiserror::Error;
use serde::{Deserialize, Serialize};

use crate::heartbeat::Heartbeat;
use crate::sync::{SyncStatus, SyncStatusSummary};

#[derive(Error, Debug)]
pub enum QueueError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Sync status not found for entry: {0}")]
    SyncStatusNotFound(String),
    #[error("Invalid sync status: {0}")]
    InvalidSyncStatus(String),
    #[error("Queue entry not found: {0}")]
    EntryNotFound(String),
    #[error("Queue is full, maximum capacity reached")]
    QueueFull,
    #[error("Storage limit exceeded")]
    StorageLimitExceeded,
    #[error("Database corruption detected: {0}")]
    DatabaseCorruption(String),
}

/// Represents a queue entry with sync metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueEntry {
    /// The heartbeat data
    pub heartbeat: Heartbeat,
    /// Current sync status
    pub sync_status: SyncStatus,
    /// Sync metadata (error messages, retry info, etc.)
    pub sync_metadata: Option<String>,
    /// Number of sync attempts
    pub retry_count: u32,
    /// Timestamp when this entry was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Timestamp of last sync attempt
    pub last_attempt: Option<chrono::DateTime<chrono::Utc>>,
}

impl QueueEntry {
    pub fn new(heartbeat: Heartbeat) -> Self {
        Self {
            heartbeat,
            sync_status: SyncStatus::Pending,
            sync_metadata: None,
            retry_count: 0,
            created_at: chrono::Utc::now(),
            last_attempt: None,
        }
    }
}

/// Represents queue statistics
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct QueueStats {
    /// Total number of entries
    pub total_count: usize,
    /// Number of pending entries
    pub pending_count: usize,
    /// Number of syncing entries
    pub syncing_count: usize,
    /// Number of synced entries
    pub synced_count: usize,
    /// Number of failed entries
    pub failed_count: usize,
    /// Number of permanent failures
    pub permanent_failure_count: usize,
    /// Oldest entry timestamp
    pub oldest_entry: Option<chrono::DateTime<chrono::Utc>>,
    /// Newest entry timestamp
    pub newest_entry: Option<chrono::DateTime<chrono::Utc>>,
}


/// Trait defining the queue operations for offline heartbeat synchronization
pub trait QueueOps {
    /// Add a heartbeat to the queue
    fn add(&self, heartbeat: Heartbeat) -> Result<(), QueueError>;

    /// Get pending heartbeats (with optional sync status filtering)
    fn get_pending(&self, limit: Option<usize>, status_filter: Option<SyncStatus>) -> Result<Vec<Heartbeat>, QueueError>;

    /// Remove a heartbeat from the queue by ID
    fn remove(&self, id: &str) -> Result<(), QueueError>;

    /// Update the sync status of a heartbeat
    fn update_sync_status(&self, id: &str, status: SyncStatus, metadata: Option<String>) -> Result<(), QueueError>;

    /// Count heartbeats by sync status
    fn count_by_status(&self, status: Option<SyncStatus>) -> Result<usize, QueueError>;

    /// Get sync statistics
    fn get_sync_stats(&self) -> Result<SyncStatusSummary, QueueError>;

    /// Clean up old entries based on retention policy
    fn cleanup_old_entries(&self, max_age_days: i32) -> Result<usize, QueueError>;

    /// Enforce maximum queue size by removing oldest entries
    fn enforce_max_count(&self, max_count: usize) -> Result<usize, QueueError>;

    /// Vacuum database to optimize storage
    fn vacuum(&self) -> Result<(), QueueError>;

    /// Deduplicate heartbeats based on entity and time window
    fn deduplicate(&self, time_window_seconds: i64) -> Result<usize, QueueError>;

    /// Increment retry count for a heartbeat
    fn increment_retry(&self, id: &str) -> Result<(), QueueError>;

    /// Get retry count for a heartbeat
    fn get_retry_count(&self, id: &str) -> Result<u32, QueueError>;

    /// Get total count of heartbeats in queue
    fn count(&self) -> Result<usize, QueueError>;
}

pub struct Queue {
    conn: Connection,
}

impl QueueOps for Queue {
    fn add(&self, heartbeat: Heartbeat) -> Result<(), QueueError> {
        let data = serde_json::to_string(&heartbeat)?;

        // Ensure sync_status is explicitly set on insert so rows are queryable
        // regardless of whether the column default is present in the schema.
        self.conn.execute(
            "INSERT OR REPLACE INTO heartbeats (id, data, sync_status) VALUES (?1, ?2, 'pending')",
            params![heartbeat.id, data],
        )?;

        // Log queue operation with metrics
        let current_count = self.count()?;
        tracing::info!(
            operation = "add",
            heartbeat_id = %heartbeat.id,
            queue_size = current_count,
            entity = %heartbeat.entity,
            project = ?heartbeat.project,
            "Heartbeat added to queue"
        );

        Ok(())
    }

    fn get_pending(&self, limit: Option<usize>, status_filter: Option<SyncStatus>) -> Result<Vec<Heartbeat>, QueueError> {
        let limit = limit.unwrap_or(100);
        let status_filter = status_filter.unwrap_or(SyncStatus::Pending);
        let status_str: String = status_filter.into();

        let mut stmt = self.conn.prepare(
            "SELECT data FROM heartbeats WHERE sync_status = ?1 ORDER BY created_at ASC LIMIT ?2"
        )?;

        let heartbeats_iter = stmt.query_map(params![status_str, limit], |row| {
            let data: String = row.get(0)?;
            serde_json::from_str::<Heartbeat>(&data).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
            })
        })?;

        let mut heartbeats = Vec::new();
        for heartbeat in heartbeats_iter {
            heartbeats.push(heartbeat?);
        }

        Ok(heartbeats)
    }

    fn remove(&self, id: &str) -> Result<(), QueueError> {
        self.conn.execute(
            "DELETE FROM heartbeats WHERE id = ?1",
            params![id],
        )?;

        // Log queue operation with metrics
        let current_count = self.count()?;
        tracing::info!(
            operation = "remove",
            heartbeat_id = %id,
            queue_size = current_count,
            "Heartbeat removed from queue"
        );

        Ok(())
    }

    fn update_sync_status(&self, id: &str, status: SyncStatus, metadata: Option<String>) -> Result<(), QueueError> {
        let status_str: String = status.into();

        self.conn.execute(
            "UPDATE heartbeats SET sync_status = ?1, sync_metadata = ?2, last_attempt = CURRENT_TIMESTAMP WHERE id = ?3",
            params![status_str, metadata, id],
        )?;

        // Log sync status update
        tracing::debug!(
            operation = "update_sync_status",
            heartbeat_id = %id,
            sync_status = %status_str,
            metadata = ?metadata,
            "Heartbeat sync status updated"
        );

        Ok(())
    }

    fn count_by_status(&self, status: Option<SyncStatus>) -> Result<usize, QueueError> {
        let count: usize = if let Some(status) = status {
            let status_str: String = status.into();
            self.conn.query_row(
                "SELECT COUNT(*) FROM heartbeats WHERE sync_status = ?1",
                params![status_str],
                |row| row.get(0),
            )?
        } else {
            self.conn.query_row(
                "SELECT COUNT(*) FROM heartbeats",
                [],
                |row| row.get(0),
            )?
        };

        Ok(count)
    }

    fn get_sync_stats(&self) -> Result<SyncStatusSummary, QueueError> {
        let mut summary = SyncStatusSummary::default();

        // Get counts for each status
        for status in &[SyncStatus::Pending, SyncStatus::Syncing, SyncStatus::Synced, SyncStatus::Failed, SyncStatus::PermanentFailure] {
            let status_str: String = (*status).into();
            let count: usize = self.conn.query_row(
                "SELECT COUNT(*) FROM heartbeats WHERE sync_status = ?1",
                params![status_str],
                |row| row.get(0),
            )?;

            match status {
                SyncStatus::Pending => summary.pending = count,
                SyncStatus::Syncing => summary.syncing = count,
                SyncStatus::Synced => summary.synced = count,
                SyncStatus::Failed => summary.failed = count,
                SyncStatus::PermanentFailure => summary.permanent_failures = count,
            }
        }

        summary.total = self.count()?;

        // Get last sync attempt timestamp - handle NULL case properly
        let last_sync: Option<String> = self.conn.query_row(
            "SELECT MAX(last_attempt) FROM heartbeats WHERE last_attempt IS NOT NULL",
            [],
            |row| row.get::<_, Option<String>>(0),
        ).ok().flatten();

        if let Some(last_sync_str) = last_sync {
            // Parse the timestamp (SQLite format: YYYY-MM-DD HH:MM:SS)
            if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&last_sync_str, "%Y-%m-%d %H:%M:%S") {
                summary.last_sync = Some(std::time::SystemTime::from(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc)));
            }
        }

        Ok(summary)
    }

    fn cleanup_old_entries(&self, max_age_days: i32) -> Result<usize, QueueError> {
        // Handle special case: max_age_days = 0 means remove all entries
        if max_age_days == 0 {
            let rows_affected = self.conn.execute("DELETE FROM heartbeats", [])?;

            // Log cleanup operation
            if rows_affected > 0 {
                tracing::info!(
                    operation = "cleanup_old_entries",
                    max_age_days = max_age_days,
                    entries_removed = rows_affected,
                    queue_size_after_cleanup = 0,
                    "All entries cleaned up from queue"
                );
            }

            return Ok(rows_affected);
        }

        // Calculate the cutoff timestamp (current time minus max_age_days)
        let cutoff_datetime = chrono::Utc::now() - chrono::Duration::days(max_age_days as i64);
        let cutoff_str = cutoff_datetime.format("%Y-%m-%d %H:%M:%S").to_string();

        // Remove entries older than the cutoff date
        let rows_affected = self.conn.execute(
            "DELETE FROM heartbeats WHERE created_at < ?1",
            params![cutoff_str],
        )?;

        // Log cleanup operation
        if rows_affected > 0 {
            let current_count = self.count()?;
            tracing::info!(
                operation = "cleanup_old_entries",
                max_age_days = max_age_days,
                entries_removed = rows_affected,
                queue_size_after_cleanup = current_count,
                "Old entries cleaned up from queue"
            );
        }

        Ok(rows_affected)
    }

    fn enforce_max_count(&self, max_count: usize) -> Result<usize, QueueError> {
        let current_count: usize = self.count()?;

        if current_count <= max_count {
            return Ok(0);
        }

        let excess = current_count - max_count;

        let rows_affected = self.conn.execute(
            "DELETE FROM heartbeats WHERE id IN (
                SELECT id FROM heartbeats
                ORDER BY created_at ASC
                LIMIT ?
            )",
            params![excess],
        )?;

        // Log max count enforcement
        if rows_affected > 0 {
            let new_count = self.count()?;
            tracing::info!(
                operation = "enforce_max_count",
                max_count = max_count,
                previous_count = current_count,
                entries_removed = rows_affected,
                new_count = new_count,
                "Queue size enforced to maximum limit"
            );
        }

        Ok(rows_affected)
    }

    fn vacuum(&self) -> Result<(), QueueError> {
        tracing::info!(
            operation = "vacuum",
            "Starting database vacuum operation"
        );

        self.conn.execute("VACUUM", [])?;

        tracing::info!(
            operation = "vacuum",
            "Database vacuum completed successfully"
        );

        Ok(())
    }

    fn deduplicate(&self, time_window_seconds: i64) -> Result<usize, QueueError> {
        // Remove duplicate heartbeats within the same time window
        // Keep the most recent heartbeat for each entity within the time window
        let rows_affected = self.conn.execute(
            "DELETE FROM heartbeats
            WHERE id IN (
                SELECT h1.id
                FROM heartbeats h1
                JOIN heartbeats h2 ON
                    h1.id != h2.id AND
                    h1.entity = h2.entity AND
                    ABS(h1.time - h2.time) < ?1
                WHERE h1.time < h2.time
            )",
            params![time_window_seconds],
        )?;

        // Log deduplication results
        if rows_affected > 0 {
            let current_count = self.count()?;
            tracing::info!(
                operation = "deduplicate",
                time_window_seconds = time_window_seconds,
                duplicates_removed = rows_affected,
                queue_size_after_dedup = current_count,
                "Heartbeat deduplication completed"
            );
        }

        Ok(rows_affected)
    }

    fn increment_retry(&self, id: &str) -> Result<(), QueueError> {
        self.conn.execute(
            "UPDATE heartbeats SET retry_count = retry_count + 1, last_attempt = CURRENT_TIMESTAMP WHERE id = ?1",
            params![id],
        )?;

        // Log retry increment
        let retry_count = self.get_retry_count(id)?;
        tracing::debug!(
            operation = "increment_retry",
            heartbeat_id = %id,
            retry_count = retry_count,
            "Heartbeat retry count incremented"
        );

        Ok(())
    }

    fn get_retry_count(&self, id: &str) -> Result<u32, QueueError> {
        let count: u32 = self.conn.query_row(
            "SELECT retry_count FROM heartbeats WHERE id = ?1",
            params![id],
            |row| row.get(0),
        ).optional()?.unwrap_or(0);

        Ok(count)
    }

    fn count(&self) -> Result<usize, QueueError> {
        let count: usize = self.conn.query_row(
            "SELECT COUNT(*) FROM heartbeats",
            [],
            |row| row.get(0),
        )?;

        // Log queue size periodically for monitoring
        if count.is_multiple_of(10) { // Log every 10 operations to avoid spam
            tracing::debug!(
                operation = "count",
                queue_size = count,
                "Current queue size"
            );
        }

        Ok(count)
    }
}

impl Queue {
    pub fn new() -> Result<Self, QueueError> {
        let db_path = Self::get_db_path()?;
        let conn = Self::open_with_corruption_handling(&db_path)?;

        // Initialize the database
        Self::init_database(&conn)?;

        Ok(Self { conn })
    }

    /// Create a Queue with a custom database path for testing
    pub fn with_path(db_path: PathBuf) -> Result<Self, QueueError> {
        let conn = Self::open_with_corruption_handling(&db_path)?;

        // Initialize the database
        Self::init_database(&conn)?;

        Ok(Self { conn })
    }

    /// Initialize database schema and indexes
    fn init_database(conn: &Connection) -> Result<(), QueueError> {
        // Create table if it doesn't exist with initial schema
        conn.execute(
            "CREATE TABLE IF NOT EXISTS heartbeats (
                id TEXT PRIMARY KEY,
                data TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                retry_count INTEGER DEFAULT 0,
                last_attempt DATETIME
            )",
            [],
        )?;

        // Create schema version table if it doesn't exist
        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY,
                applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // Get current schema version
        let current_version: i32 = conn
            .query_row("SELECT version FROM schema_version ORDER BY version DESC LIMIT 1", [], |row| row.get(0))
            .optional()?
            .unwrap_or(0);

        // Apply migrations if needed
        if current_version < 1 {
            // Check if sync_status column already exists before adding it
            let columns: Vec<String> = conn
                .prepare("PRAGMA table_info(heartbeats)")?
                .query_map([], |row| row.get(1))?
                .collect::<Result<Vec<_>, _>>()?;

            if !columns.contains(&"sync_status".to_string()) {
                conn.execute(
                    "ALTER TABLE heartbeats ADD COLUMN sync_status TEXT DEFAULT 'pending'",
                    [],
                )?;
            }

            if !columns.contains(&"sync_metadata".to_string()) {
                conn.execute(
                    "ALTER TABLE heartbeats ADD COLUMN sync_metadata TEXT",
                    [],
                )?;
            }

            // Update schema version
            conn.execute(
                "INSERT INTO schema_version (version) VALUES (1)",
                [],
            )?;
        }

        // Create indexes for sync performance
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_heartbeats_sync_status ON heartbeats(sync_status)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_heartbeats_created_at ON heartbeats(created_at)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_heartbeats_retry_count ON heartbeats(retry_count)",
            [],
        )?;

        Ok(())
    }

    /// Open database connection with corruption handling
    fn open_with_corruption_handling(db_path: &PathBuf) -> Result<Connection, QueueError> {
        // First attempt to open normally
        match Connection::open(db_path) {
            Ok(conn) => {
                // Verify database integrity
                if let Err(e) = Self::verify_database_integrity(&conn) {
                    // Close the corrupted connection
                    drop(conn);

                    // Attempt recovery
                    return Self::attempt_database_recovery(db_path);
                }
                Ok(conn)
            }
            Err(e) => {
                Self::attempt_database_recovery(db_path)
            }
        }
    }

    /// Verify database integrity using PRAGMA integrity_check
    fn verify_database_integrity(conn: &Connection) -> Result<(), QueueError> {
        let result: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;

        if result.to_lowercase() == "ok" {
            Ok(())
        } else {
            Err(QueueError::DatabaseCorruption(format!("Database integrity check failed: {}", result)))
        }
    }

    /// Attempt database recovery by creating a new database and migrating data
    fn attempt_database_recovery(db_path: &PathBuf) -> Result<Connection, QueueError> {
        let backup_path = db_path.with_extension("db.backup");

        // Create backup of corrupted database
        if db_path.exists() {
            std::fs::copy(db_path, &backup_path).map_err(|e| {
                QueueError::DatabaseCorruption(format!("Failed to create backup: {}", e))
            })?;
        }

        // Remove corrupted database
        if db_path.exists() {
            std::fs::remove_file(db_path).map_err(|e| {
                QueueError::DatabaseCorruption(format!("Failed to remove corrupted database: {}", e))
            })?;
        }

        // Create new database
        let conn = Connection::open(db_path)?;

        // Recreate schema
        conn.execute(
            "CREATE TABLE IF NOT EXISTS heartbeats (
                id TEXT PRIMARY KEY,
                data TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                retry_count INTEGER DEFAULT 0,
                last_attempt DATETIME,
                sync_status TEXT DEFAULT 'pending',
                sync_metadata TEXT
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY,
                applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // Create indexes
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_heartbeats_sync_status ON heartbeats(sync_status)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_heartbeats_created_at ON heartbeats(created_at)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_heartbeats_retry_count ON heartbeats(retry_count)",
            [],
        )?;

        Ok(conn)
    }

    fn get_db_path() -> Result<PathBuf, QueueError> {
        let mut chronova_dir = dirs::home_dir()
            .ok_or_else(|| rusqlite::Error::InvalidPath("Could not determine home directory".to_string().into()))?;

        chronova_dir.push(".chronova");
        std::fs::create_dir_all(&chronova_dir)?;

        chronova_dir.push("queue.db");
        Ok(chronova_dir)
    }
}

impl Drop for Queue {
    fn drop(&mut self) {
        // Clean up old entries on shutdown (older than 7 days)
        let _ = self.cleanup_old_entries(7);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    use rusqlite::Connection;

    #[test]
    fn test_queue_error_variants() {
        // Test Database error
        let db_error = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(1),
            Some("test error".to_string())
        );
        let queue_error = QueueError::Database(db_error);
        assert!(queue_error.to_string().contains("Database error:"));

        // Test Serialization error
        let invalid_json = "invalid json";
        let serde_error = serde_json::from_str::<serde_json::Value>(invalid_json).unwrap_err();
        let queue_error = QueueError::Serialization(serde_error);
        assert!(queue_error.to_string().contains("Serialization error:"));

        // Test IO error
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let queue_error = QueueError::Io(io_error);
        assert!(queue_error.to_string().contains("IO error:"));

        // Test sync-specific errors
        let sync_status_error = QueueError::SyncStatusNotFound("test-id".to_string());
        assert!(sync_status_error.to_string().contains("Sync status not found for entry: test-id"));

        let invalid_status_error = QueueError::InvalidSyncStatus("invalid".to_string());
        assert!(invalid_status_error.to_string().contains("Invalid sync status: invalid"));

        let not_found_error = QueueError::EntryNotFound("test-id".to_string());
        assert!(not_found_error.to_string().contains("Queue entry not found: test-id"));

        let queue_full_error = QueueError::QueueFull;
        assert_eq!(queue_full_error.to_string(), "Queue is full, maximum capacity reached");

        let storage_limit_error = QueueError::StorageLimitExceeded;
        assert_eq!(storage_limit_error.to_string(), "Storage limit exceeded");

        let corruption_error = QueueError::DatabaseCorruption("corrupted data".to_string());
        assert!(corruption_error.to_string().contains("Database corruption detected: corrupted data"));
    }

    #[test]
    fn test_queue_error_clone() {
        let queue_error = QueueError::QueueFull;
        // Since QueueError doesn't implement Clone, we test the string representation directly
        assert_eq!(queue_error.to_string(), "Queue is full, maximum capacity reached");
    }

    #[test]
    fn test_queue_error_debug() {
        let queue_error = QueueError::QueueFull;
        let debug_output = format!("{:?}", queue_error);
        assert!(debug_output.contains("QueueFull"));
    }

    fn create_test_queue_with_old_schema() -> Result<(tempfile::TempDir, Queue), QueueError> {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_queue.db");
        let conn = Connection::open(&db_path)?;

        // Create old schema without sync_status and sync_metadata columns
        conn.execute(
            "CREATE TABLE heartbeats (
                id TEXT PRIMARY KEY,
                data TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                retry_count INTEGER DEFAULT 0,
                last_attempt DATETIME
            )",
            [],
        )?;

        Ok((temp_dir, Queue { conn }))
    }

    fn create_test_queue_with_new_schema() -> Result<(tempfile::TempDir, Queue), QueueError> {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_queue.db");
        let conn = Connection::open(&db_path)?;

        // Create new schema with sync_status and sync_metadata columns
        conn.execute(
            "CREATE TABLE heartbeats (
                id TEXT PRIMARY KEY,
                data TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                retry_count INTEGER DEFAULT 0,
                last_attempt DATETIME,
                sync_status TEXT DEFAULT 'pending',
                sync_metadata TEXT
            )",
            [],
        )?;

        Ok((temp_dir, Queue { conn }))
    }

    #[test]
    fn test_database_migration_adds_sync_columns() -> Result<(), QueueError> {
        let (_temp_dir, queue) = create_test_queue_with_old_schema()?;

        // Verify old schema doesn't have sync columns
        let columns: Vec<String> = queue.conn
            .prepare("PRAGMA table_info(heartbeats)")?
            .query_map([], |row| row.get(1))?
            .collect::<Result<Vec<_>, _>>()?;

        assert!(!columns.contains(&"sync_status".to_string()));
        assert!(!columns.contains(&"sync_metadata".to_string()));

        // Apply migration
        queue.conn.execute(
            "ALTER TABLE heartbeats ADD COLUMN sync_status TEXT DEFAULT 'pending'",
            [],
        )?;

        queue.conn.execute(
            "ALTER TABLE heartbeats ADD COLUMN sync_metadata TEXT",
            [],
        )?;

        // Verify new columns exist
        let columns: Vec<String> = queue.conn
            .prepare("PRAGMA table_info(heartbeats)")?
            .query_map([], |row| row.get(1))?
            .collect::<Result<Vec<_>, _>>()?;

        assert!(columns.contains(&"sync_status".to_string()));
        assert!(columns.contains(&"sync_metadata".to_string()));

        Ok(())
    }

    #[test]
    fn test_sync_status_default_value() -> Result<(), QueueError> {
        let (_temp_dir, queue) = create_test_queue_with_new_schema()?;

        // Insert a heartbeat
        queue.conn.execute(
            "INSERT INTO heartbeats (id, data) VALUES (?1, ?2)",
            ["test-id", "{}"],
        )?;

        // Verify default sync_status is 'pending'
        let sync_status: String = queue.conn
            .query_row(
                "SELECT sync_status FROM heartbeats WHERE id = ?1",
                ["test-id"],
                |row| row.get(0),
            )?;

        assert_eq!(sync_status, "pending");

        Ok(())
    }

    #[test]
    fn test_sync_metadata_nullable() -> Result<(), QueueError> {
        let (_temp_dir, queue) = create_test_queue_with_new_schema()?;

        // Insert a heartbeat without sync_metadata
        queue.conn.execute(
            "INSERT INTO heartbeats (id, data) VALUES (?1, ?2)",
            ["test-id", "{}"],
        )?;

        // Verify sync_metadata is NULL
        let sync_metadata: Option<String> = queue.conn
            .query_row(
                "SELECT sync_metadata FROM heartbeats WHERE id = ?1",
                ["test-id"],
                |row| row.get(0),
            )?;

        assert!(sync_metadata.is_none());

        Ok(())
    }

    #[test]
    fn test_database_indexes_for_sync_performance() -> Result<(), QueueError> {
        let (_temp_dir, queue) = create_test_queue_with_new_schema()?;

        // Create indexes for sync performance
        queue.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_heartbeats_sync_status ON heartbeats(sync_status)",
            [],
        )?;

        queue.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_heartbeats_created_at ON heartbeats(created_at)",
            [],
        )?;

        queue.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_heartbeats_retry_count ON heartbeats(retry_count)",
            [],
        )?;

        // Verify indexes exist
        let indexes: Vec<String> = queue.conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND name LIKE 'idx_heartbeats_%'")?
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        assert!(indexes.contains(&"idx_heartbeats_sync_status".to_string()));
        assert!(indexes.contains(&"idx_heartbeats_created_at".to_string()));
        assert!(indexes.contains(&"idx_heartbeats_retry_count".to_string()));

        Ok(())
    }

    #[test]
    fn test_schema_versioning_table() -> Result<(), QueueError> {
        let (_temp_dir, queue) = create_test_queue_with_new_schema()?;

        // Create schema version table
        queue.conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY,
                applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // Insert current version
        queue.conn.execute(
            "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
            [1],
        )?;

        // Verify version is stored
        let version: i32 = queue.conn
            .query_row(
                "SELECT version FROM schema_version",
                [],
                |row| row.get(0),
            )?;

        assert_eq!(version, 1);

        Ok(())
    }

    fn create_test_queue() -> Result<(tempfile::TempDir, Queue), QueueError> {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_queue.db");
        let conn = Connection::open(&db_path)?;

        conn.execute(
            "CREATE TABLE heartbeats (
                id TEXT PRIMARY KEY,
                data TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                retry_count INTEGER DEFAULT 0,
                last_attempt DATETIME,
                sync_status TEXT DEFAULT 'pending',
                sync_metadata TEXT
            )",
            [],
        )?;

        Ok((temp_dir, Queue { conn }))
    }

    fn create_test_heartbeat(id: &str) -> Heartbeat {
        Heartbeat {
            id: id.to_string(),
            entity: format!("/path/to/file_{}.rs", id),
            entity_type: "file".to_string(),
            time: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
            project: Some("test-project".to_string()),
            branch: Some("main".to_string()),
            language: Some("Rust".to_string()),
            is_write: false,
            lines: Some(100),
            lineno: Some(10),
            cursorpos: Some(5),
            user_agent: Some("test/1.0".to_string()),
            category: Some("coding".to_string()),
            machine: Some("test-machine".to_string()),
            editor: None,
            operating_system: None,
            commit_hash: None,
            commit_author: None,
            commit_message: None,
            repository_url: None,
            dependencies: Vec::new(),
        }
    }

    #[test]
    fn test_add_and_get_heartbeat() -> Result<(), QueueError> {
        let (_temp_dir, queue) = create_test_queue()?;
        let heartbeat = create_test_heartbeat("test-1");

        queue.add(heartbeat.clone())?;

        let pending = queue.get_pending(None, None)?;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, heartbeat.id);

        Ok(())
    }

    #[test]
    fn test_remove_heartbeat() -> Result<(), QueueError> {
        let (_temp_dir, queue) = create_test_queue()?;
        let heartbeat = create_test_heartbeat("test-2");

        queue.add(heartbeat.clone())?;
        assert_eq!(queue.count()?, 1);

        queue.remove(&heartbeat.id)?;
        assert_eq!(queue.count()?, 0);

        Ok(())
    }

    #[test]
    fn test_increment_retry() -> Result<(), QueueError> {
        let (_temp_dir, queue) = create_test_queue()?;
        let heartbeat = create_test_heartbeat("test-3");

        queue.add(heartbeat.clone())?;

        assert_eq!(queue.get_retry_count(&heartbeat.id)?, 0);

        queue.increment_retry(&heartbeat.id)?;
        assert_eq!(queue.get_retry_count(&heartbeat.id)?, 1);

        queue.increment_retry(&heartbeat.id)?;
        assert_eq!(queue.get_retry_count(&heartbeat.id)?, 2);

        Ok(())
    }

    #[test]
    fn test_multiple_heartbeats_order() -> Result<(), QueueError> {
        let (_temp_dir, queue) = create_test_queue()?;

        queue.add(create_test_heartbeat("test-1"))?;
        queue.add(create_test_heartbeat("test-2"))?;
        queue.add(create_test_heartbeat("test-3"))?;

        let pending = queue.get_pending(None, None)?;
        assert_eq!(pending.len(), 3);

        // Should be in insertion order (oldest first)
        assert_eq!(pending[0].id, "test-1");
        assert_eq!(pending[1].id, "test-2");
        assert_eq!(pending[2].id, "test-3");

        Ok(())
    }

    #[test]
    fn test_get_pending_with_status_filter() -> Result<(), QueueError> {
        let (_temp_dir, queue) = create_test_queue()?;
        let heartbeat = create_test_heartbeat("test-1");

        queue.add(heartbeat.clone())?;

        // Test with Pending status filter
        let pending = queue.get_pending(Some(10), Some(SyncStatus::Pending))?;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "test-1");

        // Test with different status filter (should return empty)
        let syncing = queue.get_pending(Some(10), Some(SyncStatus::Syncing))?;
        assert_eq!(syncing.len(), 0);

        Ok(())
    }

    #[test]
    fn test_update_sync_status() -> Result<(), QueueError> {
        let (_temp_dir, queue) = create_test_queue()?;
        let heartbeat = create_test_heartbeat("test-1");

        queue.add(heartbeat.clone())?;

        // Update status to Syncing
        queue.update_sync_status(&heartbeat.id, SyncStatus::Syncing, Some("syncing now".to_string()))?;

        // Verify the status was updated
        let pending = queue.get_pending(Some(10), Some(SyncStatus::Syncing))?;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "test-1");

        // Update status to Synced
        queue.update_sync_status(&heartbeat.id, SyncStatus::Synced, Some("success".to_string()))?;

        // Verify the status was updated again
        let synced = queue.get_pending(Some(10), Some(SyncStatus::Synced))?;
        assert_eq!(synced.len(), 1);
        assert_eq!(synced[0].id, "test-1");

        Ok(())
    }

    #[test]
    fn test_count_by_status() -> Result<(), QueueError> {
        let (_temp_dir, queue) = create_test_queue()?;

        // Add heartbeats with different statuses
        let h1 = create_test_heartbeat("test-1");
        let h2 = create_test_heartbeat("test-2");
        let h3 = create_test_heartbeat("test-3");

        queue.add(h1.clone())?;
        queue.add(h2.clone())?;
        queue.add(h3.clone())?;

        // Initially all should be pending
        assert_eq!(queue.count_by_status(Some(SyncStatus::Pending))?, 3);
        assert_eq!(queue.count_by_status(Some(SyncStatus::Syncing))?, 0);
        assert_eq!(queue.count_by_status(Some(SyncStatus::Synced))?, 0);

        // Update statuses
        queue.update_sync_status(&h1.id, SyncStatus::Syncing, None)?;
        queue.update_sync_status(&h2.id, SyncStatus::Synced, None)?;

        // Verify counts
        assert_eq!(queue.count_by_status(Some(SyncStatus::Pending))?, 1);
        assert_eq!(queue.count_by_status(Some(SyncStatus::Syncing))?, 1);
        assert_eq!(queue.count_by_status(Some(SyncStatus::Synced))?, 1);
        assert_eq!(queue.count_by_status(None)?, 3); // Total count

        Ok(())
    }

    #[test]
    fn test_get_sync_stats() -> Result<(), QueueError> {
        let (_temp_dir, queue) = create_test_queue()?;

        // Add heartbeats with different statuses
        let h1 = create_test_heartbeat("test-1");
        let h2 = create_test_heartbeat("test-2");
        let h3 = create_test_heartbeat("test-3");
        let h4 = create_test_heartbeat("test-4");
        let h5 = create_test_heartbeat("test-5");

        queue.add(h1.clone())?;
        queue.add(h2.clone())?;
        queue.add(h3.clone())?;
        queue.add(h4.clone())?;
        queue.add(h5.clone())?;

        // Set different statuses
        queue.update_sync_status(&h1.id, SyncStatus::Pending, None)?; // Already pending by default
        queue.update_sync_status(&h2.id, SyncStatus::Syncing, None)?;
        queue.update_sync_status(&h3.id, SyncStatus::Synced, None)?;
        queue.update_sync_status(&h4.id, SyncStatus::Failed, None)?;
        queue.update_sync_status(&h5.id, SyncStatus::PermanentFailure, None)?;

        let stats = queue.get_sync_stats()?;

        assert_eq!(stats.total, 5);
        assert_eq!(stats.pending, 1);
        assert_eq!(stats.syncing, 1);
        assert_eq!(stats.synced, 1);
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.permanent_failures, 1);

        // Last sync should be set (from the update operations)
        assert!(stats.last_sync.is_some());

        Ok(())
    }

    #[test]
    fn test_cleanup_old_entries() -> Result<(), QueueError> {
        let (_temp_dir, queue) = create_test_queue()?;
        let heartbeat = create_test_heartbeat("test-1");

        queue.add(heartbeat.clone())?;
        assert_eq!(queue.count()?, 1);

        // Manually set the created_at to an old date to test cleanup
        queue.conn.execute(
            "UPDATE heartbeats SET created_at = datetime('now', '-7 days') WHERE id = ?1",
            params![heartbeat.id],
        )?;

        // Clean up entries older than 1 day (should remove our manually aged entry)
        let removed = queue.cleanup_old_entries(1)?;
        assert_eq!(removed, 1);
        assert_eq!(queue.count()?, 0);

        Ok(())
    }

    #[test]
    fn test_enforce_max_count() -> Result<(), QueueError> {
        let (_temp_dir, queue) = create_test_queue()?;

        // Add more entries than the max count
        for i in 0..10 {
            queue.add(create_test_heartbeat(&format!("test-{}", i)))?;
        }

        assert_eq!(queue.count()?, 10);

        // Enforce max count of 5
        let removed = queue.enforce_max_count(5)?;
        assert_eq!(removed, 5);
        assert_eq!(queue.count()?, 5);

        // Verify oldest entries were removed (test-0 through test-4 should be gone)
        let remaining: Vec<String> = queue.conn
            .prepare("SELECT id FROM heartbeats ORDER BY created_at ASC")?
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        // Should have test-5 through test-9 (newest entries)
        assert_eq!(remaining.len(), 5);
        assert!(remaining.contains(&"test-5".to_string()));
        assert!(remaining.contains(&"test-9".to_string()));
        assert!(!remaining.contains(&"test-0".to_string()));
        assert!(!remaining.contains(&"test-4".to_string()));

        Ok(())
    }

    #[test]
    fn test_enforce_max_count_no_removal_when_under_limit() -> Result<(), QueueError> {
        let (_temp_dir, queue) = create_test_queue()?;

        // Add 3 entries
        for i in 0..3 {
            queue.add(create_test_heartbeat(&format!("test-{}", i)))?;
        }

        assert_eq!(queue.count()?, 3);

        // Enforce max count of 5 (should remove nothing)
        let removed = queue.enforce_max_count(5)?;
        assert_eq!(removed, 0);
        assert_eq!(queue.count()?, 3);

        Ok(())
    }

    #[test]
    fn test_vacuum() -> Result<(), QueueError> {
        let (_temp_dir, queue) = create_test_queue()?;

        // Add some entries
        for i in 0..5 {
            queue.add(create_test_heartbeat(&format!("test-{}", i)))?;
        }

        // Remove some entries to create fragmentation
        queue.remove("test-1")?;
        queue.remove("test-3")?;

        // Vacuum should succeed without errors
        queue.vacuum()?;

        // Verify data is still accessible after vacuum
        assert_eq!(queue.count()?, 3);
        let remaining = queue.get_pending(None, None)?;
        assert_eq!(remaining.len(), 3);

        Ok(())
    }

    #[test]
    fn test_queue_ops_trait_completeness() -> Result<(), QueueError> {
        let (_temp_dir, queue) = create_test_queue()?;
        let heartbeat = create_test_heartbeat("test-1");

        // Test all trait methods are implemented and work
        queue.add(heartbeat.clone())?;

        // Test get_pending with different parameters
        let _ = queue.get_pending(None, None)?;
        let _ = queue.get_pending(Some(5), Some(SyncStatus::Pending))?;

        // Test remove
        queue.remove(&heartbeat.id)?;
        assert_eq!(queue.count()?, 0);

        // Test update_sync_status
        queue.add(heartbeat.clone())?;
        queue.update_sync_status(&heartbeat.id, SyncStatus::Syncing, Some("test".to_string()))?;

        // Test count_by_status
        assert_eq!(queue.count_by_status(Some(SyncStatus::Syncing))?, 1);

        // Test get_sync_stats
        let _ = queue.get_sync_stats()?;

        // Test cleanup_old_entries
        let _ = queue.cleanup_old_entries(7)?;

        // Test enforce_max_count
        let _ = queue.enforce_max_count(100)?;

        // Test vacuum
        queue.vacuum()?;

        // Test increment_retry and get_retry_count
        queue.increment_retry(&heartbeat.id)?;
        assert_eq!(queue.get_retry_count(&heartbeat.id)?, 1);

        // Test count
        assert_eq!(queue.count()?, 1);

        Ok(())
    }
}
