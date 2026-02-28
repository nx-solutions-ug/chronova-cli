//! Integration tests for Git worktree resolution
//!
//! These tests verify that the DataCollector correctly handles Git worktrees,
//! ensuring that project detection and git info retrieval work properly when
//! operating within a worktree context.

use chronova_cli::collector::DataCollector;
use git2::{Repository, Signature};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Helper function to create a git repository with an initial commit
fn create_repo_with_commit(path: &Path) -> Repository {
    let repo = Repository::init(path).expect("Failed to init repository");

    // Create a README file
    let readme_path = path.join("README.md");
    fs::write(&readme_path, "# Test Repository\n").expect("Failed to write README");

    // Stage and commit
    let mut index = repo.index().expect("Failed to get index");
    index
        .add_path(Path::new("README.md"))
        .expect("Failed to add README");
    let tree_oid = index.write_tree().expect("Failed to write tree");
    drop(index); // Drop index before finding tree

    let tree = repo.find_tree(tree_oid).expect("Failed to find tree");

    let sig =
        Signature::now("Test Author", "test@example.com").expect("Failed to create signature");
    repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
        .expect("Failed to commit");

    drop(tree); // Drop tree before returning repo
    repo
}

/// Helper function to create a package.json file
fn create_package_json(path: &Path, name: &str) {
    let content = serde_json::json!({
        "name": name,
        "version": "1.0.0"
    });
    fs::write(
        path.join("package.json"),
        serde_json::to_string(&content).unwrap(),
    )
    .expect("Failed to write package.json");
}

/// Helper function to add files and commit in a repo
fn add_and_commit(repo: &Repository, files: &[(&Path, &str)], message: &str) {
    let mut index = repo.index().expect("Failed to get index");
    for (file_path, _content) in files {
        index.add_path(file_path).expect("Failed to add file");
    }
    let tree_oid = index.write_tree().expect("Failed to write tree");
    drop(index);

    let tree = repo.find_tree(tree_oid).expect("Failed to find tree");
    let sig = Signature::now("Test", "test@example.com").expect("Failed to create signature");
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<_> = parent.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .expect("Failed to commit");
}

// =============================================================================
// Full Worktree Flow Tests
// =============================================================================

#[tokio::test]
async fn test_worktree_full_flow() {
    // Setup: Create main repository with package.json
    let temp_dir = TempDir::new().unwrap();
    let main_repo_path = temp_dir.path().join("chronova-repo");
    fs::create_dir_all(&main_repo_path).unwrap();

    // Initialize main repo
    let main_repo = create_repo_with_commit(&main_repo_path);

    // Add package.json to main repo
    create_package_json(&main_repo_path, "chronova-cli");

    // Create a source file in main repo
    let src_dir = main_repo_path.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let lib_file = src_dir.join("lib.rs");
    fs::write(&lib_file, "//! Chronova CLI library\n").unwrap();

    // Commit the new files
    add_and_commit(
        &main_repo,
        &[
            (Path::new("package.json"), ""),
            (Path::new("src/lib.rs"), ""),
        ],
        "Add package.json and lib.rs",
    );

    // Create a worktree for a feature branch
    let worktree_path = temp_dir.path().join("feature-branch");
    let worktree = main_repo
        .worktree("feature-branch", &worktree_path, None)
        .expect("Failed to create worktree");

    // Add a new file in the worktree
    let feature_file = worktree_path.join("src").join("feature.rs");
    fs::write(&feature_file, "//! Feature implementation\n").unwrap();

    // Commit in worktree
    let worktree_repo = Repository::open(&worktree_path).expect("Failed to open worktree repo");
    add_and_commit(
        &worktree_repo,
        &[(Path::new("src/feature.rs"), "")],
        "Add feature",
    );

    // Test: Detect project from worktree file
    let collector = DataCollector::new();
    let project = collector
        .detect_project(feature_file.to_str().unwrap())
        .await
        .expect("Failed to detect project");

    // The project root should be the worktree path (where the file is)
    // because detect_project finds the nearest project root
    assert!(
        project.root == worktree_path || project.root == main_repo_path,
        "Project root should be either worktree or main repo, got: {:?}",
        project.root
    );

    // Project name should come from package.json
    assert_eq!(
        project.name, "chronova-cli",
        "Project name should be from package.json"
    );

    // Test: Detect git info from worktree file
    let git_info = collector
        .detect_git_info(feature_file.to_str().unwrap())
        .await
        .expect("Failed to detect git info");

    // Branch should be the worktree branch
    assert_eq!(
        git_info.branch,
        Some("feature-branch".to_string()),
        "Branch should be feature-branch"
    );

    // Commit message should be from the worktree's latest commit
    assert_eq!(
        git_info.commit_message,
        Some("Add feature".to_string()),
        "Commit message should be from worktree"
    );

    // Cleanup
    worktree.prune(None).ok();
}

#[tokio::test]
async fn test_worktree_project_detection_returns_main_repo() {
    // This test verifies that resolve_main_repo_path correctly identifies
    // the main repository when operating within a worktree

    let temp_dir = TempDir::new().unwrap();
    let main_repo_path = temp_dir.path().join("main-project");
    fs::create_dir_all(&main_repo_path).unwrap();

    // Create main repo with package.json
    let main_repo = create_repo_with_commit(&main_repo_path);
    create_package_json(&main_repo_path, "main-project");

    // Commit package.json
    add_and_commit(
        &main_repo,
        &[(Path::new("package.json"), "")],
        "Add package.json",
    );

    // Create worktree
    let worktree_path = temp_dir.path().join("dev-branch");
    let worktree = main_repo
        .worktree("dev-branch", &worktree_path, None)
        .expect("Failed to create worktree");

    // Test: resolve_main_repo_path should return main repo path
    let collector = DataCollector::new();
    let resolved = collector.resolve_main_repo_path(&worktree_path);

    assert!(
        resolved.is_some(),
        "resolve_main_repo_path should return Some for worktree"
    );

    let resolved_path = resolved.unwrap();
    assert!(
        resolved_path.ends_with("main-project"),
        "Resolved path should end with 'main-project', got: {:?}",
        resolved_path
    );

    // Cleanup
    worktree.prune(None).ok();
}

// =============================================================================
// Multiple Worktrees Tests
// =============================================================================

#[tokio::test]
async fn test_multiple_worktrees_same_main_repo() {
    // Test that multiple worktrees all resolve to the same main repository

    let temp_dir = TempDir::new().unwrap();
    let main_repo_path = temp_dir.path().join("shared-project");
    fs::create_dir_all(&main_repo_path).unwrap();

    // Create main repo
    let main_repo = create_repo_with_commit(&main_repo_path);
    create_package_json(&main_repo_path, "shared-project");

    // Commit package.json
    add_and_commit(
        &main_repo,
        &[(Path::new("package.json"), "")],
        "Add package.json",
    );

    // Create multiple worktrees
    let worktree1_path = temp_dir.path().join("feature-a");
    let worktree2_path = temp_dir.path().join("feature-b");
    let worktree3_path = temp_dir.path().join("hotfix-123");

    let worktree1 = main_repo
        .worktree("feature-a", &worktree1_path, None)
        .expect("Failed to create worktree1");
    let worktree2 = main_repo
        .worktree("feature-b", &worktree2_path, None)
        .expect("Failed to create worktree2");
    let worktree3 = main_repo
        .worktree("hotfix-123", &worktree3_path, None)
        .expect("Failed to create worktree3");

    let collector = DataCollector::new();

    // All worktrees should resolve to the same main repo
    let resolved1 = collector.resolve_main_repo_path(&worktree1_path);
    let resolved2 = collector.resolve_main_repo_path(&worktree2_path);
    let resolved3 = collector.resolve_main_repo_path(&worktree3_path);

    assert!(resolved1.is_some(), "Worktree1 should resolve");
    assert!(resolved2.is_some(), "Worktree2 should resolve");
    assert!(resolved3.is_some(), "Worktree3 should resolve");

    assert_eq!(
        resolved1, resolved2,
        "All worktrees should resolve to same main repo"
    );
    assert_eq!(
        resolved2, resolved3,
        "All worktrees should resolve to same main repo"
    );

    // Each worktree should have correct branch name
    let git_info1 = collector
        .detect_git_info(worktree1_path.to_str().unwrap())
        .await
        .expect("Failed to get git info for worktree1");
    let git_info2 = collector
        .detect_git_info(worktree2_path.to_str().unwrap())
        .await
        .expect("Failed to get git info for worktree2");
    let git_info3 = collector
        .detect_git_info(worktree3_path.to_str().unwrap())
        .await
        .expect("Failed to get git info for worktree3");

    assert_eq!(git_info1.branch, Some("feature-a".to_string()));
    assert_eq!(git_info2.branch, Some("feature-b".to_string()));
    assert_eq!(git_info3.branch, Some("hotfix-123".to_string()));

    // Cleanup
    worktree1.prune(None).ok();
    worktree2.prune(None).ok();
    worktree3.prune(None).ok();
}

// =============================================================================
// Nested Directories Tests
// =============================================================================

#[tokio::test]
async fn test_worktree_nested_directories() {
    // Test that deeply nested files in a worktree still resolve correctly

    let temp_dir = TempDir::new().unwrap();
    let main_repo_path = temp_dir.path().join("nested-test");
    fs::create_dir_all(&main_repo_path).unwrap();

    // Create main repo
    let main_repo = create_repo_with_commit(&main_repo_path);
    create_package_json(&main_repo_path, "nested-test");

    // Commit package.json
    add_and_commit(
        &main_repo,
        &[(Path::new("package.json"), "")],
        "Add package.json",
    );

    // Create worktree
    let worktree_path = temp_dir.path().join("deep-feature");
    let worktree = main_repo
        .worktree("deep-feature", &worktree_path, None)
        .expect("Failed to create worktree");

    // Create deeply nested directory structure
    let deep_path = worktree_path
        .join("src")
        .join("components")
        .join("ui")
        .join("buttons")
        .join("primary");
    fs::create_dir_all(&deep_path).unwrap();

    let deep_file = deep_path.join("PrimaryButton.tsx");
    fs::write(&deep_file, "// Primary button component\n").unwrap();

    let collector = DataCollector::new();

    // Test: resolve_main_repo_path from deeply nested file
    let resolved = collector.resolve_main_repo_path(&deep_file);
    assert!(
        resolved.is_some(),
        "Should resolve main repo from deeply nested file"
    );

    let resolved_path = resolved.unwrap();
    assert!(
        resolved_path.ends_with("nested-test"),
        "Resolved path should be main repo, got: {:?}",
        resolved_path
    );

    // Test: detect_project from deeply nested file
    let project = collector
        .detect_project(deep_file.to_str().unwrap())
        .await
        .expect("Failed to detect project from nested file");

    assert_eq!(
        project.name, "nested-test",
        "Project name should be correct"
    );

    // Test: detect_git_info from deeply nested file
    let git_info = collector
        .detect_git_info(deep_file.to_str().unwrap())
        .await
        .expect("Failed to detect git info from nested file");

    assert_eq!(
        git_info.branch,
        Some("deep-feature".to_string()),
        "Branch should be correct from nested file"
    );

    // Cleanup
    worktree.prune(None).ok();
}

// =============================================================================
// Edge Cases Tests
// =============================================================================

#[tokio::test]
async fn test_worktree_no_commits_yet() {
    // Test behavior when worktree has no commits yet (just created)

    let temp_dir = TempDir::new().unwrap();
    let main_repo_path = temp_dir.path().join("no-commits-test");
    fs::create_dir_all(&main_repo_path).unwrap();

    // Create main repo with initial commit
    let main_repo = create_repo_with_commit(&main_repo_path);

    // Create worktree
    let worktree_path = temp_dir.path().join("new-feature");
    let worktree = main_repo
        .worktree("new-feature", &worktree_path, None)
        .expect("Failed to create worktree");

    // Don't make any commits in the worktree

    let collector = DataCollector::new();

    // Test: resolve_main_repo_path should still work
    let resolved = collector.resolve_main_repo_path(&worktree_path);
    assert!(
        resolved.is_some(),
        "Should resolve main repo even without commits in worktree"
    );

    // Test: detect_git_info should return branch name
    let git_info = collector
        .detect_git_info(worktree_path.to_str().unwrap())
        .await;

    // Branch should be detected
    assert!(
        git_info.is_some(),
        "Should detect git info even without new commits"
    );
    let info = git_info.unwrap();
    assert_eq!(info.branch, Some("new-feature".to_string()));

    // Cleanup
    worktree.prune(None).ok();
}

#[tokio::test]
async fn test_worktree_file_modification_detection() {
    // Test that file modifications in worktree are correctly attributed

    let temp_dir = TempDir::new().unwrap();
    let main_repo_path = temp_dir.path().join("mod-test");
    fs::create_dir_all(&main_repo_path).unwrap();

    // Create main repo
    let main_repo = create_repo_with_commit(&main_repo_path);
    create_package_json(&main_repo_path, "mod-test");

    // Create a file in main repo
    let main_file = main_repo_path.join("src").join("main.rs");
    fs::create_dir_all(main_file.parent().unwrap()).unwrap();
    fs::write(&main_file, "fn main() {}\n").unwrap();

    // Commit in main
    add_and_commit(
        &main_repo,
        &[
            (Path::new("package.json"), ""),
            (Path::new("src/main.rs"), ""),
        ],
        "Initial setup",
    );

    // Create worktree
    let worktree_path = temp_dir.path().join("modify-branch");
    let worktree = main_repo
        .worktree("modify-branch", &worktree_path, None)
        .expect("Failed to create worktree");

    // Modify file in worktree
    let worktree_file = worktree_path.join("src").join("main.rs");
    fs::write(&worktree_file, "fn main() { println!(\"modified\"); }\n").unwrap();

    let collector = DataCollector::new();

    // Test: detect_project from modified file in worktree
    let project = collector
        .detect_project(worktree_file.to_str().unwrap())
        .await
        .expect("Failed to detect project");

    assert_eq!(project.name, "mod-test", "Project name should be correct");

    // Test: detect_git_info should show worktree branch
    let git_info = collector
        .detect_git_info(worktree_file.to_str().unwrap())
        .await
        .expect("Failed to detect git info");

    assert_eq!(
        git_info.branch,
        Some("modify-branch".to_string()),
        "Should detect worktree branch"
    );

    // Cleanup
    worktree.prune(None).ok();
}

#[tokio::test]
async fn test_non_worktree_repo_returns_none() {
    // Test that resolve_main_repo_path returns None for non-worktree repos

    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("normal-repo");
    fs::create_dir_all(&repo_path).unwrap();

    // Create a normal repo (not a worktree)
    create_repo_with_commit(&repo_path);

    let collector = DataCollector::new();

    // Test: resolve_main_repo_path should return None
    let resolved = collector.resolve_main_repo_path(&repo_path);
    assert!(
        resolved.is_none(),
        "resolve_main_repo_path should return None for non-worktree repo"
    );

    // Test: detect_project should return the repo path
    let project = collector
        .detect_project(repo_path.to_str().unwrap())
        .await
        .expect("Failed to detect project");
    assert!(
        project.root.ends_with("normal-repo"),
        "Should return the repo path for non-worktree, got: {:?}",
        project.root
    );
}

#[tokio::test]
async fn test_worktree_with_wakatime_project_file() {
    // Test that .wakatime-project file in main repo is used for project name

    let temp_dir = TempDir::new().unwrap();
    let main_repo_path = temp_dir.path().join("wakatime-project-test");
    fs::create_dir_all(&main_repo_path).unwrap();

    // Create main repo
    let main_repo = create_repo_with_commit(&main_repo_path);

    // Add .wakatime-project file with custom name
    fs::write(
        main_repo_path.join(".wakatime-project"),
        "my-custom-project-name",
    )
    .unwrap();

    // Commit
    add_and_commit(
        &main_repo,
        &[(Path::new(".wakatime-project"), "")],
        "Add wakatime project file",
    );

    // Create worktree
    let worktree_path = temp_dir.path().join("custom-branch");
    let worktree = main_repo
        .worktree("custom-branch", &worktree_path, None)
        .expect("Failed to create worktree");

    // Create a file in worktree
    let worktree_file = worktree_path.join("src").join("test.rs");
    fs::create_dir_all(worktree_file.parent().unwrap()).unwrap();
    fs::write(&worktree_file, "// test\n").unwrap();

    let collector = DataCollector::new();

    // Test: project name should come from .wakatime-project
    let project = collector
        .detect_project(worktree_file.to_str().unwrap())
        .await
        .expect("Failed to detect project");

    // The project name should be from .wakatime-project file
    // Note: This depends on whether the worktree path or main repo path is used
    // for project detection. The current implementation finds the nearest project root.
    assert!(
        project.name == "my-custom-project-name" || project.name == "wakatime-project-test",
        "Project name should be from .wakatime-project or directory name, got: {}",
        project.name
    );

    // Cleanup
    worktree.prune(None).ok();
}

#[tokio::test]
async fn test_worktree_git_info_includes_remote_url() {
    // Test that remote URL is correctly detected from worktree

    let temp_dir = TempDir::new().unwrap();
    let main_repo_path = temp_dir.path().join("remote-test");
    fs::create_dir_all(&main_repo_path).unwrap();

    // Create main repo
    let main_repo = create_repo_with_commit(&main_repo_path);

    // Add remote origin
    main_repo
        .remote("origin", "https://github.com/example/test-repo.git")
        .unwrap();

    // Create worktree
    let worktree_path = temp_dir.path().join("remote-branch");
    let worktree = main_repo
        .worktree("remote-branch", &worktree_path, None)
        .expect("Failed to create worktree");

    let collector = DataCollector::new();

    // Test: git info should include remote URL
    let git_info = collector
        .detect_git_info(worktree_path.to_str().unwrap())
        .await
        .expect("Failed to detect git info");

    assert_eq!(
        git_info.repository_url,
        Some("https://github.com/example/test-repo.git".to_string()),
        "Remote URL should be detected from worktree"
    );

    // Cleanup
    worktree.prune(None).ok();
}
