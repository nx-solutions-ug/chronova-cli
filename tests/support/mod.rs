use chronova_cli::heartbeat::Heartbeat;

pub const HEARTBEAT_PATH: &str = "/users/current/heartbeats";

pub fn queued_heartbeat(id: &str) -> Heartbeat {
    Heartbeat {
        id: id.to_string(),
        entity: format!("/src/{}.rs", id),
        entity_type: "file".to_string(),
        time: 1_700_000_000.0,
        project: Some("chronova-cli".to_string()),
        branch: None,
        language: Some("Rust".to_string()),
        is_write: false,
        lines: None,
        lineno: None,
        cursorpos: None,
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
        ai: Default::default(),
    }
}
