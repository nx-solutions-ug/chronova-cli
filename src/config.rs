use configparser::ini::Ini;
use dirs::home_dir;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::sync::SyncConfig;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to parse config file: {0}")]
    ParseError(String),
    #[error("Config file not found: {0}")]
    NotFound(String),
    #[error("Invalid config path: {0}")]
    InvalidPath(String),
}

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

impl Config {
    pub fn load(config_path: &str) -> Result<Self, ConfigError> {
        let config_path = Self::resolve_config_path(config_path)?;

        if !config_path.exists() {
            return Ok(Self::default());
        }

        let mut ini = Ini::new();
        ini.set_multiline(true);

        let config_map = ini.load(&config_path).map_err(|e| {
            ConfigError::ParseError(format!(
                "Failed to load config from {}: {}",
                config_path.display(),
                e
            ))
        })?;

        let settings = config_map.get("settings").cloned().unwrap_or_default();

        Ok(Config {
            api_key: settings.get("api_key").and_then(|v| v.clone()),
            api_url: settings.get("api_url").and_then(|v| v.clone()),
            debug: settings
                .get("debug")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .unwrap_or(false),
            proxy: settings.get("proxy").and_then(|v| v.clone()),
            hide_file_names: settings
                .get("hide_file_names")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .unwrap_or(false),
            hide_project_names: settings
                .get("hide_project_names")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .unwrap_or(false),
            hide_branch_names: settings
                .get("hide_branch_names")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .unwrap_or(false),
            hide_project_folder: settings
                .get("hide_project_folder")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .unwrap_or(false),
            exclude_unknown_project: settings
                .get("exclude_unknown_project")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .unwrap_or(false),
            disable_offline: settings
                .get("offline")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .map(|v: bool| !v) // offline = true means disable_offline = false
                .unwrap_or(false),
            guess_language: settings
                .get("guess_language")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .unwrap_or(false),
            hostname: settings.get("hostname").and_then(|v| v.clone()),
            log_file: settings.get("log_file").and_then(|v| v.clone()),
            no_ssl_verify: settings
                .get("no_ssl_verify")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .unwrap_or(false),
            ssl_certs_file: settings.get("ssl_certs_file").and_then(|v| v.clone()),
            metrics: settings
                .get("metrics")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .unwrap_or(false),
            include_only_with_project_file: settings
                .get("include_only_with_project_file")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .unwrap_or(false),
            sync_config: Self::parse_sync_config(&settings),
            ignore_patterns: settings
                .get("exclude")
                .and_then(|s| s.as_ref())
                .map(|s| {
                    s.split('\n')
                        .map(|line| line.trim().to_string())
                        .filter(|line| !line.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            include_patterns: settings
                .get("include")
                .and_then(|s| s.as_ref())
                .map(|s| {
                    s.split('\n')
                        .map(|line| line.trim().to_string())
                        .filter(|line| !line.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    pub fn resolve_config_path(config_path: &str) -> Result<PathBuf, ConfigError> {
        let path = Path::new(config_path);

        if path.is_absolute() {
            return Ok(path.to_path_buf());
        }

        // Handle ~ expansion
        if config_path.starts_with("~/") {
            if let Some(mut home) = home_dir() {
                home.push(&config_path[2..]);
                return Ok(home);
            }
        }

        // Handle relative paths by resolving them relative to the current directory
        if !config_path.contains('/') && !config_path.contains('\\') {
            // Only simple filenames without path separators, check if it's one of our special cases
            if config_path == "~/.chronova.cfg" || config_path == ".chronova.cfg" {
                if let Some(mut home) = home_dir() {
                    home.push(".chronova.cfg");
                    return Ok(home);
                }
            }
        }

        // For all other cases, treat as relative to current directory
        if let Ok(current_dir) = std::env::current_dir() {
            let resolved_path = current_dir.join(config_path);
            return Ok(resolved_path);
        }

        Err(ConfigError::InvalidPath(config_path.to_string()))
    }

    pub fn get_api_key(&self, cli_key: Option<&String>) -> Option<String> {
        cli_key.cloned().or_else(|| self.api_key.clone())
    }

    pub fn get_api_url(&self) -> String {
        self.api_url
            .clone()
            .unwrap_or_else(|| "https://chronova.dev/api/v1".to_string())
    }

    fn parse_sync_config(
        settings: &std::collections::HashMap<String, Option<String>>,
    ) -> SyncConfig {
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_key: None,
            api_url: Some("https://chronova.dev/api/v1".to_string()),
            debug: false,
            proxy: None,
            ignore_patterns: vec![
                "COMMIT_EDITMSG$".to_string(),
                "PULLREQ_EDITMSG$".to_string(),
                "MERGE_MSG$".to_string(),
                "TAG_EDITMSG$".to_string(),
            ],
            include_patterns: vec![],
            hide_file_names: false,
            hide_project_names: false,
            hide_branch_names: false,
            hide_project_folder: false,
            exclude_unknown_project: false,
            disable_offline: false,
            guess_language: false,
            hostname: None,
            log_file: None,
            no_ssl_verify: false,
            ssl_certs_file: None,
            metrics: false,
            include_only_with_project_file: false,
            sync_config: SyncConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(!config.debug);
        assert!(!config.ignore_patterns.is_empty());
    }

    #[test]
    fn test_load_config_from_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let config_content = r#"
[settings]
api_key = test_key_123
api_url = https://chronova.local:3000/api/v1
debug = true
hide_file_names = true
exclude =
    *.tmp
    *.log
"#;
        fs::write(temp_file.path(), config_content).unwrap();

        let config = Config::load(temp_file.path().to_str().unwrap()).unwrap();
        assert_eq!(config.api_key, Some("test_key_123".to_string()));
        assert_eq!(
            config.api_url,
            Some("https://chronova.local:3000/api/v1".to_string())
        );
        assert!(config.debug);
        assert!(config.hide_file_names);
        assert!(config.ignore_patterns.contains(&"*.tmp".to_string()));
        assert!(config.ignore_patterns.contains(&"*.log".to_string()));
    }

    #[test]
    fn test_get_api_key_precedence() {
        let config = Config {
            api_key: Some("config_key".to_string()),
            ..Default::default()
        };

        // CLI key takes precedence
        assert_eq!(
            config.get_api_key(Some(&"cli_key".to_string())),
            Some("cli_key".to_string())
        );

        // Fall back to config key
        assert_eq!(config.get_api_key(None), Some("config_key".to_string()));

        // No key available
        let empty_config = Config::default();
        assert_eq!(empty_config.get_api_key(None), None);
    }
}
