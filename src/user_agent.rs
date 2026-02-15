//! User agent generation for Chronova CLI
//!
//! This module generates user agent strings that are compatible with Wakatime's format:
//! `chronova/{version} ({os}-{core}-{platform}) {runtime} {plugin}`

use sysinfo::System;
use std::env;

/// Generates a user agent string compatible with Wakatime's format
///
/// Format: `chronova/{version} ({os}-{core}-{platform}) {runtime} {plugin}`
///
/// # Arguments
/// * `plugin` - Mandatory plugin information (e.g., "vscode/1.106.3 vscode-wakatime/25.5.0")
/// * `plugin_format` - "string/version string-chronova/version"
///
/// # Returns
/// A formatted user agent string
pub fn generate_user_agent(plugin: Option<&str>) -> String {
    let version = env!("CARGO_PKG_VERSION");

    // Get system information
    let os_info = get_os_info();
    let runtime = get_runtime_info();

    // Build plugin part:
    // If plugin provided and contains at least two whitespace-separated parts,
    // use the first two (ide/version and plugin/version). If only one part is present,
    // duplicate it to form two parts. If no plugin is provided, default to
    // duplicating the cli identifier: "chronova-cli/{version} chronova-cli/{version}"
    let plugin_part = match plugin {
        Some(p) => {
            let s = sanitize_plugin_string(p);
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() >= 2 {
                format!("{} {}", parts[0], parts[1])
            } else if parts.len() == 1 {
                format!("{} {}", parts[0], parts[0])
            } else {
                // If plugin string was empty after sanitization, fall back to duplicated CLI token
                // Don't include the 'v' prefix to match Wakatime-style tokens (e.g. "chronova-cli/0.1.0")
                format!("chronova-cli/{} chronova-cli/{}", version, version)
            }
        }
        // When no plugin is provided, produce a two-part duplicated CLI token to match Wakatime-style plugin token
        None => format!("chronova-cli/{} chronova-cli/{}", version, version),
    };

    // Final format matches wakatime style:
    // client/version (os-core-platform) runtime plugin1 plugin2
    format!(
        "chronova/{} ({}-{}-{}) {} {}",
        version,
        os_info.os,
        os_info.core,
        os_info.platform,
        runtime,
        plugin_part
    )
}

/// Sanitizes plugin string by removing surrounding quotes if present
fn sanitize_plugin_string(plugin: &str) -> String {
    // Remove surrounding quotes if present
    if plugin.starts_with('"') && plugin.ends_with('"') && plugin.len() >= 2 {
        plugin[1..plugin.len()-1].to_string()
    } else {
        plugin.to_string()
    }
}

/// Information about the operating system
struct OsInfo {
    os: String,
    core: String,
    platform: String,
}

/// Gets operating system information
fn get_os_info() -> OsInfo {
    let os_name = System::name().unwrap_or_else(|| "unknown".to_string());
    let os_version = System::os_version().unwrap_or_else(|| "unknown".to_string());
    let kernel_version = System::kernel_version().unwrap_or_else(|| "unknown".to_string());

    // For compatibility with Wakatime format, we need to format this appropriately
    let os = format!("{}-{}", os_name.to_lowercase(), kernel_version.to_lowercase());

    // Core and platform info - simplified for now
    let core = os_version.to_lowercase();
    let platform = match env::consts::ARCH {
        "x86_64" => "x86_64".to_string(),
        "aarch64" => "arm64".to_string(),
        arch => arch.to_string(),
    };

    OsInfo {
        os,
        core,
        platform,
    }
}

/// Gets runtime information
fn get_runtime_info() -> String {
    // Get rustc version from environment or use a default
    let rustc_version = option_env!("RUSTC_VERSION").unwrap_or("1.75.0"); // Using a default version
    format!("rustc/{}", rustc_version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_user_agent_without_plugin() {
        let ua = generate_user_agent(None);
        assert!(ua.starts_with("chronova/"));
        // Check that it contains the basic structure with parentheses and runtime info
        assert!(ua.contains("(") && ua.contains(")"));
        assert!(ua.contains("rustc/"));
        assert!(ua.ends_with(&format!("chronova-cli/{} chronova-cli/{}", env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_VERSION"))));
    }

    #[test]
    fn test_generate_user_agent_with_plugin() {
        let ua = generate_user_agent(Some("vscode/1.106.3 vscode-wakatime/25.5.0"));
        assert!(ua.starts_with("chronova/"));
        // Check that it contains the basic structure with parentheses and runtime info
        assert!(ua.contains("(") && ua.contains(")"));
        assert!(ua.contains("rustc/"));
        assert!(ua.ends_with("vscode/1.106.3 vscode-wakatime/25.5.0"));
    }

    #[test]
    fn test_generate_user_agent_with_quoted_plugin() {
        let ua = generate_user_agent(Some("\"vscode/1.106.3 vscode-wakatime/25.5.0\""));
        assert!(ua.starts_with("chronova/"));
        // Check that it contains the basic structure with parentheses and runtime info
        assert!(ua.contains("(") && ua.contains(")"));
        assert!(ua.contains("rustc/"));
        // Should not contain quotes in the final output
        assert!(!ua.contains("\""));
        assert!(ua.ends_with("vscode/1.106.3 vscode-wakatime/25.5.0"));
    }

    #[test]
    fn test_sanitize_plugin_string() {
        // Test with quotes
        assert_eq!(sanitize_plugin_string("\"vscode/1.106.3 vscode-wakatime/25.5.0\""),
                   "vscode/1.106.3 vscode-wakatime/25.5.0");

        // Test without quotes
        assert_eq!(sanitize_plugin_string("vscode/1.106.3 vscode-wakatime/25.5.0"),
                   "vscode/1.106.3 vscode-wakatime/25.5.0");

        // Test with single quote (should not be removed)
        assert_eq!(sanitize_plugin_string("'vscode/1.106.3 vscode-wakatime/25.5.0'"),
                   "'vscode/1.106.3 vscode-wakatime/25.5.0'");

        // Test with only opening quote (should not be removed)
        assert_eq!(sanitize_plugin_string("\"vscode/1.106.3 vscode-wakatime/25.5.0"),
                   "\"vscode/1.106.3 vscode-wakatime/25.5.0");

        // Test with only closing quote (should not be removed)
        assert_eq!(sanitize_plugin_string("vscode/1.106.3 vscode-wakatime/25.5.0\""),
                   "vscode/1.106.3 vscode-wakatime/25.5.0\"");
    }
}
