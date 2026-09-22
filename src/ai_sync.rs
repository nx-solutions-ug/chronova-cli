//! Claude Code AI activity sync (`--sync-ai-activity`).
//!
//! The `claude-code-wakatime` plugin (>= 4.1.0) no longer parses Claude Code
//! transcripts itself. It only rate-limits to 60s and shells out with
//! `--sync-ai-activity --plugin <ua> --project-folder <cwd>`, expecting the CLI
//! to do the work. This module is the CLI half of that contract, ported from
//! upstream `wakatime-cli`'s `pkg/ai/claude.go` and `pkg/ai/ai.go`.
//!
//! Flow: take an exclusive lock, load the `ai_logs_last_parsed_at` cutoff, walk
//! `~/.claude/projects/**/*.jsonl` for transcripts modified at or after it,
//! turn each `toolUseResult` into heartbeats, enqueue them, flush the queue,
//! then advance the cutoff to the newest heartbeat actually produced.
//!
//! The plugin logs any output this process writes to stdout/stderr as an error,
//! so a successful run must stay completely silent. All diagnostics go to the
//! log file via `tracing`.

use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::cli::Cli;
use crate::collector::{DataCollector, GitInfo};
use crate::config::Config;
use crate::heartbeat::{AiTelemetry, Heartbeat, HeartbeatManager, HeartbeatManagerExt};
use crate::queue::{Queue, QueueOps};
use crate::user_agent::generate_user_agent;

/// Only the trailing window of an oversized transcript is parsed, mirroring
/// upstream's `maxTranscriptLineSize`. The first (probably truncated) line of
/// that window is discarded.
const MAX_TRANSCRIPT_TAIL_BYTES: u64 = 10 * 1024 * 1024;

/// Cutoff used the first time this machine ever runs an AI sync.
///
/// Upstream backfills from a hardcoded 2025-02-24, but that is unsafe here: the
/// Chronova API mints its own heartbeat ids and performs no de-duplication, so
/// replaying transcripts that an older plugin build already reported would
/// double-count every one of them. A short lookback is the safe default; to
/// backfill a known gap, seed `ai_logs_last_parsed_at` explicitly.
const DEFAULT_LOOKBACK: Duration = Duration::from_secs(120);

/// A lock file older than this is assumed to belong to a crashed run.
const LOCK_STALE_AFTER: Duration = Duration::from_secs(300);

/// Category the Chronova dashboard's `classifyHeartbeats` reads as AI-assisted.
const AI_CATEGORY: &str = "ai coding";

/// Value for `ai_agent`.
///
/// Spelled to match the server's `EDITOR_NAME_MAP`, which keys on `claude code`
/// (and `claudecode`) but not `claude-code`; an unmapped value is rendered
/// verbatim, which would fragment agent grouping in the dashboard.
const AI_AGENT: &str = "Claude Code";

/// Entry point for `--sync-ai-activity`.
///
/// Returns the number of heartbeats enqueued. Errors are returned rather than
/// printed; the caller decides how loudly to fail.
pub async fn sync_ai_activity(cli: &Cli, config: Config) -> Result<usize> {
    // Serialize concurrent runs. Several Claude Code sessions each fire the
    // plugin on an independent 60s timer, so overlap is the normal case, not an
    // edge case. A busy lock is success: the other run covers the same work.
    let _lock = match SyncLock::acquire()? {
        Some(lock) => lock,
        None => {
            tracing::debug!("ai sync already running elsewhere, skipping");
            return Ok(0);
        }
    };

    let cutoff = load_last_parsed_at().unwrap_or_else(|| {
        let fallback = Utc::now() - chrono::Duration::from_std(DEFAULT_LOOKBACK).unwrap();
        tracing::info!(
            "no ai_logs_last_parsed_at recorded, starting from {}",
            fallback.to_rfc3339()
        );
        fallback
    });

    let transcripts = transcript_paths(cutoff)?;
    if transcripts.is_empty() {
        tracing::debug!(
            "no claude transcripts modified since {}",
            cutoff.to_rfc3339()
        );
        return Ok(0);
    }

    tracing::info!(
        "parsing {} claude transcript(s) modified since {}",
        transcripts.len(),
        cutoff.to_rfc3339()
    );

    let collector = DataCollector::new();
    let mut ctx = BuildContext::new(
        &collector,
        &config,
        cli.plugin.as_deref(),
        cli.project_folder.as_deref(),
    );

    let mut heartbeats: Vec<Heartbeat> = Vec::new();
    for path in &transcripts {
        match parse_transcript(path, cutoff) {
            Ok(parsed) => {
                for record in parsed {
                    heartbeats.extend(ctx.build(record).await);
                }
            }
            // One unreadable transcript must not sink the whole run.
            Err(e) => tracing::warn!("failed parsing transcript {}: {}", path.display(), e),
        }
    }

    if heartbeats.is_empty() {
        tracing::debug!("no ai heartbeats produced");
        return Ok(0);
    }

    let newest = heartbeats
        .iter()
        .map(|h| h.time)
        .fold(f64::NEG_INFINITY, f64::max);

    let queued_ids: Vec<String> = heartbeats.iter().map(|h| h.id.clone()).collect();
    let count = queued_ids.len();
    tracing::info!("enqueuing {} ai heartbeat(s)", count);

    // A `Queue` handle is needed here to enqueue the batch before the manager
    // exists, so this opens one directly and passes it to
    // `HeartbeatManager::new_with_queue` rather than `HeartbeatManager::new()`.
    let queue = Queue::new().context("failed to open offline queue")?;
    queue
        .add_batch(heartbeats)
        .context("failed to enqueue ai heartbeats")?;

    let manager = HeartbeatManager::new_with_queue(config, queue);
    let outcome = manager.manual_sync().await;

    // On a total failure, take the batch back out and leave the cutoff alone so
    // the next run re-derives it from the transcripts. Transcripts are the
    // real durable store: the queue only survives for `sync_retention_days`
    // (`config.rs:275`, default 7) before retention cleanup prunes it — from
    // `process_queue` on the sync/flush path, or from `Queue`'s own `Drop`
    // as a fallback for callers that never reach `process_queue` at all
    // (see AGENTS.md's Landmines section). Re-parsing is cheap.
    // A partial success keeps its rows and advances, so retries cannot
    // duplicate the heartbeats that did land.
    match outcome {
        Ok(result) if result.synced_count == 0 && result.failed_count > 0 => {
            tracing::warn!(
                "ai sync sent nothing ({} failed); rolling back and holding the cutoff",
                result.failed_count
            );
            rollback(&queued_ids);
            return Ok(0);
        }
        Ok(result) => {
            tracing::info!(
                "ai sync flushed queue: {} synced, {} failed",
                result.synced_count,
                result.failed_count
            );
        }
        Err(e) => {
            tracing::warn!(
                "ai sync could not flush queue: {}; rolling back and holding the cutoff",
                e
            );
            rollback(&queued_ids);
            return Ok(0);
        }
    }

    if let Some(ts) = timestamp_from_unix(newest) {
        if let Err(e) = store_last_parsed_at(ts) {
            tracing::warn!("failed to update ai_logs_last_parsed_at: {}", e);
        }
    }

    Ok(count)
}

/// Removes heartbeats this run enqueued, so a failed send leaves no trace.
fn rollback(ids: &[String]) {
    let queue = match Queue::new() {
        Ok(queue) => queue,
        Err(e) => {
            tracing::warn!("could not reopen queue to roll back ai heartbeats: {}", e);
            return;
        }
    };

    let mut failed = 0usize;
    for id in ids {
        if queue.remove(id).is_err() {
            failed += 1;
        }
    }

    if failed > 0 {
        tracing::warn!(
            "failed to roll back {} of {} ai heartbeat(s)",
            failed,
            ids.len()
        );
    }
}

/// Exclusive, best-effort lock guarding a sync run. Released on drop.
struct SyncLock {
    path: PathBuf,
}

impl SyncLock {
    /// Returns `Ok(None)` when another live run already holds the lock.
    fn acquire() -> Result<Option<Self>> {
        let path = lock_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }

        for _ in 0..2 {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    // Purely informational; nothing reads it back.
                    let _ = write!(file, "{}", std::process::id());
                    return Ok(Some(Self { path }));
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if is_stale(&path) {
                        tracing::warn!("removing stale ai sync lock at {}", path.display());
                        // If removal races with the owner, the retry simply
                        // observes AlreadyExists again and we give up.
                        let _ = fs::remove_file(&path);
                        continue;
                    }
                    return Ok(None);
                }
                Err(e) => {
                    return Err(e).context(format!("failed to create lock {}", path.display()))
                }
            }
        }

        Ok(None)
    }
}

impl Drop for SyncLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn is_stale(path: &Path) -> bool {
    let Ok(modified) = fs::metadata(path).and_then(|m| m.modified()) else {
        return false;
    };
    SystemTime::now()
        .duration_since(modified)
        .map(|age| age > LOCK_STALE_AFTER)
        .unwrap_or(false)
}

fn lock_path() -> Result<PathBuf> {
    let mut path = dirs::home_dir().context("could not determine home directory")?;
    path.push(".chronova");
    path.push("ai-sync.lock");
    Ok(path)
}

fn internal_config_path() -> Result<PathBuf> {
    let mut path = dirs::home_dir().context("could not determine home directory")?;
    path.push(".chronova-internal.cfg");
    Ok(path)
}

/// Reads `[internal] ai_logs_last_parsed_at`, ignoring a value in the future.
fn load_last_parsed_at() -> Option<DateTime<Utc>> {
    let path = internal_config_path().ok()?;
    let mut ini = configparser::ini::Ini::new();
    ini.load(&path).ok()?;

    let raw = ini.get("internal", "ai_logs_last_parsed_at")?;
    match DateTime::parse_from_rfc3339(raw.trim()) {
        Ok(parsed) => {
            let parsed = parsed.with_timezone(&Utc);
            // A clock that jumped backwards would otherwise strand the cutoff
            // in the future and stop all tracking.
            Some(parsed.min(Utc::now()))
        }
        Err(e) => {
            tracing::warn!("failed to parse ai_logs_last_parsed_at {:?}: {}", raw, e);
            None
        }
    }
}

/// Writes the cutoff back, preserving any other keys already in the file.
fn store_last_parsed_at(ts: DateTime<Utc>) -> Result<()> {
    let path = internal_config_path()?;
    let mut ini = configparser::ini::Ini::new();
    // Missing file is fine; we are about to create it.
    let _ = ini.load(&path);
    ini.set("internal", "ai_logs_last_parsed_at", Some(ts.to_rfc3339()));
    ini.write(&path)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn timestamp_from_unix(seconds: f64) -> Option<DateTime<Utc>> {
    if !seconds.is_finite() || seconds <= 0.0 {
        return None;
    }
    let nanos = (seconds * 1_000_000_000.0).round() as i64;
    Some(Utc.timestamp_nanos(nanos))
}

/// Every `*.jsonl` under `~/.claude/projects` modified at or after `cutoff`.
///
/// Note this deliberately ignores `--project-folder`: upstream treats that flag
/// as a project *override* for heartbeats that lack a `cwd`, not as a filter, so
/// one session's sync still covers transcripts from every project.
fn transcript_paths(cutoff: DateTime<Utc>) -> Result<Vec<PathBuf>> {
    let mut root = dirs::home_dir().context("could not determine home directory")?;
    root.push(".claude");
    root.push("projects");

    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let cutoff_system = SystemTime::UNIX_EPOCH
        .checked_add(Duration::from_nanos(
            cutoff.timestamp_nanos_opt().unwrap_or(0).max(0) as u64,
        ))
        .unwrap_or(SystemTime::UNIX_EPOCH);

    let mut found = Vec::new();
    let mut stack = vec![root];

    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::debug!("skipping unreadable directory {}: {}", dir.display(), e);
                continue;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            if file_type.is_dir() {
                stack.push(path);
                continue;
            }

            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }

            let modified = entry.metadata().and_then(|m| m.modified());
            // `>=`, not `>`: upstream's `timestampAtOrAfterCutoff` is inclusive,
            // and sub-second writes would otherwise be dropped.
            if matches!(modified, Ok(m) if m >= cutoff_system) {
                found.push(path);
            }
        }
    }

    found.sort();
    Ok(found)
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ContentValue {
    Text(String),
    Items(Vec<serde_json::Value>),
}

impl ContentValue {
    /// Total lines across the value, matching upstream `contentValue.lineChanges`.
    fn line_changes(&self) -> i32 {
        match self {
            Self::Text(s) => count_string_lines(s),
            Self::Items(items) => items
                .iter()
                .map(|item| match item {
                    serde_json::Value::String(s) => count_string_lines(s),
                    serde_json::Value::Object(map) => map
                        .get("text")
                        .and_then(|t| t.as_str())
                        .map(count_string_lines)
                        .unwrap_or(0),
                    _ => 0,
                })
                .sum(),
        }
    }
}

fn opt_line_changes(value: &Option<ContentValue>) -> i32 {
    value.as_ref().map(ContentValue::line_changes).unwrap_or(0)
}

#[derive(Debug, Deserialize)]
struct StructuredPatch {
    #[serde(default, rename = "newLines")]
    new_lines: i32,
    #[serde(default, rename = "oldLines")]
    old_lines: i32,
}

fn lenient_paths<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .filter_map(|item| match item {
                serde_json::Value::String(path) => Some(path),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    })
}

/// One file a shell command changed, as Claude Code reports it in
/// `bashEditDiff`. The hunks carry the same shape as `structuredPatch`.
#[derive(Debug, Deserialize)]
struct BashEditFile {
    #[serde(default, rename = "filePath")]
    file_path: Option<String>,
    #[serde(default)]
    hunks: Vec<StructuredPatch>,
}

#[derive(Debug, Deserialize)]
struct BashEditDiff {
    /// Every path the command changed. `files` is only the subset Claude Code
    /// attached a diff to, and it is capped — `moreFiles` counts the rest.
    ///
    /// Read leniently on purpose: a shape this does not recognise yields an
    /// empty list and the caller falls back to `files`, rather than failing the
    /// whole result and losing the heartbeat with it.
    #[serde(default, rename = "changedFiles", deserialize_with = "lenient_paths")]
    changed_files: Vec<String>,
    #[serde(default)]
    files: Vec<BashEditFile>,
}

#[derive(Debug, Deserialize)]
struct ToolUseResultFile {
    #[serde(default, rename = "filePath")]
    file_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ToolUseResult {
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    file: Option<ToolUseResultFile>,
    #[serde(default)]
    content: Option<ContentValue>,
    #[serde(default)]
    stdout: Option<ContentValue>,
    #[serde(default)]
    stderr: Option<ContentValue>,
    #[serde(default)]
    result: Option<ContentValue>,
    #[serde(default, rename = "codeText")]
    code_text: Option<ContentValue>,
    #[serde(default, rename = "filePath")]
    file_path: Option<String>,
    #[serde(default, rename = "originalFile")]
    original_file: Option<String>,
    #[serde(default, rename = "oldString")]
    old_string: Option<String>,
    #[serde(default, rename = "newString")]
    new_string: Option<String>,
    #[serde(default, rename = "structuredPatch")]
    structured_patch: Option<Vec<StructuredPatch>>,
    #[serde(default, rename = "bashEditDiff")]
    bash_edit_diff: Option<BashEditDiff>,
    /// Remaining keys, used only to recognise agent-bookkeeping results.
    #[serde(flatten)]
    raw: HashMap<String, serde_json::Value>,
}

impl ToolUseResult {
    fn path(&self) -> Option<&str> {
        self.file_path
            .as_deref()
            .or_else(|| self.file.as_ref()?.file_path.as_deref())
    }

    /// Net lines changed. May be negative when an edit removes more than it adds.
    fn line_changes(&self) -> i32 {
        if let Some(patches) = &self.structured_patch {
            if !patches.is_empty() {
                return patches.iter().map(|p| p.new_lines - p.old_lines).sum();
            }
        }

        if let Some(new_string) = &self.new_string {
            let new_lines = count_string_lines(new_string);
            return match &self.old_string {
                Some(old) => new_lines - count_string_lines(old),
                None => new_lines,
            };
        }

        // A non-empty `originalFile` means the tool read/verified the file
        // rather than writing it. An empty one comes from a create.
        if self.original_file.as_deref().is_some_and(|f| !f.is_empty()) {
            return 0;
        }

        // Top-level content covers writes and creates. `file.content` is a Read
        // result and deliberately does not count.
        opt_line_changes(&self.content)
    }

    fn is_write(&self) -> bool {
        if self
            .structured_patch
            .as_ref()
            .is_some_and(|p| !p.is_empty())
        {
            return true;
        }

        if matches!(self.kind.as_deref(), Some("create" | "update" | "delete")) {
            return true;
        }

        if self.new_string.is_some() || self.old_string.is_some() {
            return true;
        }

        if self.original_file.as_deref().is_some_and(|f| !f.is_empty()) {
            return false;
        }

        opt_line_changes(&self.content) != 0
    }

    /// True when the result is purely agent bookkeeping (task/search plumbing)
    /// with no code payload, so it must not inflate app line counts.
    fn is_agentic_only(&self) -> bool {
        if self.kind.is_some()
            || self.file.is_some()
            || self.file_path.is_some()
            || self.original_file.is_some()
            || self.old_string.is_some()
            || self.new_string.is_some()
            || self.structured_patch.is_some()
            || self.stdout.is_some()
            || self.stderr.is_some()
            || self.result.is_some()
            || self.code_text.is_some()
        {
            return false;
        }

        const AGENTIC_KEYS: [&str; 11] = [
            "agentId",
            "agentType",
            "matches",
            "query",
            "results",
            "statusChange",
            "task",
            "taskId",
            "total_deferred_tools",
            "updatedFields",
            "verificationNudgeNeeded",
        ];

        AGENTIC_KEYS.iter().any(|k| self.raw.contains_key(*k))
    }

    fn app_line_changes(&self) -> i32 {
        opt_line_changes(&self.content)
            + opt_line_changes(&self.stdout)
            + opt_line_changes(&self.stderr)
            + opt_line_changes(&self.result)
            + opt_line_changes(&self.code_text)
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ToolUseResultValue {
    Text(String),
    Items(Vec<serde_json::Value>),
    Object(Box<ToolUseResult>),
}

#[derive(Debug, Default, Deserialize)]
struct Usage {
    #[serde(default)]
    input_tokens: Option<i64>,
    #[serde(default)]
    cache_creation_input_tokens: Option<i64>,
    #[serde(default)]
    cache_read_input_tokens: Option<i64>,
    #[serde(default)]
    output_tokens: Option<i64>,
    #[serde(default)]
    total_tokens: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
    Other(serde::de::IgnoredAny),
}

impl MessageContent {
    fn text_blocks(&self) -> Vec<&str> {
        match self {
            Self::Text(s) => vec![s.as_str()],
            Self::Blocks(blocks) => blocks
                .iter()
                .filter(|b| b.kind == "text")
                .map(|b| b.text.as_str())
                .collect(),
            Self::Other(_) => Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Message {
    #[serde(default)]
    id: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    effort: String,
    #[serde(default)]
    usage: Option<Usage>,
    #[serde(default)]
    content: Option<MessageContent>,
}

#[derive(Debug, Deserialize)]
struct LogLine {
    #[serde(default)]
    timestamp: Option<DateTime<Utc>>,
    #[serde(default, rename = "toolUseResult")]
    tool_use_result: Option<ToolUseResultValue>,
    #[serde(default)]
    usage: Option<Usage>,
    #[serde(default)]
    message: Option<Message>,
    #[serde(default, rename = "isSidechain")]
    is_sidechain: Option<bool>,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
}

/// One heartbeat's worth of extracted facts, before project/language lookup.
#[derive(Debug)]
struct Record {
    entity: String,
    entity_type: &'static str,
    time: f64,
    is_write: bool,
    /// Net lines changed; negative for net deletions.
    line_changes: i32,
    prompt_tokens: i64,
    completion_tokens: i64,
    action: String,
    /// Directory used for project attribution, when known.
    project_dir: Option<String>,
    model: String,
}

/// Running token totals, so each heartbeat reports only its own delta.
#[derive(Debug, Default, Clone, Copy)]
struct Tokens {
    current_input: i64,
    current_cached: i64,
    current_output: i64,
    last_input: i64,
    last_cached: i64,
    last_output: i64,
}

impl Tokens {
    /// Reported tokens: fresh input and output only.
    ///
    /// Cache reads are deliberately excluded. They replay context that was
    /// already counted when it was first written, so folding them in inflates
    /// the total by orders of magnitude on a long session (a 1M-context run
    /// re-reads most of its context every single turn). The Chronova schema has
    /// no separate field for cached input, so the honest choice is to omit it.
    fn delta(&self) -> (i64, i64) {
        let prompt = (self.current_input - self.last_input).max(0);
        let completion = (self.current_output - self.last_output).max(0);
        (prompt, completion)
    }

    /// Cache reads still count as activity even though they are not reported.
    fn has_delta(&self) -> bool {
        let (prompt, completion) = self.delta();
        let cached = (self.current_cached - self.last_cached).max(0);
        prompt > 0 || completion > 0 || cached > 0
    }

    fn advance(&mut self) {
        self.last_input = self.current_input;
        self.last_cached = self.current_cached;
        self.last_output = self.current_output;
    }
}

/// Tracks the last message's contribution so a streamed message logged several
/// times replaces its earlier contribution instead of accumulating.
#[derive(Debug, Default)]
struct LastMessage {
    id: String,
    input: i64,
    cached: i64,
    output: i64,
}

fn parse_transcript(path: &Path, cutoff: DateTime<Utc>) -> Result<Vec<Record>> {
    let file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let size = file.metadata()?.len();

    let mut reader = BufReader::new(file);
    let mut skip_first_line = false;

    // Huge transcripts are read from the tail only; the first line of that
    // window is almost certainly cut in half, so drop it.
    if size > MAX_TRANSCRIPT_TAIL_BYTES {
        reader.seek(SeekFrom::Start(size - MAX_TRANSCRIPT_TAIL_BYTES))?;
        skip_first_line = true;
    }

    let session_fallback = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let session_entity = app_heartbeat_entity("Claude", &session_fallback);

    let mut records = Vec::new();
    let mut tokens = Tokens::default();
    let mut last_msg = LastMessage::default();
    let mut model = String::new();
    let mut effort = String::new();
    let mut cwd = String::new();
    let mut cwd_from_transcript = false;

    for line in reader.lines() {
        let line = match line {
            Ok(line) => line,
            // A partially-flushed tail line is expected on a live transcript.
            Err(e) => {
                tracing::debug!("stopping read of {}: {}", path.display(), e);
                break;
            }
        };

        if skip_first_line {
            skip_first_line = false;
            continue;
        }

        if line.trim().is_empty() {
            continue;
        }

        let log_line: LogLine = match serde_json::from_str(&line) {
            Ok(parsed) => parsed,
            Err(e) => {
                tracing::debug!("skipping unparseable line in {}: {}", path.display(), e);
                continue;
            }
        };

        if let Some(msg) = &log_line.message {
            if !msg.model.is_empty() {
                model = msg.model.clone();
            }
            if !msg.effort.is_empty() {
                effort = msg.effort.clone();
            }
        }
        if let Some(line_cwd) = project_path(&log_line, !cwd_from_transcript) {
            cwd = line_cwd;
            cwd_from_transcript = log_line.cwd.as_deref().is_some_and(|c| !c.is_empty());
        }

        accumulate_tokens(&log_line, &mut tokens, &mut last_msg);

        // Lines older than the cutoff still move the token baseline, otherwise
        // the first in-window heartbeat would claim the whole backlog.
        let Some(timestamp) = log_line.timestamp.filter(|t| *t >= cutoff) else {
            tokens.advance();
            continue;
        };

        let time = timestamp.timestamp_nanos_opt().unwrap_or(0) as f64 / 1_000_000_000.0;
        let mut assign_tokens = tokens.has_delta();
        let mut produced = 0usize;

        // App heartbeat: the session itself (prompts, agent chatter, tokens).
        if let Some(record) = app_record(
            &log_line,
            &session_entity,
            time,
            &model_token(&model, &effort),
            &cwd,
            if assign_tokens { Some(tokens) } else { None },
        ) {
            records.push(record);
            assign_tokens = false;
            produced += 1;
        }

        // File heartbeat: the edited file.
        if let Some(record) = file_record(
            &log_line,
            time,
            &model_token(&model, &effort),
            &cwd,
            cwd_from_transcript,
            if assign_tokens { Some(tokens) } else { None },
        ) {
            records.push(record);
            assign_tokens = false;
            produced += 1;
        }

        // A shell command edits files too — `sed -i`, a formatter, codegen, a
        // checkout. Claude Code reports those in `bashEditDiff` with the same
        // hunks an Edit carries, and they are the larger share of file activity
        // in an agentic session. Without this they were the session's only
        // trace, so the work showed up as time with no file and no language.
        for record in bash_edit_records(
            &log_line,
            time,
            &model_token(&model, &effort),
            &cwd,
            cwd_from_transcript,
            if assign_tokens { Some(tokens) } else { None },
        ) {
            records.push(record);
            produced += 1;
        }

        if produced > 0 || should_advance_for_noop(&log_line) {
            tokens.advance();
        }
    }

    Ok(records)
}

/// Tool results that produce no heartbeat but still consumed tokens.
/// `model` alone, or `model-effort`, matching upstream's model user-agent token.
fn model_token(model: &str, effort: &str) -> String {
    match (model.is_empty(), effort.is_empty()) {
        (true, _) => String::new(),
        (false, true) => model.to_string(),
        (false, false) => format!("{}-{}", model, effort),
    }
}

fn should_advance_for_noop(log_line: &LogLine) -> bool {
    match &log_line.tool_use_result {
        Some(ToolUseResultValue::Object(result)) => result.is_agentic_only(),
        Some(_) => false,
        None => false,
    }
}

/// One record per file a shell command changed.
///
/// `file_record` handles the Edit and Write tools, which report exactly one
/// path per result. A single shell command can touch many files at once, so
/// this yields a record for each and lets the token delta land on the first.
fn bash_edit_records(
    log_line: &LogLine,
    time: f64,
    model: &str,
    cwd: &str,
    cwd_from_transcript: bool,
    tokens: Option<Tokens>,
) -> Vec<Record> {
    let Some(ToolUseResultValue::Object(result)) = log_line.tool_use_result.as_ref() else {
        return Vec::new();
    };
    let Some(diff) = result.bash_edit_diff.as_ref() else {
        return Vec::new();
    };

    let mut records = Vec::new();
    let mut remaining = tokens;

    // `changedFiles` is the authoritative list; `files` only carries the diffs
    // for as many as Claude Code chose to attach. Falling back to `files` keeps
    // a result that names no `changedFiles` from being dropped entirely.
    let mut paths: Vec<&str> = diff.changed_files.iter().map(String::as_str).collect();
    if paths.is_empty() {
        paths = diff
            .files
            .iter()
            .filter_map(|f| f.file_path.as_deref())
            .collect();
    }

    for path in paths {
        if path.is_empty() || is_task_output_path(path) {
            continue;
        }

        let line_changes: i32 = diff
            .files
            .iter()
            .filter(|f| f.file_path.as_deref() == Some(path))
            .flat_map(|f| f.hunks.iter())
            .map(|h| h.new_lines - h.old_lines)
            .sum();
        let (prompt_tokens, completion_tokens) =
            remaining.take().map(|t| t.delta()).unwrap_or((0, 0));

        records.push(Record {
            entity: path.to_string(),
            entity_type: "file",
            time,
            is_write: true,
            line_changes,
            prompt_tokens,
            completion_tokens,
            action: "edit".to_string(),
            project_dir: (cwd_from_transcript && !cwd.is_empty()).then(|| cwd.to_string()),
            model: model.to_string(),
        });
    }

    records
}

fn app_record(
    log_line: &LogLine,
    session_entity: &str,
    time: f64,
    model: &str,
    cwd: &str,
    tokens: Option<Tokens>,
) -> Option<Record> {
    let line_changes = app_line_changes(log_line.tool_use_result.as_ref());
    let prompt_length = prompt_length(log_line);

    if line_changes == 0 && prompt_length == 0 {
        return None;
    }

    let (prompt_tokens, completion_tokens) = tokens.map(|t| t.delta()).unwrap_or((0, 0));

    Some(Record {
        entity: session_entity.to_string(),
        entity_type: "app",
        time,
        is_write: false,
        line_changes: 0,
        prompt_tokens,
        completion_tokens,
        action: if prompt_length > 0 {
            "prompt"
        } else {
            "tool_use"
        }
        .to_string(),
        project_dir: (!cwd.is_empty()).then(|| cwd.to_string()),
        model: model.to_string(),
    })
}

fn file_record(
    log_line: &LogLine,
    time: f64,
    model: &str,
    cwd: &str,
    cwd_from_transcript: bool,
    tokens: Option<Tokens>,
) -> Option<Record> {
    let ToolUseResultValue::Object(result) = log_line.tool_use_result.as_ref()? else {
        return None;
    };

    let file_path = result.path()?.to_string();
    if file_path.is_empty() || is_task_output_path(&file_path) {
        return None;
    }

    let line_changes = result.line_changes();
    let is_write = result.is_write();

    // Nothing changed and nothing written: a plain read, not AI activity.
    if line_changes == 0 && !is_write {
        return None;
    }

    let (prompt_tokens, completion_tokens) = tokens.map(|t| t.delta()).unwrap_or((0, 0));

    let action = match result.kind.as_deref() {
        Some(kind @ ("create" | "update" | "delete")) => kind.to_string(),
        _ => "edit".to_string(),
    };

    Some(Record {
        entity: file_path,
        entity_type: "file",
        time,
        is_write,
        line_changes,
        prompt_tokens,
        completion_tokens,
        action,
        // Only a cwd read from the transcript is trustworthy enough to override
        // the project detected from the file's own path.
        project_dir: (cwd_from_transcript && !cwd.is_empty()).then(|| cwd.to_string()),
        model: model.to_string(),
    })
}

fn app_line_changes(result: Option<&ToolUseResultValue>) -> i32 {
    let Some(result) = result else {
        return 0;
    };

    match result {
        ToolUseResultValue::Text(s) if !s.trim().is_empty() => count_string_lines(s),
        ToolUseResultValue::Text(_) => 0,
        ToolUseResultValue::Items(items) => items
            .iter()
            .map(|item| match item {
                serde_json::Value::String(s) => count_string_lines(s),
                serde_json::Value::Object(map) => map
                    .get("text")
                    .and_then(|t| t.as_str())
                    .map(count_string_lines)
                    .unwrap_or(0),
                _ => 0,
            })
            .sum(),
        ToolUseResultValue::Object(obj) => {
            if obj.is_agentic_only() {
                return 0;
            }
            if obj.path().is_some_and(|p| !p.is_empty()) {
                return 0;
            }
            obj.app_line_changes()
        }
    }
}

fn accumulate_tokens(log_line: &LogLine, tokens: &mut Tokens, last_msg: &mut LastMessage) {
    if let Some(message) = &log_line.message {
        if let Some(usage) = &message.usage {
            let input =
                usage.input_tokens.unwrap_or(0) + usage.cache_creation_input_tokens.unwrap_or(0);
            let cached = usage.cache_read_input_tokens.unwrap_or(0);
            let output = usage.output_tokens.or(usage.total_tokens).unwrap_or(0);

            if !message.id.is_empty() && message.id == last_msg.id {
                // Streaming update for a message we already counted.
                tokens.current_input += input - last_msg.input;
                tokens.current_cached += cached - last_msg.cached;
                tokens.current_output += output - last_msg.output;
            } else {
                tokens.current_input += input;
                tokens.current_cached += cached;
                tokens.current_output += output;
            }

            last_msg.id = message.id.clone();
            last_msg.input = input;
            last_msg.cached = cached;
            last_msg.output = output;
            return;
        }
    }

    let Some(usage) = &log_line.usage else {
        return;
    };

    if usage.input_tokens.is_some() || usage.cache_creation_input_tokens.is_some() {
        tokens.current_input =
            usage.input_tokens.unwrap_or(0) + usage.cache_creation_input_tokens.unwrap_or(0);
    }
    if let Some(cached) = usage.cache_read_input_tokens {
        tokens.current_cached = cached;
    }
    if let Some(output) = usage.output_tokens.or(usage.total_tokens) {
        tokens.current_output = output;
    }
}

/// Length of the user's actual prompt, excluding wrapper tags such as
/// `<system-reminder>` and `<ide_context>` blocks.
fn prompt_length(log_line: &LogLine) -> usize {
    if log_line.is_sidechain.unwrap_or(false) {
        return 0;
    }
    if log_line.kind.as_deref() != Some("user") {
        return 0;
    }
    let Some(message) = &log_line.message else {
        return 0;
    };
    if !message.role.eq_ignore_ascii_case("user") {
        return 0;
    }
    let Some(content) = &message.content else {
        return 0;
    };

    content
        .text_blocks()
        .iter()
        .map(|t| prompt_text_length(t))
        .sum()
}

/// Strips leading XML-ish tag blocks; text that is entirely tags counts as 0.
fn prompt_text_length(text: &str) -> usize {
    let mut trimmed = text.trim();
    if trimmed.is_empty() {
        return 0;
    }
    if !trimmed.starts_with('<') {
        return trimmed.chars().count();
    }

    while trimmed.starts_with('<') {
        let Some(close_open) = trimmed.find('>') else {
            return 0;
        };
        if close_open < 2 {
            return 0;
        }

        let tag = trimmed[1..close_open].trim();
        if tag.is_empty() || tag.starts_with('/') {
            return 0;
        }

        let tag_name = tag.split_whitespace().next().unwrap_or("");
        let tag_name = tag_name.trim_end_matches('/');
        if tag_name.is_empty() {
            return 0;
        }

        // Self-closing: skip just the tag and keep going.
        if tag.ends_with('/') {
            trimmed = trimmed[close_open + 1..].trim();
            continue;
        }

        let close_tag = format!("</{}>", tag_name);
        let Some(idx) = trimmed.find(&close_tag) else {
            return 0;
        };
        trimmed = trimmed[idx + close_tag.len()..].trim();
    }

    trimmed.chars().count()
}

/// `cwd` when the line carries one, else the edited file's directory.
fn project_path(log_line: &LogLine, fallback_to_file_path: bool) -> Option<String> {
    if let Some(cwd) = log_line.cwd.as_deref().filter(|c| !c.is_empty()) {
        return Some(cwd.to_string());
    }
    if !fallback_to_file_path {
        return None;
    }

    let ToolUseResultValue::Object(result) = log_line.tool_use_result.as_ref()? else {
        return None;
    };
    let path = result.path()?;
    if path.is_empty() || is_task_output_path(path) {
        return None;
    }

    Path::new(path)
        .parent()
        .and_then(|p| p.to_str())
        .map(|s| s.to_string())
}

/// Sub-agent task artifacts under `/tmp/claude-<uid>/.../tasks/*.output`.
fn is_task_output_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let Some(rest) = normalized.split("/claude-").nth(1) else {
        // Also handle a path that begins with `claude-<uid>/`.
        return normalized
            .strip_prefix("claude-")
            .is_some_and(task_output_suffix);
    };
    task_output_suffix(rest)
}

fn task_output_suffix(rest: &str) -> bool {
    let Some((uid, tail)) = rest.split_once('/') else {
        return false;
    };
    if uid.is_empty() || !uid.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    // Needs at least one intermediate segment, then `tasks/<name>.output`.
    let segments: Vec<&str> = tail.split('/').collect();
    if segments.len() < 3 {
        return false;
    }
    let Some(last) = segments.last() else {
        return false;
    };
    segments[segments.len() - 2] == "tasks" && last.ends_with(".output")
}

fn count_string_lines(content: &str) -> i32 {
    // Upstream counts a non-empty payload as at least one line.
    1 + content.matches('\n').count() as i32
}

/// `Claude <transcript-stem>`, matching upstream `appHeartbeatEntity`.
fn app_heartbeat_entity(parser_name: &str, raw_entity: &str) -> String {
    let entity = raw_entity.trim();
    if entity.is_empty() {
        return parser_name.to_string();
    }
    let base = Path::new(entity)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if base.is_empty() || base.eq_ignore_ascii_case(parser_name) {
        return parser_name.to_string();
    }
    format!("{} {}", parser_name, base)
}

/// Turns `Record`s into `Heartbeat`s, caching per-directory project lookups
/// because a single sync commonly yields hundreds of records in a few projects.
struct BuildContext<'a> {
    collector: &'a DataCollector,
    config: &'a Config,
    plugin: Option<&'a str>,
    project_folder: Option<&'a str>,
    machine: Option<String>,
    project_cache: HashMap<String, Option<String>>,
    language_cache: HashMap<String, Option<String>>,
    git_cache: HashMap<String, Option<GitInfo>>,
}

impl<'a> BuildContext<'a> {
    fn new(
        collector: &'a DataCollector,
        config: &'a Config,
        plugin: Option<&'a str>,
        project_folder: Option<&'a str>,
    ) -> Self {
        Self {
            collector,
            config,
            plugin,
            project_folder,
            machine: Some(gethostname::gethostname().to_string_lossy().into_owned()),
            project_cache: HashMap::new(),
            language_cache: HashMap::new(),
            git_cache: HashMap::new(),
        }
    }

    async fn build(&mut self, record: Record) -> Vec<Heartbeat> {
        // Project priority: the line's own cwd, then the file's own path, then
        // the plugin's `--project-folder` as a last resort.
        let mut project = None;
        if let Some(dir) = &record.project_dir {
            project = self.project_for(dir).await;
        }
        if project.is_none() && record.entity_type == "file" {
            project = self.project_for(&record.entity).await;
        }
        if project.is_none() {
            if let Some(folder) = self.project_folder {
                project = self.project_for(folder).await;
            }
        }

        let language = if record.entity_type == "file" {
            self.language_for(&record.entity).await
        } else {
            None
        };

        // A file names its own repository, and a worktree is only visible from
        // the file's own path. An `app` entity is the session id, not a path,
        // so it falls back to the cwd the line already attributes its project
        // from.
        let git = if self.config.disable_git_info {
            None
        } else {
            let path = if record.entity_type == "file" {
                Some(record.entity.as_str())
            } else {
                record.project_dir.as_deref()
            };
            match path {
                Some(path) => self.git_for(path).await,
                None => None,
            }
        };

        let hidden = |hide: bool, value: Option<String>| if hide { None } else { value };
        let branch = hidden(
            self.config.hide_branch_names,
            git.as_ref().and_then(|g| g.branch.clone()),
        );
        let commit_hash = hidden(
            self.config.hide_commit_hash,
            git.as_ref().and_then(|g| g.commit_hash.clone()),
        );
        let commit_author = hidden(
            self.config.hide_commit_author,
            git.as_ref().and_then(|g| g.commit_author.clone()),
        );
        let commit_message = hidden(
            self.config.hide_commit_message,
            git.as_ref().and_then(|g| g.commit_message.clone()),
        );
        let repository_url = hidden(
            self.config.hide_repository_url,
            git.as_ref().and_then(|g| g.repository_url.clone()),
        );

        // Every line count the API accepts must be non-negative, so a net
        // deletion reports its magnitude as "suggested" and zero as "accepted".
        let (suggested, accepted) = if record.line_changes == 0 {
            (None, None)
        } else {
            (
                Some(record.line_changes.abs()),
                Some(record.line_changes.max(0)),
            )
        };

        let ai = AiTelemetry {
            ai_agent: Some(AI_AGENT.to_string()),
            ai_action: Some(truncate(&record.action, 50)),
            ai_prompt_tokens: (record.prompt_tokens > 0).then_some(record.prompt_tokens),
            ai_completion_tokens: (record.completion_tokens > 0)
                .then_some(record.completion_tokens),
            ai_lines_suggested: suggested,
            ai_lines_accepted: accepted,
            ai_lines_rejected: None,
            is_ai_agent: Some(true),
        };

        vec![Heartbeat {
            id: uuid::Uuid::new_v4().to_string(),
            entity: record.entity,
            entity_type: record.entity_type.to_string(),
            time: record.time,
            project,
            branch,
            language,
            is_write: record.is_write,
            lines: None,
            lineno: None,
            cursorpos: None,
            user_agent: Some(self.user_agent_with_model(&record.model)),
            category: Some(AI_CATEGORY.to_string()),
            machine: self.machine.clone(),
            // The API JSON-encodes a structured editor into a plain string
            // column, which nothing can classify afterwards; it derives the
            // editor from `user_agent` instead, which already names Claude Code.
            editor: None,
            operating_system: None,
            commit_hash,
            commit_author,
            commit_message,
            repository_url,
            dependencies: Vec::new(),
            ai,
        }]
    }

    /// Appends a `model/<name>` token to the finished user agent.
    ///
    /// It has to go on afterwards: `generate_user_agent` keeps only the first
    /// two whitespace-separated parts of the plugin string, so a model token
    /// passed in there is silently dropped. The server stores `user_agent`
    /// verbatim, and this is the only column that can hold the model at all.
    fn user_agent_with_model(&self, model: &str) -> String {
        let base = generate_user_agent(self.plugin);
        if model.is_empty() {
            return base;
        }
        format!("{} model/{}", base, model)
    }

    async fn project_for(&mut self, path: &str) -> Option<String> {
        if let Some(cached) = self.project_cache.get(path) {
            return cached.clone();
        }
        let resolved = self.collector.detect_project(path).await.and_then(|info| {
            info.root
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        });
        self.project_cache
            .insert(path.to_string(), resolved.clone());
        resolved
    }

    async fn git_for(&mut self, path: &str) -> Option<GitInfo> {
        if let Some(cached) = self.git_cache.get(path) {
            return cached.clone();
        }
        let resolved = self.collector.detect_git_info(path).await;
        self.git_cache.insert(path.to_string(), resolved.clone());
        resolved
    }

    async fn language_for(&mut self, path: &str) -> Option<String> {
        if let Some(cached) = self.language_cache.get(path) {
            return cached.clone();
        }
        let resolved = self.collector.detect_language(path).await;
        self.language_cache
            .insert(path.to_string(), resolved.clone());
        resolved
    }
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    value.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_transcript(dir: &TempDir, name: &str, lines: &[&str]) -> PathBuf {
        let path = dir.path().join(name);
        let mut file = fs::File::create(&path).unwrap();
        for line in lines {
            writeln!(file, "{}", line).unwrap();
        }
        path
    }

    fn epoch() -> DateTime<Utc> {
        Utc.timestamp_opt(0, 0).unwrap()
    }

    fn find<'a>(records: &'a [Record], entity: &str) -> Option<&'a Record> {
        records.iter().find(|r| r.entity == entity)
    }

    fn repo_with_commit(dir: &TempDir) -> PathBuf {
        use git2::{Repository, Signature};

        let repo_dir = dir.path().join("repo");
        fs::create_dir_all(&repo_dir).unwrap();
        let repo = Repository::init(&repo_dir).expect("init repo");
        fs::write(repo_dir.join("README.md"), "hello").unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new("README.md")).unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let sig = Signature::now("Test Author", "author@example.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
            .unwrap();
        repo.remote("origin", "https://example.com/repo.git")
            .unwrap();

        repo_dir
    }

    fn record_at(entity: &str, entity_type: &'static str, project_dir: Option<&str>) -> Record {
        Record {
            entity: entity.to_string(),
            entity_type,
            time: 1.0,
            is_write: false,
            line_changes: 0,
            action: "tool_use".to_string(),
            model: "claude-opus-5".to_string(),
            prompt_tokens: 0,
            completion_tokens: 0,
            project_dir: project_dir.map(|s| s.to_string()),
        }
    }

    fn build_one(config: Config, record: Record) -> Heartbeat {
        let collector = DataCollector::new();
        let mut ctx = BuildContext::new(&collector, &config, None, None);
        let mut built = tokio_test::block_on(ctx.build(record));
        assert_eq!(built.len(), 1, "one record builds one heartbeat");
        built.remove(0)
    }

    #[test]
    fn a_file_heartbeat_carries_the_git_information_of_its_repository() {
        let dir = TempDir::new().unwrap();
        let repo_dir = repo_with_commit(&dir);
        let file = repo_dir.join("README.md");

        let hb = build_one(
            Config::default(),
            record_at(file.to_str().unwrap(), "file", None),
        );

        assert!(hb.branch.is_some(), "branch");
        assert_eq!(hb.commit_author.as_deref(), Some("Test Author"));
        assert_eq!(hb.commit_message.as_deref(), Some("initial commit"));
        assert_eq!(
            hb.repository_url.as_deref(),
            Some("https://example.com/repo.git")
        );
        assert!(hb.commit_hash.is_some(), "commit_hash");
    }

    #[test]
    fn an_app_heartbeat_falls_back_to_the_directory_it_attributes_its_project_from() {
        let dir = TempDir::new().unwrap();
        let repo_dir = repo_with_commit(&dir);

        let hb = build_one(
            Config::default(),
            record_at("Claude sess-1", "app", repo_dir.to_str()),
        );

        assert_eq!(hb.entity_type, "app");
        assert_eq!(hb.commit_author.as_deref(), Some("Test Author"));
        assert!(hb.commit_hash.is_some(), "commit_hash");
        assert!(hb.branch.is_some(), "branch");
    }

    #[test]
    fn an_app_heartbeat_without_a_directory_reports_no_git_information() {
        let hb = build_one(Config::default(), record_at("Claude sess-1", "app", None));

        assert!(hb.branch.is_none());
        assert!(hb.commit_hash.is_none());
        assert!(hb.repository_url.is_none());
    }

    #[test]
    fn disable_git_info_suppresses_every_git_field() {
        let dir = TempDir::new().unwrap();
        let repo_dir = repo_with_commit(&dir);
        let file = repo_dir.join("README.md");

        let config = Config {
            disable_git_info: true,
            ..Config::default()
        };
        let hb = build_one(config, record_at(file.to_str().unwrap(), "file", None));

        assert!(hb.branch.is_none());
        assert!(hb.commit_hash.is_none());
        assert!(hb.commit_author.is_none());
        assert!(hb.commit_message.is_none());
        assert!(hb.repository_url.is_none());
    }

    #[test]
    fn each_hide_flag_suppresses_only_its_own_field() {
        let dir = TempDir::new().unwrap();
        let repo_dir = repo_with_commit(&dir);
        let file = repo_dir.join("README.md");

        let config = Config {
            hide_branch_names: true,
            hide_commit_message: true,
            ..Config::default()
        };
        let hb = build_one(config, record_at(file.to_str().unwrap(), "file", None));

        assert!(hb.branch.is_none(), "hidden branch");
        assert!(hb.commit_message.is_none(), "hidden message");
        assert!(hb.commit_hash.is_some(), "hash stays");
        assert!(hb.repository_url.is_some(), "url stays");
    }

    #[test]
    fn a_shell_command_that_edits_files_yields_one_record_per_file() {
        let dir = TempDir::new().unwrap();
        let path = write_transcript(
            &dir,
            "sess-bash.jsonl",
            &[
                r#"{"type":"user","timestamp":"2026-09-13T10:00:00Z","cwd":"/proj","version":"2.1.0","toolUseResult":{"stdout":"done","bashEditDiff":{"changedFiles":["/proj/src/a.ts","/proj/docs/b.md"],"moreFiles":0,"files":[{"filePath":"/proj/src/a.ts","hunks":[{"oldStart":1,"oldLines":4,"newStart":1,"newLines":9}]},{"filePath":"/proj/docs/b.md","hunks":[{"oldStart":1,"oldLines":10,"newStart":1,"newLines":7}]}]}}}"#,
            ],
        );

        let records = parse_transcript(&path, epoch()).unwrap();

        let a = find(&records, "/proj/src/a.ts").expect("ts record");
        assert_eq!(a.entity_type, "file");
        assert!(a.is_write);
        assert_eq!(a.line_changes, 5);
        assert_eq!(a.project_dir.as_deref(), Some("/proj"));

        let b = find(&records, "/proj/docs/b.md").expect("md record");
        assert_eq!(b.entity_type, "file");
        assert_eq!(b.line_changes, -3);
    }

    #[test]
    fn a_shell_command_records_every_changed_file_not_only_the_diffed_ones() {
        let dir = TempDir::new().unwrap();
        let path = write_transcript(
            &dir,
            "sess-more.jsonl",
            &[
                r#"{"type":"user","timestamp":"2026-09-13T10:00:00Z","cwd":"/proj","version":"2.1.0","toolUseResult":{"stdout":"ok","bashEditDiff":{"changedFiles":["/proj/src/a.ts","/proj/src/b.ts","/proj/docs/c.md"],"moreFiles":2,"files":[{"filePath":"/proj/src/a.ts","hunks":[{"oldStart":1,"oldLines":2,"newStart":1,"newLines":5}]}]}}}"#,
            ],
        );

        let records = parse_transcript(&path, epoch()).unwrap();

        let diffed = find(&records, "/proj/src/a.ts").expect("the diffed file");
        assert_eq!(diffed.line_changes, 3);

        for undiffed in ["/proj/src/b.ts", "/proj/docs/c.md"] {
            let r = find(&records, undiffed).expect("a file with no attached diff");
            assert_eq!(r.entity_type, "file");
            assert!(r.is_write);
            assert_eq!(
                r.line_changes, 0,
                "no hunks means no line count, not no record"
            );
        }
    }

    #[test]
    fn a_shell_result_without_changed_files_still_uses_the_diffed_ones() {
        let dir = TempDir::new().unwrap();
        let path = write_transcript(
            &dir,
            "sess-fallback.jsonl",
            &[
                r#"{"type":"user","timestamp":"2026-09-13T10:00:00Z","cwd":"/proj","version":"2.1.0","toolUseResult":{"stdout":"ok","bashEditDiff":{"moreFiles":0,"files":[{"filePath":"/proj/src/only.ts","hunks":[{"oldStart":1,"oldLines":1,"newStart":1,"newLines":4}]}]}}}"#,
            ],
        );

        let records = parse_transcript(&path, epoch()).unwrap();
        let r = find(&records, "/proj/src/only.ts").expect("fallback to files[]");
        assert_eq!(r.line_changes, 3);
    }

    #[test]
    fn an_unrecognised_changed_files_shape_degrades_to_the_diffed_files() {
        let dir = TempDir::new().unwrap();
        let path = write_transcript(
            &dir,
            "sess-shape.jsonl",
            &[
                r#"{"type":"user","timestamp":"2026-09-13T10:00:00Z","cwd":"/proj","version":"2.1.0","toolUseResult":{"stdout":"ok","bashEditDiff":{"changedFiles":7,"moreFiles":0,"files":[{"filePath":"/proj/src/kept.ts","hunks":[{"oldStart":1,"oldLines":1,"newStart":1,"newLines":3}]}]}}}"#,
            ],
        );

        let records = parse_transcript(&path, epoch()).unwrap();
        let r = find(&records, "/proj/src/kept.ts")
            .expect("a shape we cannot read must not cost the whole result");
        assert_eq!(r.line_changes, 2);
    }

    #[test]
    fn a_shell_command_that_changes_nothing_yields_no_file_record() {
        let dir = TempDir::new().unwrap();
        let path = write_transcript(
            &dir,
            "sess-plain.jsonl",
            &[
                r#"{"type":"user","timestamp":"2026-09-13T10:00:00Z","cwd":"/proj","version":"2.1.0","toolUseResult":{"stdout":"all tests passed","stderr":""}}"#,
            ],
        );

        let records = parse_transcript(&path, epoch()).unwrap();
        assert!(
            records.iter().all(|r| r.entity_type != "file"),
            "a plain shell command works on no file"
        );
    }

    #[test]
    fn a_shell_edit_of_a_task_output_artifact_is_skipped() {
        let dir = TempDir::new().unwrap();
        let path = write_transcript(
            &dir,
            "sess-artifact.jsonl",
            &[
                r#"{"type":"user","timestamp":"2026-09-13T10:00:00Z","cwd":"/proj","version":"2.1.0","toolUseResult":{"stdout":"x","bashEditDiff":{"changedFiles":["/tmp/claude-1000/sess/tasks/run.output","/proj/src/real.ts"],"moreFiles":0,"files":[{"filePath":"/tmp/claude-1000/sess/tasks/run.output","hunks":[{"oldStart":1,"oldLines":0,"newStart":1,"newLines":40}]},{"filePath":"/proj/src/real.ts","hunks":[{"oldStart":1,"oldLines":1,"newStart":1,"newLines":2}]}]}}}"#,
            ],
        );

        let records = parse_transcript(&path, epoch()).unwrap();
        assert!(
            find(&records, "/tmp/claude-1000/sess/tasks/run.output").is_none(),
            "a task output artifact is not the user's file work"
        );
        assert!(
            find(&records, "/proj/src/real.ts").is_some(),
            "the real file in the same command still counts"
        );
    }

    #[test]
    fn structured_patch_yields_a_write_with_net_line_changes() {
        let dir = TempDir::new().unwrap();
        let path = write_transcript(
            &dir,
            "sess-a.jsonl",
            &[
                r#"{"type":"user","timestamp":"2026-09-13T10:00:00Z","cwd":"/proj","version":"2.1.0","toolUseResult":{"filePath":"/proj/src/main.rs","structuredPatch":[{"newLines":12,"oldLines":4}]}}"#,
            ],
        );

        let records = parse_transcript(&path, epoch()).unwrap();
        let record = find(&records, "/proj/src/main.rs").expect("file record");

        assert_eq!(record.entity_type, "file");
        assert!(record.is_write);
        assert_eq!(record.line_changes, 8);
        assert_eq!(record.action, "edit");
        assert_eq!(record.project_dir.as_deref(), Some("/proj"));
    }

    #[test]
    fn net_deletion_is_clamped_to_a_non_negative_accepted_count() {
        let dir = TempDir::new().unwrap();
        let path = write_transcript(
            &dir,
            "sess-b.jsonl",
            &[
                r#"{"type":"user","timestamp":"2026-09-13T10:01:00Z","cwd":"/proj","toolUseResult":{"filePath":"/proj/src/old.rs","structuredPatch":[{"newLines":2,"oldLines":20}]}}"#,
            ],
        );

        let records = parse_transcript(&path, epoch()).unwrap();
        let record = find(&records, "/proj/src/old.rs").expect("file record");
        assert_eq!(record.line_changes, -18);

        let (suggested, accepted) = if record.line_changes == 0 {
            (None, None)
        } else {
            (
                Some(record.line_changes.abs()),
                Some(record.line_changes.max(0)),
            )
        };
        assert_eq!(suggested, Some(18));
        assert_eq!(accepted, Some(0), "the API rejects negative line counts");
    }

    #[test]
    fn a_read_result_produces_no_heartbeat() {
        let dir = TempDir::new().unwrap();
        let path = write_transcript(
            &dir,
            "sess-c.jsonl",
            &[
                r#"{"type":"user","timestamp":"2026-09-13T10:02:00Z","cwd":"/proj","toolUseResult":{"filePath":"/proj/README.md","originalFile":"line1\nline2","content":"line1\nline2"}}"#,
            ],
        );

        let records = parse_transcript(&path, epoch()).unwrap();
        assert!(records.is_empty(), "got {:?}", records);
    }

    #[test]
    fn a_user_prompt_produces_an_app_record() {
        let dir = TempDir::new().unwrap();
        let path = write_transcript(
            &dir,
            "sess-d.jsonl",
            &[
                r#"{"type":"user","timestamp":"2026-09-13T10:03:00Z","cwd":"/proj","message":{"role":"user","content":[{"type":"text","text":"please refactor this"}]}}"#,
            ],
        );

        let records = parse_transcript(&path, epoch()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].entity, "Claude sess-d");
        assert_eq!(records[0].entity_type, "app");
        assert_eq!(records[0].action, "prompt");
        assert!(!records[0].is_write);
    }

    #[test]
    fn a_prompt_that_is_only_wrapper_tags_is_ignored() {
        let dir = TempDir::new().unwrap();
        let path = write_transcript(
            &dir,
            "sess-e.jsonl",
            &[
                r#"{"type":"user","timestamp":"2026-09-13T10:04:00Z","cwd":"/proj","message":{"role":"user","content":[{"type":"text","text":"<system-reminder>noise</system-reminder>"}]}}"#,
            ],
        );

        assert!(parse_transcript(&path, epoch()).unwrap().is_empty());
    }

    #[test]
    fn subagent_task_output_artifacts_are_skipped() {
        let dir = TempDir::new().unwrap();
        let path = write_transcript(
            &dir,
            "sess-f.jsonl",
            &[
                r#"{"type":"user","timestamp":"2026-09-13T10:05:00Z","cwd":"/proj","toolUseResult":{"filePath":"/tmp/claude-1000/abc/tasks/x.output","structuredPatch":[{"newLines":5,"oldLines":0}]}}"#,
            ],
        );

        assert!(parse_transcript(&path, epoch()).unwrap().is_empty());
    }

    #[test]
    fn lines_before_the_cutoff_are_excluded() {
        let dir = TempDir::new().unwrap();
        let path = write_transcript(
            &dir,
            "sess-g.jsonl",
            &[
                r#"{"type":"user","timestamp":"2026-09-13T09:00:00Z","cwd":"/proj","toolUseResult":{"filePath":"/proj/a.rs","structuredPatch":[{"newLines":3,"oldLines":0}]}}"#,
                r#"{"type":"user","timestamp":"2026-09-13T11:00:00Z","cwd":"/proj","toolUseResult":{"filePath":"/proj/b.rs","structuredPatch":[{"newLines":4,"oldLines":0}]}}"#,
            ],
        );

        let cutoff = DateTime::parse_from_rfc3339("2026-09-13T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let records = parse_transcript(&path, cutoff).unwrap();

        assert!(find(&records, "/proj/a.rs").is_none());
        assert!(find(&records, "/proj/b.rs").is_some());
    }

    #[test]
    fn token_usage_is_reported_as_a_delta_not_a_running_total() {
        let dir = TempDir::new().unwrap();
        let path = write_transcript(
            &dir,
            "sess-h.jsonl",
            &[
                r#"{"type":"assistant","timestamp":"2026-09-13T10:00:00Z","cwd":"/proj","message":{"id":"m1","role":"assistant","model":"claude-opus-5","usage":{"input_tokens":100,"output_tokens":10}}}"#,
                r#"{"type":"user","timestamp":"2026-09-13T10:00:01Z","cwd":"/proj","toolUseResult":{"filePath":"/proj/x.rs","structuredPatch":[{"newLines":2,"oldLines":0}]}}"#,
                r#"{"type":"assistant","timestamp":"2026-09-13T10:00:02Z","cwd":"/proj","message":{"id":"m2","role":"assistant","model":"claude-opus-5","usage":{"input_tokens":50,"output_tokens":5}}}"#,
                r#"{"type":"user","timestamp":"2026-09-13T10:00:03Z","cwd":"/proj","toolUseResult":{"filePath":"/proj/y.rs","structuredPatch":[{"newLines":3,"oldLines":0}]}}"#,
            ],
        );

        let records = parse_transcript(&path, epoch()).unwrap();
        let first = find(&records, "/proj/x.rs").expect("first file record");
        let second = find(&records, "/proj/y.rs").expect("second file record");

        assert_eq!(first.prompt_tokens, 100);
        assert_eq!(first.completion_tokens, 10);
        assert_eq!(second.prompt_tokens, 50, "must not re-report the first 100");
        assert_eq!(second.completion_tokens, 5);
    }

    #[test]
    fn a_restreamed_message_replaces_rather_than_doubles_its_tokens() {
        let dir = TempDir::new().unwrap();
        let path = write_transcript(
            &dir,
            "sess-i.jsonl",
            &[
                r#"{"type":"assistant","timestamp":"2026-09-13T10:00:00Z","cwd":"/proj","message":{"id":"m1","role":"assistant","usage":{"input_tokens":100,"output_tokens":5}}}"#,
                r#"{"type":"assistant","timestamp":"2026-09-13T10:00:01Z","cwd":"/proj","message":{"id":"m1","role":"assistant","usage":{"input_tokens":100,"output_tokens":20}}}"#,
                r#"{"type":"user","timestamp":"2026-09-13T10:00:02Z","cwd":"/proj","toolUseResult":{"filePath":"/proj/z.rs","structuredPatch":[{"newLines":1,"oldLines":0}]}}"#,
            ],
        );

        let records = parse_transcript(&path, epoch()).unwrap();
        let record = find(&records, "/proj/z.rs").expect("file record");
        assert_eq!(record.prompt_tokens, 100, "input counted once, not twice");
        assert_eq!(record.completion_tokens, 20);
    }

    #[test]
    fn agent_bookkeeping_results_do_not_become_heartbeats() {
        let dir = TempDir::new().unwrap();
        let path = write_transcript(
            &dir,
            "sess-j.jsonl",
            &[
                r#"{"type":"user","timestamp":"2026-09-13T10:06:00Z","cwd":"/proj","toolUseResult":{"agentId":"a1","agentType":"explore","results":["x"]}}"#,
            ],
        );

        assert!(parse_transcript(&path, epoch()).unwrap().is_empty());
    }

    #[test]
    fn a_file_path_without_cwd_falls_back_to_the_files_own_directory() {
        let dir = TempDir::new().unwrap();
        let path = write_transcript(
            &dir,
            "sess-k.jsonl",
            &[
                r#"{"type":"user","timestamp":"2026-09-13T10:07:00Z","toolUseResult":{"filePath":"/elsewhere/deep/file.rs","structuredPatch":[{"newLines":2,"oldLines":1}]}}"#,
            ],
        );

        let records = parse_transcript(&path, epoch()).unwrap();
        let record = find(&records, "/elsewhere/deep/file.rs").expect("file record");
        assert_eq!(
            record.project_dir, None,
            "a cwd inferred from the file path must not override project detection"
        );
    }

    #[test]
    fn unparseable_lines_are_skipped_without_losing_the_rest() {
        let dir = TempDir::new().unwrap();
        let path = write_transcript(
            &dir,
            "sess-l.jsonl",
            &[
                "{not json at all",
                r#"{"type":"user","timestamp":"2026-09-13T10:08:00Z","cwd":"/proj","toolUseResult":{"filePath":"/proj/ok.rs","structuredPatch":[{"newLines":1,"oldLines":0}]}}"#,
            ],
        );

        let records = parse_transcript(&path, epoch()).unwrap();
        assert!(find(&records, "/proj/ok.rs").is_some());
    }

    #[test]
    fn new_string_without_old_string_counts_its_own_lines() {
        let dir = TempDir::new().unwrap();
        let path = write_transcript(
            &dir,
            "sess-m.jsonl",
            &[
                r#"{"type":"user","timestamp":"2026-09-13T10:09:00Z","cwd":"/proj","toolUseResult":{"filePath":"/proj/new.rs","newString":"a\nb\nc"}}"#,
            ],
        );

        let records = parse_transcript(&path, epoch()).unwrap();
        let record = find(&records, "/proj/new.rs").expect("file record");
        assert_eq!(record.line_changes, 3);
        assert!(record.is_write);
    }

    #[test]
    fn task_output_path_detection() {
        assert!(is_task_output_path("/tmp/claude-1000/x/tasks/a.output"));
        assert!(is_task_output_path(r"C:\tmp\claude-42\x\y\tasks\b.output"));
        assert!(!is_task_output_path("/tmp/claude-1000/tasks/a.output"));
        assert!(!is_task_output_path("/proj/src/tasks/a.rs"));
        assert!(!is_task_output_path("/proj/src/main.rs"));
    }

    #[test]
    fn prompt_text_length_ignores_wrapper_tags() {
        assert_eq!(prompt_text_length("hello"), 5);
        assert_eq!(
            prompt_text_length("<system-reminder>x</system-reminder>"),
            0
        );
        assert_eq!(
            prompt_text_length("<system-reminder>x</system-reminder>hi there"),
            8
        );
        assert_eq!(prompt_text_length("   "), 0);
        assert_eq!(prompt_text_length("<unclosed>"), 0);
    }

    #[test]
    fn app_entity_is_named_after_the_transcript() {
        assert_eq!(
            app_heartbeat_entity("Claude", "/a/b/sess-1.jsonl"),
            "Claude sess-1"
        );
        assert_eq!(app_heartbeat_entity("Claude", ""), "Claude");
        assert_eq!(app_heartbeat_entity("Claude", "claude.jsonl"), "Claude");
    }

    #[test]
    fn count_string_lines_counts_a_single_line_payload_as_one() {
        assert_eq!(count_string_lines("one"), 1);
        assert_eq!(count_string_lines("one\ntwo"), 2);
        assert_eq!(count_string_lines("one\ntwo\n"), 3);
    }

    #[test]
    fn model_token_includes_effort_only_when_present() {
        assert_eq!(model_token("claude-opus-5", "high"), "claude-opus-5-high");
        assert_eq!(model_token("claude-opus-5", ""), "claude-opus-5");
        assert_eq!(model_token("", "high"), "");
    }

    #[test]
    fn a_transcript_larger_than_the_tail_window_still_parses() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("huge.jsonl");
        let mut file = fs::File::create(&path).unwrap();

        let filler = "x".repeat(4096);
        let mut written = 0u64;
        while written < MAX_TRANSCRIPT_TAIL_BYTES + 8192 {
            let line = format!(r#"{{"type":"user","padding":"{}"}}"#, filler);
            writeln!(file, "{}", line).unwrap();
            written += line.len() as u64 + 1;
        }
        writeln!(
            file,
            r#"{{"type":"user","timestamp":"2026-09-13T10:10:00Z","cwd":"/proj","toolUseResult":{{"filePath":"/proj/tail.rs","structuredPatch":[{{"newLines":9,"oldLines":0}}]}}}}"#
        )
        .unwrap();
        drop(file);

        let records = parse_transcript(&path, epoch()).unwrap();
        let record = find(&records, "/proj/tail.rs").expect("tail record");
        assert_eq!(record.line_changes, 9);
    }
}
