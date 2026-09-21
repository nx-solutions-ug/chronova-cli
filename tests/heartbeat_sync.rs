//! The public surface `HeartbeatManager` promises through `HeartbeatManagerExt`.
//!
//! This file previously held six `assert!(true, "... should exist")` tests. They
//! could not fail, so removing the trait would not have shown up. Binding the
//! implementation to a generic parameter does the check the comments described:
//! the file stops compiling if the impl goes away.

use chronova_cli::heartbeat::{HeartbeatManager, HeartbeatManagerExt};

fn assert_implements<T: HeartbeatManagerExt>() {}

#[test]
fn heartbeat_manager_implements_the_extension_trait() {
    assert_implements::<HeartbeatManager>();
}
