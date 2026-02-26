use clap::Parser;

/// A high-performance, drop-in replacement for wakatime-cli
#[derive(Parser, Debug)]
#[command(
    version,
    about,
    long_about = None,
    version = concat!("v", env!("CARGO_PKG_VERSION")),
    disable_version_flag = true
)]
pub struct Cli {
    /// Absolute path to file for the heartbeat. Can also be a url, domain or app when --entity-type is not file. (alias: --file)
    #[arg(long, alias = "file")]
    pub entity: Option<String>,

    /// Your chronova api key; uses api_key from ~/.chronova.cfg by default.
    #[arg(long)]
    pub key: Option<String>,

    /// Optional text editor plugin name and version for User-Agent header.
    #[arg(long)]
    pub plugin: Option<String>,

    /// Optional floating-point unix epoch timestamp. Uses current time by default.
    #[arg(long)]
    pub time: Option<f64>,

    /// Optional line number. This is the current line being edited.
    #[arg(long)]
    pub lineno: Option<i32>,

    /// Optional cursor position in the current file.
    #[arg(long)]
    pub cursorpos: Option<i32>,

    /// Optional lines in the file. Normally, this is detected automatically but can be provided manually for performance, accuracy, or when using --local-file. (alias: --lines-in-file)
    #[arg(long, alias = "lines-in-file")]
    pub lines: Option<i32>,

    /// Category of this heartbeat activity. Can be "coding", "building", "indexing", "debugging", "learning", "meeting", "planning", "researching", "communicating", "supporting", "advising", "running tests", "writing tests", "manual testing", "writing docs", "code reviewing", "browsing", "translating", or "designing". Defaults to "coding".
    #[arg(long)]
    pub category: Option<String>,

    /// Override auto-detected project. Use --alternate-project to supply a fallback project if one can't be auto-detected.
    #[arg(long)]
    pub project: Option<String>,

    /// Optional language name. If valid, takes priority over auto-detected language.
    #[arg(long)]
    pub language: Option<String>,

    /// Optional config file. Defaults to '~/.chronova.cfg'.
    #[arg(long, default_value = "~/.chronova.cfg")]
    pub config: String,

    /// Number of seconds to wait when sending heartbeats to api. Defaults to 30 seconds.
    #[arg(long, default_value = "30")]
    pub timeout: u64,

    /// Turns on debug messages in log file, and sends diagnostics if a crash occurs.
    #[arg(long)]
    pub verbose: bool,

    /// When set, tells api this heartbeat was triggered from writing to a file. Accepts explicit true/false values for compatibility with clients that pass values.
    #[arg(long, value_parser = clap::value_parser!(bool), value_name = "true|false", num_args = 0..=1, default_missing_value = "true")]
    pub write: Option<bool>,

    /// Entity type for this heartbeat. Can be "file", "domain", "url", or "app". Defaults to "file".
    #[arg(long, default_value = "file")]
    pub entity_type: String,

    /// Prints dashboard time for Today, then exits.
    #[arg(long)]
    pub today: bool,

    /// Optional alternate project name. Auto-detected project takes priority.
    #[arg(long)]
    pub alternate_project: Option<String>,

    /// API base url used when sending heartbeats and fetching code stats. Defaults to https://chronova.dev/api/v1.
    #[arg(long)]
    pub api_url: Option<String>,

    /// Optional name of local machine. Defaults to local machine name read from system.
    #[arg(long)]
    pub hostname: Option<String>,

    /// Optional branch name. Auto-detected branch takes priority.
    #[arg(long)]
    pub branch: Option<String>,

    /// Obfuscate branch names. Will not send revision control branch names to api.
    #[arg(long)]
    pub hide_branch_names: Option<String>,

    /// Obfuscate filenames. Will not send file names to api.
    #[arg(long)]
    pub hide_file_names: Option<String>,

    /// Obfuscate project names. When a project folder is detected instead of using the folder name as the project, a .wakatime-project file is created with a random project name.
    #[arg(long)]
    pub hide_project_names: Option<String>,

    /// When set, send the file's path relative to the project folder.
    #[arg(long)]
    pub hide_project_folder: bool,

    /// Filename patterns to exclude from logging. POSIX regex syntax. Can be used more than once.
    #[arg(long)]
    pub exclude: Option<Vec<String>>,

    /// Filename patterns to log. When used in combination with --exclude, files matching include will still be logged. POSIX regex syntax. Can be used more than once.
    #[arg(long)]
    pub include: Option<Vec<String>>,

    /// Disables offline time logging instead of queuing logged time.
    #[arg(long)]
    pub disable_offline: bool,

    /// When set, any activity where the project cannot be detected will be ignored.
    #[arg(long)]
    pub exclude_unknown_project: bool,

    /// Enable detecting language from file contents.
    #[arg(long)]
    pub guess_language: bool,

    /// Optional absolute path to local file for the heartbeat.
    #[arg(long)]
    pub local_file: Option<String>,

    /// Optional log file. Defaults to '~/.chronova.log'.
    #[arg(long)]
    pub log_file: Option<String>,

    /// Disables SSL certificate verification for HTTPS requests. By default, SSL certificates are verified.
    #[arg(long)]
    pub no_ssl_verify: bool,

    /// Override the bundled CA certs file. By default, uses system ca certs.
    #[arg(long)]
    pub ssl_certs_file: Option<String>,

    /// Format output. Can be "text", "json" or "raw-json". Defaults to "text".
    #[arg(long)]
    pub output: Option<String>,

    /// Optional workspace path. Usually used when hiding the project folder, or when a project root folder can't be auto detected.
    #[arg(long)]
    pub project_folder: Option<String>,

    /// Optional proxy configuration. Supports HTTPS SOCKS and NTLM proxies.
    #[arg(long)]
    pub proxy: Option<String>,

    /// When --verbose or debug enabled, also sends diagnostics on any error not just crashes.
    #[arg(long)]
    pub send_diagnostics_on_errors: bool,

    /// When set, collects metrics usage in '~/.wakatime/metrics' folder. Defaults to false.
    #[arg(long)]
    pub metrics: bool,

    /// Amount of offline activity to sync from your local ~/.chronova/queue.db SQLite file to your WakaTime Dashboard before exiting.
    #[arg(long)]
    pub sync_offline_activity: Option<i32>,

    /// Force sync all offline heartbeats regardless of connectivity status.
    #[arg(long)]
    pub force_sync: bool,

    /// Prints the number of heartbeats in the offline db, then exits.
    #[arg(long)]
    pub offline_count: bool,

    /// Reads extra heartbeats from STDIN as a JSON array until EOF.
    #[arg(long)]
    pub extra_heartbeats: bool,

    /// Prints the top developer within a team for the given entity, then exits.
    #[arg(long)]
    pub file_experts: bool,

    /// Prints value for the given config key, then exits.
    #[arg(long)]
    pub config_read: Option<String>,

    /// Optional config section when reading or writing a config key. Defaults to [settings].
    #[arg(long)]
    pub config_section: Option<String>,

    /// Writes value to a config key, then exits. Expects two arguments, key and value.
    #[arg(long, num_args = 2)]
    pub config_write: Option<Vec<String>>,

    /// Optional internal config file. Defaults to '~/.wakatime/wakatime-internal.cfg'.
    #[arg(long)]
    pub internal_config: Option<String>,

    /// If enabled, logs will go to stdout. Will overwrite logfile configs.
    #[arg(long)]
    pub log_to_stdout: bool,

    /// Prints offline heartbeats to stdout.
    #[arg(long)]
    pub print_offline_heartbeats: Option<i32>,

    /// Prints time for the given goal id today, then exits.
    #[arg(long)]
    pub today_goal: Option<String>,

    /// When optionally included with --today, causes output to show total code time today without categories.
    #[arg(long)]
    pub today_hide_categories: bool,

    /// (internal) Prints the wakatime-cli useragent, as it will be sent to the api, then exits.
    #[arg(long)]
    pub user_agent: bool,

    /// (internal) Specify an offline queue file, which will be used instead of the default one.
    #[arg(long)]
    pub offline_queue_file: Option<String>,

    /// (internal) Specify the legacy offline queue file, which will be used instead of the default one.
    #[arg(long)]
    pub offline_queue_file_legacy: Option<String>,

    /// Normally files that don't exist on disk are skipped and not tracked. When this option is present, the main heartbeat file will be tracked even if it doesn't exist.
    #[arg(long)]
    pub is_unsaved_entity: bool,

    /// Optional number of lines added or removed by humans since last heartbeat in the current file.
    #[arg(long, allow_hyphen_values = true)]
    pub human_line_changes: Option<i32>,

    /// Optional number of lines added or removed by AI since last heartbeat in the current file.
    #[arg(long, allow_hyphen_values = true)]
    pub ai_line_changes: Option<i32>,

    /// Disables tracking folders unless they contain a .wakatime-project file. Defaults to false.
    #[arg(long)]
    pub include_only_with_project_file: bool,

    /// Print version information and exit
    #[arg(long)]
    pub version: bool,
}
