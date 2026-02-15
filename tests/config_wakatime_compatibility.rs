use std::fs;
use tempfile::NamedTempFile;

// Simplified Config struct for testing
#[derive(Debug, Clone)]
pub struct Config {
    pub api_key: Option<String>,
    pub api_url: Option<String>,
    pub debug: bool,
    pub hide_file_names: bool,
    pub hide_project_names: bool,
    pub hide_branch_names: bool,
    pub hide_project_folder: bool,
    pub exclude_unknown_project: bool,
    pub disable_offline: bool,
    pub guess_language: bool,
    pub hostname: Option<String>,
    pub log_file: Option<String>,
    pub no_ssl_verify: bool,
    pub ssl_certs_file: Option<String>,
    pub metrics: bool,
    pub include_only_with_project_file: bool,
    pub ignore_patterns: Vec<String>,
    pub include_patterns: Vec<String>,
}

impl Config {
    pub fn load(config_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        use configparser::ini::Ini;
        
        let mut ini = Ini::new();
        ini.set_multiline(true);

        let config_map = ini.load(config_path)?;
        let settings = config_map.get("settings").cloned().unwrap_or_default();

        Ok(Config {
            api_key: settings.get("api_key").and_then(|v| v.clone()),
            api_url: settings.get("api_url").and_then(|v| v.clone()),
            debug: settings.get("debug")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .unwrap_or(false),
            hide_file_names: settings.get("hide_file_names")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .unwrap_or(false),
            hide_project_names: settings.get("hide_project_names")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .unwrap_or(false),
            hide_branch_names: settings.get("hide_branch_names")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .unwrap_or(false),
            hide_project_folder: settings.get("hide_project_folder")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .unwrap_or(false),
            exclude_unknown_project: settings.get("exclude_unknown_project")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .unwrap_or(false),
            disable_offline: settings.get("offline")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .map(|v: bool| !v) // offline = true means disable_offline = false
                .unwrap_or(false),
            guess_language: settings.get("guess_language")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .unwrap_or(false),
            hostname: settings.get("hostname").and_then(|v| v.clone()),
            log_file: settings.get("log_file").and_then(|v| v.clone()),
            no_ssl_verify: settings.get("no_ssl_verify")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .unwrap_or(false),
            ssl_certs_file: settings.get("ssl_certs_file").and_then(|v| v.clone()),
            metrics: settings.get("metrics")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .unwrap_or(false),
            include_only_with_project_file: settings.get("include_only_with_project_file")
                .and_then(|s| s.as_ref().and_then(|v| v.parse().ok()))
                .unwrap_or(false),
            ignore_patterns: settings.get("exclude")
                .and_then(|s| s.as_ref())
                .map(|s| s.split('\n')
                    .map(|line| line.trim().to_string())
                    .filter(|line| !line.is_empty())
                    .collect())
                .unwrap_or_default(),
            include_patterns: settings.get("include")
                .and_then(|s| s.as_ref())
                .map(|s| s.split('\n')
                    .map(|line| line.trim().to_string())
                    .filter(|line| !line.is_empty())
                    .collect())
                .unwrap_or_default(),
        })
    }
}

#[test]
fn test_wakatime_config_parsing() {
    // Create a temporary config file with WakaTime format
    let config_file = NamedTempFile::new().unwrap();
    let config_content = r#"
[settings]
api_key = test_key_123
api_url = https://test.example.com/api
debug = true
hide_file_names = true
hide_project_names = false
hide_branch_names = true
hide_project_folder = false
exclude_unknown_project = true
offline = false
guess_language = true
hostname = test-host
log_file = /tmp/wakatime.log
no_ssl_verify = false
ssl_certs_file = /etc/ssl/certs.pem
metrics = true
include_only_with_project_file = false
exclude =
    *.tmp
    *.log
    COMMIT_EDITMSG$
include =
    *.rs
    *.js
    *.py
"#;
    fs::write(config_file.path(), config_content).unwrap();

    let config = Config::load(config_file.path().to_str().unwrap()).unwrap();
    
    assert_eq!(config.api_key, Some("test_key_123".to_string()));
    assert_eq!(config.api_url, Some("https://test.example.com/api".to_string()));
    assert!(config.debug);
    assert!(config.hide_file_names);
    assert!(!config.hide_project_names);
    assert!(config.hide_branch_names);
    assert!(!config.hide_project_folder);
    assert!(config.exclude_unknown_project);
    assert!(config.disable_offline); // offline = false means disable_offline = true
    assert!(config.guess_language);
    assert_eq!(config.hostname, Some("test-host".to_string()));
    assert_eq!(config.log_file, Some("/tmp/wakatime.log".to_string()));
    assert!(!config.no_ssl_verify);
    assert_eq!(config.ssl_certs_file, Some("/etc/ssl/certs.pem".to_string()));
    assert!(config.metrics);
    assert!(!config.include_only_with_project_file);
    
    // Check exclude patterns
    assert!(config.ignore_patterns.contains(&"*.tmp".to_string()));
    assert!(config.ignore_patterns.contains(&"*.log".to_string()));
    assert!(config.ignore_patterns.contains(&"COMMIT_EDITMSG$".to_string()));
    
    // Check include patterns
    assert!(config.include_patterns.contains(&"*.rs".to_string()));
    assert!(config.include_patterns.contains(&"*.js".to_string()));
    assert!(config.include_patterns.contains(&"*.py".to_string()));
}

#[test]
fn test_wakatime_config_defaults() {
    // Test with minimal config
    let config_file = NamedTempFile::new().unwrap();
    let config_content = r#"
[settings]
api_key = test_key_123
"#;
    fs::write(config_file.path(), config_content).unwrap();

    let config = Config::load(config_file.path().to_str().unwrap()).unwrap();
    
    assert_eq!(config.api_key, Some("test_key_123".to_string()));
    // Check that defaults are applied
    assert!(!config.debug);
    assert!(!config.hide_file_names);
    assert!(!config.disable_offline); // offline defaults to true, so disable_offline = false
}

#[test]
fn test_wakatime_config_offline_setting() {
    // Test offline setting (inverted logic)
    let config_file = NamedTempFile::new().unwrap();
    let config_content = r#"
[settings]
api_key = test_key_123
offline = true
"#;
    fs::write(config_file.path(), config_content).unwrap();

    let config = Config::load(config_file.path().to_str().unwrap()).unwrap();
    assert!(!config.disable_offline); // offline = true means disable_offline = false
    
    // Test with offline = false
    let config_file2 = NamedTempFile::new().unwrap();
    let config_content2 = r#"
[settings]
api_key = test_key_123
offline = false
"#;
    fs::write(config_file2.path(), config_content2).unwrap();

    let config2 = Config::load(config_file2.path().to_str().unwrap()).unwrap();
    assert!(config2.disable_offline); // offline = false means disable_offline = true
}