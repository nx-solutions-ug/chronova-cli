use anyhow::Result;
use clap::{CommandFactory, Parser};
use std::process;

use chronova_cli::api::ApiClient;
use chronova_cli::cli::Cli;
use chronova_cli::config::Config;
use chronova_cli::heartbeat::{HeartbeatManager, HeartbeatManagerExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let cli = Cli::parse();

    // Handle --version flag (print version and exit)
    if cli.version {
        // Print name and version for wakatime-cli compatibility (e.g., "chronova-cli/v0.1.0")
        println!("chronova-cli v{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // Handle --today flag (fetch and display today's coding activity)
    if cli.today {
        // Check if JSON output is requested - if so, disable stdout logging to avoid corrupting JSON
        let json_output = cli
            .output
            .as_ref()
            .is_some_and(|format| format == "json" || format == "raw-json");

        // Setup logging with appropriate output format handling
        let _guard = if json_output {
            chronova_cli::logger::setup_logging_with_output_format(cli.verbose, true)
                .unwrap_or_else(|e| {
                    eprintln!("Failed to setup logging: {}", e);
                    process::exit(1);
                })
        } else {
            chronova_cli::logger::setup_logging(cli.verbose).unwrap_or_else(|e| {
                eprintln!("Failed to setup logging: {}", e);
                process::exit(1);
            })
        };

        // Load configuration
        let config = Config::load(&cli.config).unwrap_or_else(|e| {
            eprintln!("Failed to load configuration: {}", e);
            process::exit(1);
        });

        // Fetch and display today's activity
        if let Err(e) = fetch_today_activity(&config, &cli).await {
            eprintln!("Error fetching today's activity: {}", e);
            process::exit(1);
        }
        return Ok(());
    }

    // Handle config read/write operations
    if cli.config_read.is_some() || cli.config_write.is_some() {
        if let Err(e) = handle_config_operations(&cli).await {
            eprintln!("Error handling config operation: {}", e);
            process::exit(1);
        }
        return Ok(());
    }

    // Handle offline count operations
    if cli.offline_count {
        // Check if JSON output is requested - if so, disable stdout logging to avoid corrupting JSON
        let json_output = cli
            .output
            .as_ref()
            .is_some_and(|format| format == "json" || format == "raw-json");

        // Setup logging with appropriate output format handling
        let _guard = if json_output {
            chronova_cli::logger::setup_logging_with_output_format(cli.verbose, true)
                .unwrap_or_else(|e| {
                    eprintln!("Failed to setup logging: {}", e);
                    process::exit(1);
                })
        } else {
            chronova_cli::logger::setup_logging(cli.verbose).unwrap_or_else(|e| {
                eprintln!("Failed to setup logging: {}", e);
                process::exit(1);
            })
        };

        // Load configuration
        let config = Config::load(&cli.config).unwrap_or_else(|e| {
            eprintln!("Failed to load configuration: {}", e);
            process::exit(1);
        });

        // Initialize heartbeat manager
        let mut config = config;
        if let Some(api_url) = &cli.api_url {
            config.api_url = Some(api_url.clone());
        }
        let heartbeat_manager = HeartbeatManager::new(config);

        // Get queue statistics
        match heartbeat_manager.get_queue_stats() {
            Ok(stats) => {
                println!("Offline heartbeats queue status:");
                println!("  Total: {}", stats.total);
                println!("  Pending: {}", stats.pending);
                println!("  Syncing: {}", stats.syncing);
                println!("  Synced: {}", stats.synced);
                println!("  Failed: {}", stats.failed);
                println!("  Permanent failures: {}", stats.permanent_failures);
            }
            Err(e) => {
                eprintln!("Error getting offline queue stats: {}", e);
                process::exit(1);
            }
        }
        return Ok(());
    }

    // Handle file experts operations
    if cli.file_experts {
        return Err(anyhow::anyhow!(
            "File experts operation is not yet implemented. \n\
            This feature will be available in a future release."
        ));
    }

    // Handle today goal operations
    if cli.today_goal.is_some() {
        return Err(anyhow::anyhow!(
            "Today goal operation is not yet implemented. \n\
            This feature will be available in a future release."
        ));
    }

    // Handle user agent operations
    if cli.user_agent {
        // This would print the user agent and exit
        let user_agent = chronova_cli::user_agent::generate_user_agent(cli.plugin.as_deref());
        println!("{}", user_agent);
        return Ok(());
    }

    // Handle extra heartbeats from STDIN
    if cli.extra_heartbeats {
        // Check if JSON output is requested - if so, disable stdout logging to avoid corrupting JSON
        let json_output = cli
            .output
            .as_ref()
            .is_some_and(|format| format == "json" || format == "raw-json");

        // Setup logging with appropriate output format handling
        let _guard = if json_output {
            chronova_cli::logger::setup_logging_with_output_format(cli.verbose, true)
                .unwrap_or_else(|e| {
                    eprintln!("Failed to setup logging: {}", e);
                    process::exit(1);
                })
        } else {
            chronova_cli::logger::setup_logging(cli.verbose).unwrap_or_else(|e| {
                eprintln!("Failed to setup logging: {}", e);
                process::exit(1);
            })
        };

        // Load configuration
        let config = Config::load(&cli.config).unwrap_or_else(|e| {
            eprintln!("Failed to load configuration: {}", e);
            process::exit(1);
        });

        // Initialize heartbeat manager
        let mut config = config;
        if let Some(api_url) = &cli.api_url {
            config.api_url = Some(api_url.clone());
        }
        let heartbeat_manager = HeartbeatManager::new(config);

        // Read extra heartbeats from STDIN as JSON array
        if let Err(e) = process_extra_heartbeats(heartbeat_manager).await {
            eprintln!("Error processing extra heartbeats: {}", e);
            process::exit(1);
        }
        return Ok(());
    }

    // Entity is required for actual heartbeat processing (unless syncing offline activity)
    if cli.entity.is_none() && cli.sync_offline_activity.is_none() {
        eprintln!("Error: --entity argument is required");
        eprintln!();
        eprintln!("{}", Cli::command().render_help());
        process::exit(1);
    }

    // Check if JSON output is requested - if so, disable stdout logging to avoid corrupting JSON
    let json_output = cli
        .output
        .as_ref()
        .is_some_and(|format| format == "json" || format == "raw-json");

    // Setup logging with appropriate output format handling
    let _guard = if json_output {
        chronova_cli::logger::setup_logging_with_output_format(cli.verbose, true).unwrap_or_else(
            |e| {
                eprintln!("Failed to setup logging: {}", e);
                process::exit(1);
            },
        )
    } else {
        chronova_cli::logger::setup_logging(cli.verbose).unwrap_or_else(|e| {
            eprintln!("Failed to setup logging: {}", e);
            process::exit(1);
        })
    };

    // Load configuration
    let config = Config::load(&cli.config).unwrap_or_else(|e| {
        eprintln!("Failed to load configuration: {}", e);
        process::exit(1);
    });

    // Handle sync offline activity
    if let Some(count) = cli.sync_offline_activity {
        // Check if JSON output is requested - if so, disable stdout logging to avoid corrupting JSON
        let json_output = cli
            .output
            .as_ref()
            .is_some_and(|format| format == "json" || format == "raw-json");

        // Setup logging with appropriate output format handling
        let _guard = if json_output {
            chronova_cli::logger::setup_logging_with_output_format(cli.verbose, true)
                .unwrap_or_else(|e| {
                    eprintln!("Failed to setup logging: {}", e);
                    process::exit(1);
                })
        } else {
            chronova_cli::logger::setup_logging(cli.verbose).unwrap_or_else(|e| {
                eprintln!("Failed to setup logging: {}", e);
                process::exit(1);
            })
        };

        // Load configuration
        let config = Config::load(&cli.config).unwrap_or_else(|e| {
            eprintln!("Failed to load configuration: {}", e);
            process::exit(1);
        });

        // Initialize heartbeat manager
        let mut config = config;
        if let Some(api_url) = &cli.api_url {
            config.api_url = Some(api_url.clone());
        }
        let heartbeat_manager = HeartbeatManager::new(config);

        // Perform manual sync
        println!("Syncing offline heartbeats...");
        let force = cli.force_sync;
        match heartbeat_manager.manual_sync().await {
            Ok(result) => {
                println!("Sync completed:");
                println!("  Heartbeats synced: {}", result.synced_count);
                println!("  Heartbeats failed: {}", result.failed_count);
                println!("  Total processed: {}", result.total_count);
                if force {
                    println!("  Forced sync: true");
                }
            }
            Err(e) => {
                eprintln!("Error syncing offline heartbeats: {}", e);
                process::exit(1);
            }
        }
        return Ok(());
    }

    // Initialize heartbeat manager
    let mut config = config;
    if let Some(api_url) = &cli.api_url {
        config.api_url = Some(api_url.clone());
    }
    let heartbeat_manager = HeartbeatManager::new(config);

    // Process the heartbeat
    if let Err(e) = heartbeat_manager.process(cli).await {
        eprintln!("Error processing heartbeat: {}", e);
        process::exit(1);
    }

    Ok(())
}

async fn fetch_today_activity(config: &Config, cli: &Cli) -> Result<(), anyhow::Error> {
    let api_key = config.api_key.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "API key not found in configuration. Please set api_key in your .chronova.cfg file."
        )
    })?;

    let base_url = config.get_api_url();
    let api_client = ApiClient::new(base_url);
    let auth_client = api_client.with_api_key(api_key.clone());

    // Fetch today's statusbar data using the correct endpoint
    let statusbar_data = auth_client.get_today_statusbar().await?;

    // Handle output format based on --output flag
    if let Some(output_format) = &cli.output {
        match output_format.as_str() {
            "json" | "raw-json" => {
                // Return JSON format expected by VSCode WakaTime extension
                // When output is JSON, we MUST only output the JSON and nothing else
                // to avoid breaking VSCode extension parsing
                let json_output = serde_json::json!({
                    "text": chronova_cli::api::format_today_output(&statusbar_data, cli.today_hide_categories),
                    "has_team_features": statusbar_data.has_team_features.unwrap_or(false)
                });
                // Use print! instead of println! to avoid adding extra newline for JSON output
                print!("{}", serde_json::to_string(&json_output)?);
            }
            "text" | _ => {
                // Default text output
                let output = chronova_cli::api::format_today_output(
                    &statusbar_data,
                    cli.today_hide_categories,
                );
                println!("{}", output);
            }
        }
    } else {
        // Default text output when no --output flag is provided
        let output =
            chronova_cli::api::format_today_output(&statusbar_data, cli.today_hide_categories);
        println!("{}", output);
    }

    Ok(())
}

/// Handle config read/write operations
async fn handle_config_operations(cli: &Cli) -> Result<(), anyhow::Error> {
    let config_path = chronova_cli::config::Config::resolve_config_path(&cli.config)?;
    let section = cli.config_section.as_deref().unwrap_or("settings");

    // Handle config read
    if let Some(key) = &cli.config_read {
        let mut ini = configparser::ini::Ini::new();
        ini.set_multiline(true);

        // Load existing config if it exists
        if config_path.exists() {
            ini.load(&config_path).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to load config from {}: {}",
                    config_path.display(),
                    e
                )
            })?;
        }

        // Get the value from the specified section
        let value = ini.get(section, key);

        // Output the value (or empty string if not found)
        println!("{}", value.unwrap_or_default());
        return Ok(());
    }

    // Handle config write
    if let Some(args) = &cli.config_write {
        if args.len() != 2 {
            return Err(anyhow::anyhow!(
                "--config-write requires exactly 2 arguments: key and value"
            ));
        }

        let key = &args[0];
        let value = &args[1];

        let mut ini = configparser::ini::Ini::new();
        ini.set_multiline(true);

        // Load existing config if it exists
        if config_path.exists() {
            ini.load(&config_path).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to load config from {}: {}",
                    config_path.display(),
                    e
                )
            })?;
        }

        // Set the value in the specified section
        ini.set(section, key, Some(value.clone()));

        // Save the config back to file
        ini.write(&config_path).map_err(|e| {
            anyhow::anyhow!("Failed to write config to {}: {}", config_path.display(), e)
        })?;

        return Ok(());
    }

    Ok(())
}

/// Process extra heartbeats from STDIN as a JSON array
async fn process_extra_heartbeats(
    heartbeat_manager: HeartbeatManager,
) -> Result<(), anyhow::Error> {
    use std::io::{self, Read};
    use uuid::Uuid;

    // Read all input from STDIN
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    // Debug: Log the raw input to understand the JSON format
    tracing::debug!(
        "Raw extra heartbeats input (first 500 chars): {}",
        if input.len() > 500 {
            &input[..500]
        } else {
            &input
        }
    );

    // Try to parse as JSON value first to inspect structure
    match serde_json::from_str::<serde_json::Value>(&input) {
        Ok(value) => {
            tracing::debug!("Parsed JSON value: {}", value);

            // Check if it's an array
            if let serde_json::Value::Array(arr) = &value {
                tracing::debug!("JSON is an array with {} elements", arr.len());

                // Log first element structure for debugging
                if let Some(first) = arr.first() {
                    tracing::debug!("First element structure: {}", first);
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to parse as JSON value: {}", e);
        }
    }

    // Parse the JSON array of heartbeats, but handle missing id field
    // External heartbeats (from WakaTime extension) may not include an id field
    let heartbeats_result: Result<Vec<chronova_cli::heartbeat::Heartbeat>, _> =
        serde_json::from_str(&input);

    let heartbeats = match heartbeats_result {
        Ok(heartbeats) => heartbeats,
        Err(e) => {
            // If parsing fails due to missing id field, try parsing as a different structure
            // that doesn't require id, then add the id field manually
            tracing::warn!("Failed to parse heartbeats with strict validation: {}", e);
            tracing::info!("Attempting to parse with relaxed validation for external heartbeats");

            // Define a relaxed heartbeat structure that doesn't require id or type
            // This matches the WakaTime ExtraHeartbeat format where most fields are optional
            #[derive(Debug, serde::Deserialize)]
            struct RelaxedHeartbeat {
                pub entity: String,
                #[serde(rename = "type", default = "default_entity_type")]
                pub entity_type: String,
                pub time: f64,
                pub project: Option<String>,
                pub branch: Option<String>,
                pub language: Option<String>,
                #[serde(default)]
                pub is_write: bool,
                pub lines: Option<i32>,
                pub lineno: Option<i32>,
                pub cursorpos: Option<i32>,
                pub user_agent: Option<String>,
                pub category: Option<String>,
                pub machine: Option<String>,
                #[serde(default)]
                pub dependencies: Vec<String>,
            }

            fn default_entity_type() -> String {
                "file".to_string()
            }

            // Parse as relaxed heartbeats
            let relaxed_heartbeats: Vec<RelaxedHeartbeat> =
                serde_json::from_str(&input).map_err(|e| {
                    tracing::error!("Failed to parse even with relaxed validation: {}", e);
                    anyhow::anyhow!("Failed to parse extra heartbeats: {}", e)
                })?;

            // Convert to proper heartbeats by adding id field
            let mut heartbeats = Vec::new();
            for relaxed in relaxed_heartbeats {
                let heartbeat = chronova_cli::heartbeat::Heartbeat {
                    id: Uuid::new_v4().to_string(), // Generate UUID for missing id
                    entity: relaxed.entity,
                    entity_type: relaxed.entity_type,
                    time: relaxed.time,
                    project: relaxed.project,
                    branch: relaxed.branch,
                    language: relaxed.language,
                    is_write: relaxed.is_write,
                    lines: relaxed.lines,
                    lineno: relaxed.lineno,
                    cursorpos: relaxed.cursorpos,
                    user_agent: Some(chronova_cli::user_agent::generate_user_agent(
                        relaxed.user_agent.as_deref(),
                    )),
                    category: relaxed.category,
                    machine: relaxed.machine,
                    editor: None,
                    operating_system: None,
                    commit_hash: None,
                    commit_author: None,
                    commit_message: None,
                    repository_url: None,
                    dependencies: relaxed.dependencies,
                };
                heartbeats.push(heartbeat);
            }

            tracing::info!(
                "Successfully parsed {} external heartbeats with generated IDs",
                heartbeats.len()
            );
            heartbeats
        }
    };

    tracing::info!(
        "Processing {} extra heartbeats from STDIN",
        heartbeats.len()
    );

    for heartbeat in &heartbeats {
        heartbeat_manager.add_heartbeat_to_queue(heartbeat.clone())?;
    }

    tracing::info!("Successfully queued {} extra heartbeats", heartbeats.len());

    Ok(())
}
