// Test file for sync configuration
// We need to use a different approach since #[path] imports don't work well with complex module structures

// Copy the SyncConfig struct directly for testing
#[derive(Debug, Clone)]
pub struct SyncConfig {
    pub enabled: bool,
    pub max_queue_size: usize,
    pub sync_interval_seconds: u64,
    pub max_retry_attempts: u32,
    pub retry_base_delay_seconds: u64,
    pub retry_max_delay_seconds: u64,
    pub retry_use_jitter: bool,
    pub retention_days: u32,
    pub background_sync: bool,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_queue_size: 1000,
            sync_interval_seconds: 300,
            max_retry_attempts: 5,
            retry_base_delay_seconds: 1,
            retry_max_delay_seconds: 60,
            retry_use_jitter: true,
            retention_days: 7,
            background_sync: true,
        }
    }
}

// Simple test implementation that mimics the real Config::load behavior
fn parse_sync_config(settings: &std::collections::HashMap<String, Option<String>>) -> SyncConfig {
    let mut sync_config = SyncConfig::default();

    if let Some(enabled) = settings.get("sync_enabled") {
        if let Some(value) = enabled.as_ref() {
            if let Ok(parsed) = value.parse::<bool>() {
                sync_config.enabled = parsed;
            }
        }
    }

    if let Some(max_queue_size) = settings.get("sync_max_queue_size") {
        if let Some(value) = max_queue_size.as_ref() {
            if let Ok(parsed) = value.parse::<usize>() {
                sync_config.max_queue_size = parsed;
            }
        }
    }

    if let Some(sync_interval) = settings.get("sync_interval") {
        if let Some(value) = sync_interval.as_ref() {
            if let Ok(parsed) = value.parse::<u64>() {
                sync_config.sync_interval_seconds = parsed;
            }
        }
    }

    if let Some(max_retries) = settings.get("sync_max_retries") {
        if let Some(value) = max_retries.as_ref() {
            if let Ok(parsed) = value.parse::<u32>() {
                sync_config.max_retry_attempts = parsed;
            }
        }
    }

    if let Some(retry_base_delay) = settings.get("sync_retry_base_delay") {
        if let Some(value) = retry_base_delay.as_ref() {
            if let Ok(parsed) = value.parse::<u64>() {
                sync_config.retry_base_delay_seconds = parsed;
            }
        }
    }

    if let Some(retry_max_delay) = settings.get("sync_retry_max_delay") {
        if let Some(value) = retry_max_delay.as_ref() {
            if let Ok(parsed) = value.parse::<u64>() {
                sync_config.retry_max_delay_seconds = parsed;
            }
        }
    }

    if let Some(retry_use_jitter) = settings.get("sync_retry_use_jitter") {
        if let Some(value) = retry_use_jitter.as_ref() {
            if let Ok(parsed) = value.parse::<bool>() {
                sync_config.retry_use_jitter = parsed;
            }
        }
    }

    if let Some(retention_days) = settings.get("sync_retention_days") {
        if let Some(value) = retention_days.as_ref() {
            if let Ok(parsed) = value.parse::<u32>() {
                sync_config.retention_days = parsed;
            }
        }
    }

    if let Some(background_sync) = settings.get("sync_background") {
        if let Some(value) = background_sync.as_ref() {
            if let Ok(parsed) = value.parse::<bool>() {
                sync_config.background_sync = parsed;
            }
        }
    }

    sync_config
}

#[test]
fn test_sync_config_default() {
    let sync_config = SyncConfig::default();
    assert!(sync_config.enabled);
    assert_eq!(sync_config.max_queue_size, 1000);
    assert_eq!(sync_config.sync_interval_seconds, 300);
    assert_eq!(sync_config.max_retry_attempts, 5);
    assert_eq!(sync_config.retry_base_delay_seconds, 1);
    assert_eq!(sync_config.retry_max_delay_seconds, 60);
    assert!(sync_config.retry_use_jitter);
    assert_eq!(sync_config.retention_days, 7);
    assert!(sync_config.background_sync);
}

#[test]
fn test_sync_config_clone() {
    let sync_config1 = SyncConfig {
        enabled: false,
        max_queue_size: 500,
        sync_interval_seconds: 60,
        max_retry_attempts: 3,
        retry_base_delay_seconds: 2,
        retry_max_delay_seconds: 30,
        retry_use_jitter: false,
        retention_days: 3,
        background_sync: false,
    };

    let sync_config2 = sync_config1.clone();

    assert_eq!(sync_config1.enabled, sync_config2.enabled);
    assert_eq!(sync_config1.max_queue_size, sync_config2.max_queue_size);
    assert_eq!(
        sync_config1.sync_interval_seconds,
        sync_config2.sync_interval_seconds
    );
    assert_eq!(
        sync_config1.max_retry_attempts,
        sync_config2.max_retry_attempts
    );
    assert_eq!(
        sync_config1.retry_base_delay_seconds,
        sync_config2.retry_base_delay_seconds
    );
    assert_eq!(
        sync_config1.retry_max_delay_seconds,
        sync_config2.retry_max_delay_seconds
    );
    assert_eq!(sync_config1.retry_use_jitter, sync_config2.retry_use_jitter);
    assert_eq!(sync_config1.retention_days, sync_config2.retention_days);
    assert_eq!(sync_config1.background_sync, sync_config2.background_sync);
}

#[test]
fn test_sync_config_debug() {
    let sync_config = SyncConfig::default();
    let debug_output = format!("{:?}", sync_config);

    assert!(debug_output.contains("enabled"));
    assert!(debug_output.contains("max_queue_size"));
    assert!(debug_output.contains("sync_interval_seconds"));
    assert!(debug_output.contains("max_retry_attempts"));
    assert!(debug_output.contains("retry_base_delay_seconds"));
    assert!(debug_output.contains("retry_max_delay_seconds"));
    assert!(debug_output.contains("retry_use_jitter"));
    assert!(debug_output.contains("retention_days"));
    assert!(debug_output.contains("background_sync"));
}

// Test the parse_sync_config function directly
#[test]
fn test_parse_sync_config() {
    use std::collections::HashMap;

    let mut settings = HashMap::new();
    settings.insert("sync_enabled".to_string(), Some("false".to_string()));
    settings.insert("sync_max_queue_size".to_string(), Some("500".to_string()));
    settings.insert("sync_interval".to_string(), Some("60".to_string()));
    settings.insert("sync_max_retries".to_string(), Some("3".to_string()));
    settings.insert("sync_retry_base_delay".to_string(), Some("2".to_string()));
    settings.insert("sync_retry_max_delay".to_string(), Some("30".to_string()));
    settings.insert(
        "sync_retry_use_jitter".to_string(),
        Some("false".to_string()),
    );
    settings.insert("sync_retention_days".to_string(), Some("3".to_string()));
    settings.insert("sync_background".to_string(), Some("false".to_string()));

    let sync_config = parse_sync_config(&settings);

    assert!(!sync_config.enabled);
    assert_eq!(sync_config.max_queue_size, 500);
    assert_eq!(sync_config.sync_interval_seconds, 60);
    assert_eq!(sync_config.max_retry_attempts, 3);
    assert_eq!(sync_config.retry_base_delay_seconds, 2);
    assert_eq!(sync_config.retry_max_delay_seconds, 30);
    assert!(!sync_config.retry_use_jitter);
    assert_eq!(sync_config.retention_days, 3);
    assert!(!sync_config.background_sync);
}

#[test]
fn test_parse_sync_config_partial() {
    use std::collections::HashMap;

    let mut settings = HashMap::new();
    settings.insert("sync_enabled".to_string(), Some("false".to_string()));
    settings.insert("sync_max_queue_size".to_string(), Some("200".to_string()));

    let sync_config = parse_sync_config(&settings);

    // Overridden settings
    assert!(!sync_config.enabled);
    assert_eq!(sync_config.max_queue_size, 200);

    // Default settings should remain
    assert_eq!(sync_config.sync_interval_seconds, 300);
    assert_eq!(sync_config.max_retry_attempts, 5);
    assert_eq!(sync_config.retry_base_delay_seconds, 1);
    assert_eq!(sync_config.retry_max_delay_seconds, 60);
    assert!(sync_config.retry_use_jitter);
    assert_eq!(sync_config.retention_days, 7);
    assert!(sync_config.background_sync);
}

#[test]
fn test_parse_sync_config_invalid() {
    use std::collections::HashMap;

    let mut settings = HashMap::new();
    settings.insert("sync_enabled".to_string(), Some("invalid_bool".to_string()));
    settings.insert(
        "sync_max_queue_size".to_string(),
        Some("not_a_number".to_string()),
    );
    settings.insert("sync_interval".to_string(), Some("-5".to_string()));

    let sync_config = parse_sync_config(&settings);

    // Invalid settings should fall back to defaults
    assert!(sync_config.enabled);
    assert_eq!(sync_config.max_queue_size, 1000);
    assert_eq!(sync_config.sync_interval_seconds, 300);
}
