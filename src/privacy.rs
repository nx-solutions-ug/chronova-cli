//! Entity filtering and identifier redaction.
//!
//! Two distinct privacy mechanisms live here, and the difference matters:
//!
//! * **Filtering** (`exclude` / `include` / `exclude_unknown_project`) decides
//!   whether a heartbeat exists at all. A filtered-out heartbeat is never
//!   queued, so it cannot leak later from the offline queue.
//! * **Redaction** (the `hide_*` family) keeps the heartbeat — the activity
//!   still counts — and replaces the identifying parts of it.
//!
//! Patterns are POSIX-ish regular expressions matched case-insensitively
//! against the entity path, mirroring wakatime-cli. A pattern that does not
//! compile is reported at `warn` and skipped; one bad line in a config file
//! must not take the other patterns, or the run, down with it.

use regex::{Regex, RegexBuilder};
use std::path::Path;

use crate::config::Config;
use crate::heartbeat::Heartbeat;

/// The value substituted for an identifier that must not leave the machine.
pub const HIDDEN: &str = "HIDDEN";

/// How a `hide_*` setting was configured.
///
/// The config file only ever expresses a boolean, but a CLI flag may also
/// carry regex patterns, in which case redaction applies only to entities
/// matching one of them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum HideRule {
    #[default]
    Never,
    Always,
    /// Redact only entities matching one of these patterns.
    Patterns(Vec<String>),
}

impl From<bool> for HideRule {
    fn from(enabled: bool) -> Self {
        if enabled {
            HideRule::Always
        } else {
            HideRule::Never
        }
    }
}

impl HideRule {
    /// Parse a `--hide-*` flag value.
    ///
    /// `true`/`false` (in any case) are booleans; anything else is a list of
    /// regex patterns, one per line, matching how the config file spells its
    /// multi-line `exclude`/`include` lists.
    pub fn parse(value: &str) -> Self {
        let value = value.trim();
        match value.to_ascii_lowercase().as_str() {
            "true" => HideRule::Always,
            "false" | "" => HideRule::Never,
            _ => {
                let patterns: Vec<String> = split_patterns(value);
                if patterns.is_empty() {
                    HideRule::Never
                } else {
                    HideRule::Patterns(patterns)
                }
            }
        }
    }
}

fn split_patterns(value: &str) -> Vec<String> {
    value
        .split('\n')
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

/// Compile a list of patterns, warning about and skipping the ones that are
/// not valid regular expressions.
fn compile(patterns: &[String], setting: &str) -> Vec<Regex> {
    patterns
        .iter()
        .filter_map(
            |pattern| match RegexBuilder::new(pattern).case_insensitive(true).build() {
                Ok(regex) => Some(regex),
                Err(e) => {
                    tracing::warn!("ignoring invalid {} pattern {:?}: {}", setting, pattern, e);
                    None
                }
            },
        )
        .collect()
}

/// A [`HideRule`] with its patterns compiled once.
#[derive(Debug)]
enum CompiledHideRule {
    Never,
    Always,
    Patterns(Vec<Regex>),
}

impl CompiledHideRule {
    fn new(rule: &HideRule, setting: &str) -> Self {
        match rule {
            HideRule::Never => CompiledHideRule::Never,
            HideRule::Always => CompiledHideRule::Always,
            HideRule::Patterns(patterns) => CompiledHideRule::Patterns(compile(patterns, setting)),
        }
    }

    fn applies_to(&self, entity: &str) -> bool {
        match self {
            CompiledHideRule::Never => false,
            CompiledHideRule::Always => true,
            CompiledHideRule::Patterns(patterns) => patterns.iter().any(|p| p.is_match(entity)),
        }
    }
}

/// Filtering and redaction rules, compiled once from a [`Config`].
///
/// Compiling up front means an invalid pattern is reported once per run
/// instead of once per heartbeat.
#[derive(Debug)]
pub struct Sanitizer {
    exclude: Vec<Regex>,
    include: Vec<Regex>,
    exclude_unknown_project: bool,
    hide_file_names: CompiledHideRule,
    hide_project_names: CompiledHideRule,
    hide_branch_names: CompiledHideRule,
    hide_project_folder: bool,
}

impl Sanitizer {
    pub fn new(config: &Config) -> Self {
        Self {
            exclude: compile(&config.ignore_patterns, "exclude"),
            include: compile(&config.include_patterns, "include"),
            exclude_unknown_project: config.exclude_unknown_project,
            hide_file_names: CompiledHideRule::new(&config.hide_file_names, "hide_file_names"),
            hide_project_names: CompiledHideRule::new(
                &config.hide_project_names,
                "hide_project_names",
            ),
            hide_branch_names: CompiledHideRule::new(
                &config.hide_branch_names,
                "hide_branch_names",
            ),
            hide_project_folder: config.hide_project_folder,
        }
    }

    /// Whether an entity may be sent.
    ///
    /// `include` wins over `exclude` when both match, which is wakatime-cli's
    /// documented precedence. When any `include` pattern is configured, an
    /// entity must match one of them to be sent at all.
    pub fn allows_entity(&self, entity: &str) -> bool {
        if self.include.iter().any(|p| p.is_match(entity)) {
            return true;
        }
        if !self.include.is_empty() {
            return false;
        }
        !self.exclude.iter().any(|p| p.is_match(entity))
    }

    /// Whether a heartbeat with this project may be sent.
    ///
    /// Must be asked before redaction: once `--hide-project-names` has
    /// replaced the project with `HIDDEN`, an undetected project no longer
    /// looks undetected.
    pub fn allows_project(&self, project: Option<&str>) -> bool {
        if !self.exclude_unknown_project {
            return true;
        }
        project.is_some_and(|p| !p.is_empty())
    }

    /// Whether `--hide-branch-names` covers this entity.
    ///
    /// Exposed for the AI-sync path, which drops the branch instead of
    /// replacing it and so never calls [`Sanitizer::redact`].
    pub fn hides_branch_name(&self, entity: &str) -> bool {
        self.hide_branch_names.applies_to(entity)
    }

    /// Whether the entity path has to be made relative to its project root.
    ///
    /// Callers use this to avoid detecting the project root when no flag
    /// needs it.
    pub fn strips_project_folder(&self) -> bool {
        self.hide_project_folder
    }

    /// Replace the identifying parts of a heartbeat in place.
    ///
    /// All `hide_*` patterns are matched against the entity as it arrived, so
    /// stripping the project folder cannot change which rules fire.
    pub fn redact(&self, heartbeat: &mut Heartbeat, project_root: Option<&Path>) {
        let entity = heartbeat.entity.clone();

        if self.hide_project_folder {
            match project_relative(&heartbeat.entity, project_root) {
                Some(relative) => heartbeat.entity = relative,
                // A privacy flag that could not act must say so rather than
                // leave the user believing the path was shortened.
                None if heartbeat.entity_type == "file" => tracing::warn!(
                    "hide_project_folder: no project root below {:?}, sending the path as is",
                    heartbeat.entity
                ),
                None => {}
            }
        }

        if heartbeat.entity_type == "file" && self.hide_file_names.applies_to(&entity) {
            heartbeat.entity = hidden_file_name(&heartbeat.entity);
        }

        if self.hide_project_names.applies_to(&entity) && heartbeat.project.is_some() {
            heartbeat.project = Some(HIDDEN.to_string());
        }

        if self.hide_branch_names.applies_to(&entity) && heartbeat.branch.is_some() {
            heartbeat.branch = Some(HIDDEN.to_string());
        }
    }
}

/// The entity path relative to its project root, or `None` when it is not
/// below that root (nothing to strip, and guessing would corrupt the path).
fn project_relative(entity: &str, project_root: Option<&Path>) -> Option<String> {
    let root = project_root?;
    Path::new(entity)
        .strip_prefix(root)
        .ok()
        .map(|relative| relative.to_string_lossy().into_owned())
}

/// `HIDDEN` plus the original extension, so the server can still tell what
/// kind of file was worked on.
fn hidden_file_name(entity: &str) -> String {
    match Path::new(entity).extension().and_then(|e| e.to_str()) {
        Some(extension) => format!("{}.{}", HIDDEN, extension),
        None => HIDDEN.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heartbeat(entity: &str) -> Heartbeat {
        Heartbeat {
            id: "test-id".to_string(),
            entity: entity.to_string(),
            entity_type: "file".to_string(),
            time: 0.0,
            project: Some("chronova-cli".to_string()),
            branch: Some("main".to_string()),
            language: None,
            is_write: false,
            lines: None,
            lineno: None,
            cursorpos: None,
            user_agent: None,
            category: None,
            machine: None,
            editor: None,
            operating_system: None,
            commit_hash: None,
            commit_author: None,
            commit_message: None,
            repository_url: None,
            dependencies: Vec::new(),
            ai: Default::default(),
        }
    }

    fn sanitizer(config: Config) -> Sanitizer {
        Sanitizer::new(&config)
    }

    #[test]
    fn hide_rule_parses_booleans_and_patterns() {
        assert_eq!(HideRule::parse("true"), HideRule::Always);
        assert_eq!(HideRule::parse("TRUE"), HideRule::Always);
        assert_eq!(HideRule::parse("false"), HideRule::Never);
        assert_eq!(HideRule::parse(""), HideRule::Never);
        assert_eq!(
            HideRule::parse("secret/\n.*\\.key$"),
            HideRule::Patterns(vec!["secret/".to_string(), ".*\\.key$".to_string()])
        );
        assert_eq!(HideRule::from(true), HideRule::Always);
        assert_eq!(HideRule::from(false), HideRule::Never);
    }

    #[test]
    fn exclude_patterns_reject_matching_entities() {
        let s = sanitizer(Config {
            ignore_patterns: vec!["/secret/".to_string()],
            ..Default::default()
        });

        assert!(!s.allows_entity("/home/dev/secret/notes.rs"));
        assert!(s.allows_entity("/home/dev/public/notes.rs"));
    }

    #[test]
    fn exclude_matching_is_case_insensitive() {
        let s = sanitizer(Config {
            ignore_patterns: vec!["secret".to_string()],
            ..Default::default()
        });

        assert!(!s.allows_entity("/home/dev/SECRET/notes.rs"));
    }

    #[test]
    fn include_patterns_reject_everything_else() {
        let s = sanitizer(Config {
            ignore_patterns: vec![],
            include_patterns: vec!["/work/".to_string()],
            ..Default::default()
        });

        assert!(s.allows_entity("/home/dev/work/main.rs"));
        assert!(!s.allows_entity("/home/dev/hobby/main.rs"));
    }

    #[test]
    fn include_wins_over_exclude() {
        let s = sanitizer(Config {
            ignore_patterns: vec![".*\\.rs$".to_string()],
            include_patterns: vec!["/work/".to_string()],
            ..Default::default()
        });

        // Matches both lists: include decides.
        assert!(s.allows_entity("/home/dev/work/main.rs"));
        // Matches exclude only.
        assert!(!s.allows_entity("/home/dev/hobby/main.rs"));
    }

    #[test]
    fn invalid_pattern_is_skipped_and_the_others_still_apply() {
        // `*.tmp` is a glob, not a regex, and does not compile.
        let s = sanitizer(Config {
            ignore_patterns: vec!["*.tmp".to_string(), "COMMIT_EDITMSG$".to_string()],
            ..Default::default()
        });

        assert!(!s.allows_entity("/repo/.git/COMMIT_EDITMSG"));
        assert!(s.allows_entity("/repo/scratch.tmp"));
    }

    #[test]
    fn unknown_project_is_only_rejected_when_configured() {
        let permissive = sanitizer(Config::default());
        assert!(permissive.allows_project(None));

        let strict = sanitizer(Config {
            exclude_unknown_project: true,
            ..Default::default()
        });
        assert!(strict.allows_project(Some("chronova-cli")));
        assert!(!strict.allows_project(None));
        assert!(!strict.allows_project(Some("")));
    }

    #[test]
    fn hide_file_names_keeps_the_extension() {
        let s = sanitizer(Config {
            hide_file_names: HideRule::Always,
            ..Default::default()
        });

        let mut hb = heartbeat("/home/dev/project/src/secret.rs");
        s.redact(&mut hb, None);
        assert_eq!(hb.entity, "HIDDEN.rs");
        // Redaction replaces, it does not drop: the rest survives.
        assert_eq!(hb.project.as_deref(), Some("chronova-cli"));
        assert_eq!(hb.entity_type, "file");
    }

    #[test]
    fn hide_file_names_handles_an_extensionless_file() {
        let s = sanitizer(Config {
            hide_file_names: HideRule::Always,
            ..Default::default()
        });

        let mut hb = heartbeat("/home/dev/project/Makefile");
        s.redact(&mut hb, None);
        assert_eq!(hb.entity, "HIDDEN");
    }

    #[test]
    fn hide_file_names_leaves_non_file_entities_alone() {
        let s = sanitizer(Config {
            hide_file_names: HideRule::Always,
            ..Default::default()
        });

        let mut hb = heartbeat("chronova-session");
        hb.entity_type = "app".to_string();
        s.redact(&mut hb, None);
        assert_eq!(hb.entity, "chronova-session");
    }

    #[test]
    fn hide_file_names_patterns_only_redact_matching_entities() {
        let s = sanitizer(Config {
            hide_file_names: HideRule::Patterns(vec!["/secret/".to_string()]),
            ..Default::default()
        });

        let mut hidden = heartbeat("/home/dev/secret/keys.rs");
        s.redact(&mut hidden, None);
        assert_eq!(hidden.entity, "HIDDEN.rs");

        let mut visible = heartbeat("/home/dev/public/keys.rs");
        s.redact(&mut visible, None);
        assert_eq!(visible.entity, "/home/dev/public/keys.rs");
    }

    #[test]
    fn hide_project_and_branch_names_replace_their_values() {
        let s = sanitizer(Config {
            hide_project_names: HideRule::Always,
            hide_branch_names: HideRule::Always,
            ..Default::default()
        });

        let mut hb = heartbeat("/home/dev/project/src/main.rs");
        s.redact(&mut hb, None);
        assert_eq!(hb.project.as_deref(), Some("HIDDEN"));
        assert_eq!(hb.branch.as_deref(), Some("HIDDEN"));
        // The entity is untouched by these two flags.
        assert_eq!(hb.entity, "/home/dev/project/src/main.rs");
    }

    #[test]
    fn hide_project_folder_makes_the_entity_relative() {
        let s = sanitizer(Config {
            hide_project_folder: true,
            ..Default::default()
        });
        assert!(s.strips_project_folder());

        let mut hb = heartbeat("/home/dev/project/src/main.rs");
        s.redact(&mut hb, Some(Path::new("/home/dev/project")));
        assert_eq!(hb.entity, "src/main.rs");
    }

    #[test]
    fn hide_project_folder_leaves_paths_outside_the_project_alone() {
        let s = sanitizer(Config {
            hide_project_folder: true,
            ..Default::default()
        });

        let mut hb = heartbeat("/elsewhere/main.rs");
        s.redact(&mut hb, Some(Path::new("/home/dev/project")));
        assert_eq!(hb.entity, "/elsewhere/main.rs");

        let mut unknown_root = heartbeat("/elsewhere/main.rs");
        s.redact(&mut unknown_root, None);
        assert_eq!(unknown_root.entity, "/elsewhere/main.rs");
    }

    #[test]
    fn hide_patterns_match_the_entity_as_it_arrived() {
        // The pattern only matches the absolute path, so stripping the
        // project folder first must not stop it firing.
        let s = sanitizer(Config {
            hide_project_folder: true,
            hide_file_names: HideRule::Patterns(vec!["^/home/dev/project/".to_string()]),
            ..Default::default()
        });

        let mut hb = heartbeat("/home/dev/project/src/main.rs");
        s.redact(&mut hb, Some(Path::new("/home/dev/project")));
        assert_eq!(hb.entity, "HIDDEN.rs");
    }
}
