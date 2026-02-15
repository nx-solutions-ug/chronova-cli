use clap::Parser;

#[path = "../src/cli.rs"]
mod cli;

#[test]
fn test_wakatime_entity_argument() {
    let args = vec!["chronova-cli", "--entity", "/tmp/test.rs"];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(cli.entity, Some("/tmp/test.rs".to_string()));
}

#[test]
fn test_wakatime_key_argument() {
    let args = vec![
        "chronova-cli",
        "--entity",
        "/tmp/test.rs",
        "--key",
        "test_key_123",
    ];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(cli.key, Some("test_key_123".to_string()));
}

#[test]
fn test_wakatime_plugin_argument() {
    let args = vec![
        "chronova-cli",
        "--entity",
        "/tmp/test.rs",
        "--plugin",
        "vscode/1.88.0 vscode-wakatime/24.0.0",
    ];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(
        cli.plugin,
        Some("vscode/1.88.0 vscode-wakatime/24.0.0".to_string())
    );
}

#[test]
fn test_wakatime_time_argument() {
    let args = vec![
        "chronova-cli",
        "--entity",
        "/tmp/test.rs",
        "--time",
        "1234567890.123",
    ];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(cli.time, Some(1234567890.123));
}

#[test]
fn test_wakatime_lineno_argument() {
    let args = vec!["chronova-cli", "--entity", "/tmp/test.rs", "--lineno", "42"];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(cli.lineno, Some(42));
}

#[test]
fn test_wakatime_cursorpos_argument() {
    let args = vec![
        "chronova-cli",
        "--entity",
        "/tmp/test.rs",
        "--cursorpos",
        "123",
    ];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(cli.cursorpos, Some(123));
}

#[test]
fn test_wakatime_lines_argument() {
    let args = vec!["chronova-cli", "--entity", "/tmp/test.rs", "--lines", "100"];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(cli.lines, Some(100));
}

#[test]
fn test_wakatime_category_argument() {
    let args = vec![
        "chronova-cli",
        "--entity",
        "/tmp/test.rs",
        "--category",
        "coding",
    ];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(cli.category, Some("coding".to_string()));
}

#[test]
fn test_wakatime_project_argument() {
    let args = vec![
        "chronova-cli",
        "--entity",
        "/tmp/test.rs",
        "--project",
        "test-project",
    ];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(cli.project, Some("test-project".to_string()));
}

#[test]
fn test_wakatime_language_argument() {
    let args = vec![
        "chronova-cli",
        "--entity",
        "/tmp/test.rs",
        "--language",
        "Rust",
    ];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(cli.language, Some("Rust".to_string()));
}

#[test]
fn test_wakatime_config_argument() {
    let args = vec![
        "chronova-cli",
        "--entity",
        "/tmp/test.rs",
        "--config",
        "/path/to/config.cfg",
    ];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(cli.config, "/path/to/config.cfg".to_string());
}

#[test]
fn test_wakatime_timeout_argument() {
    let args = vec![
        "chronova-cli",
        "--entity",
        "/tmp/test.rs",
        "--timeout",
        "60",
    ];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(cli.timeout, 60);
}

#[test]
fn test_wakatime_verbose_argument() {
    let args = vec!["chronova-cli", "--entity", "/tmp/test.rs", "--verbose"];
    let cli = cli::Cli::parse_from(args);
    assert!(cli.verbose);
}

#[test]
fn test_wakatime_write_argument() {
    let args = vec!["chronova-cli", "--entity", "/tmp/test.rs", "--write"];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(cli.write, Some(true));
}

#[test]
fn test_wakatime_write_argument_with_explicit_true() {
    let args = vec![
        "chronova-cli",
        "--entity",
        "/tmp/test.rs",
        "--write",
        "true",
    ];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(cli.write, Some(true));
}

#[test]
fn test_wakatime_write_argument_with_explicit_false() {
    let args = vec![
        "chronova-cli",
        "--entity",
        "/tmp/test.rs",
        "--write",
        "false",
    ];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(cli.write, Some(false));
}

#[test]
fn test_wakatime_write_argument_with_equals_syntax() {
    let args = vec!["chronova-cli", "--entity", "/tmp/test.rs", "--write=true"];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(cli.write, Some(true));
}

#[test]
fn test_wakatime_entity_type_argument() {
    let args = vec![
        "chronova-cli",
        "--entity",
        "/tmp/test.rs",
        "--entity-type",
        "file",
    ];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(cli.entity_type, "file".to_string());
}

#[test]
fn test_wakatime_today_argument() {
    let args = vec!["chronova-cli", "--today"];
    let cli = cli::Cli::parse_from(args);
    assert!(cli.today);
}

#[test]
fn test_wakatime_alternate_project_argument() {
    let args = vec![
        "chronova-cli",
        "--entity",
        "/tmp/test.rs",
        "--alternate-project",
        "alt-project",
    ];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(cli.alternate_project, Some("alt-project".to_string()));
}

#[test]
fn test_wakatime_api_url_argument() {
    let args = vec![
        "chronova-cli",
        "--entity",
        "/tmp/test.rs",
        "--api-url",
        "https://api.example.com",
    ];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(cli.api_url, Some("https://api.example.com".to_string()));
}

#[test]
fn test_wakatime_hostname_argument() {
    let args = vec![
        "chronova-cli",
        "--entity",
        "/tmp/test.rs",
        "--hostname",
        "test-host",
    ];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(cli.hostname, Some("test-host".to_string()));
}

#[test]
fn test_wakatime_branch_argument() {
    let args = vec![
        "chronova-cli",
        "--entity",
        "/tmp/test.rs",
        "--branch",
        "main",
    ];
    let cli = cli::Cli::parse_from(args);
    assert_eq!(cli.branch, Some("main".to_string()));
}
