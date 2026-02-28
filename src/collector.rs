use git2::Repository;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ProjectInfo {
    pub name: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct GitInfo {
    pub branch: Option<String>,
    pub commit_hash: Option<String>,
    pub commit_author: Option<String>,
    pub commit_message: Option<String>,
    pub repository_url: Option<String>,
}

pub struct DataCollector;

impl Default for DataCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl DataCollector {
    pub fn new() -> Self {
        Self
    }

    pub async fn detect_project(&self, entity_path: &str) -> Option<ProjectInfo> {
        let path = Path::new(entity_path);

        // 1) Prefer explicit project markers (git, Cargo.toml, package.json, etc.)
        if let Some(root) = self.find_project_root(path) {
            let name = self.extract_project_name(&root);
            return Some(ProjectInfo { name, root });
        }

        // 2) Try to discover a git repository root via libgit2; Repository::discover climbs parents.
        if let Ok(repo) = Repository::discover(path) {
            if let Some(workdir) = repo.workdir() {
                let root = workdir.to_path_buf();
                let name = self.extract_project_name(&root);
                return Some(ProjectInfo { name, root });
            }
        }

        // 3) Heuristic: walk up ancestors looking for common source layout (e.g., 'src' directory) or package files.
        // Additionally, avoid naming projects after common code directories like 'src', 'app', 'components', etc.
        let ignored_dirs = [
            "src",
            "app",
            "components",
            "lib",
            "packages",
            "pkg",
            "dist",
            "build",
            "tests",
        ];
        let mut current = path.parent();
        while let Some(dir) = current {
            // Prefer explicit markers if present on this ancestor
            if dir.join("package.json").exists()
                || dir.join("Cargo.toml").exists()
                || dir.join("pyproject.toml").exists()
                || dir.join(".wakatime-project").exists()
                || dir.join(".git").exists()
            {
                let root = dir.to_path_buf();
                let name = self.extract_project_name(&root);
                return Some(ProjectInfo { name, root });
            }

            // If the directory name is a common code-folder, skip upwards
            if let Some(dir_name) = dir.file_name().and_then(|n| n.to_str()) {
                if ignored_dirs.iter().any(|s| s == &dir_name) {
                    current = dir.parent();
                    continue;
                } else {
                    // Use this ancestor as the project root candidate
                    let root = dir.to_path_buf();
                    let name = self.extract_project_name(&root);
                    return Some(ProjectInfo { name, root });
                }
            }

            current = dir.parent();
        }

        // 4) Fallback to immediate parent
        if let Some(parent) = path.parent() {
            let root = parent.to_path_buf();

            // If the immediate parent is a common code-folder, try its parent instead
            if let Some(parent_name) = root.file_name().and_then(|n| n.to_str()) {
                if ignored_dirs.iter().any(|s| s == &parent_name) {
                    if let Some(grand) = root.parent() {
                        let grand_root = grand.to_path_buf();
                        let name = self.extract_project_name(&grand_root);
                        return Some(ProjectInfo {
                            name,
                            root: grand_root,
                        });
                    }
                }
            }

            let name = self.extract_project_name(&root);
            return Some(ProjectInfo { name, root });
        }

        None
    }

    pub async fn detect_git_info(&self, entity_path: &str) -> Option<GitInfo> {
        let path = Path::new(entity_path);

        // Use git2 to discover repository from the entity path.
        // Wrap all operations so failures gracefully return None.
        let repo = match Repository::discover(path) {
            Ok(r) => r,
            Err(_) => return None,
        };

        // Try to get HEAD and the commit it points to
        let head = repo.head().ok();
        let branch = head
            .as_ref()
            .and_then(|h| h.shorthand().map(|s| s.to_string()));

        let commit = head.and_then(|h| h.peel_to_commit().ok());
        let commit_hash = commit.as_ref().map(|c| c.id().to_string());
        let commit_author = commit
            .as_ref()
            .and_then(|c| c.author().name().map(|s| s.to_string()));
        let commit_message = commit
            .as_ref()
            .and_then(|c| c.message().map(|s| s.to_string()));

        let repository_url = repo.find_remote("origin").ok().and_then(|r| {
            r.url().map(|s| {
                // sanitize remote URL to remove sensitive userinfo (user:pass or token before '@')
                let raw = s.to_string();

                // If scheme exists (e.g., "https://"), strip userinfo from the authority portion only
                if let Some(scheme_sep) = raw.find("://") {
                    let (scheme, rest) = raw.split_at(scheme_sep + 3); // include "://"
                                                                       // isolate authority (up to first '/') and path
                    let auth_end = rest.find('/').unwrap_or(rest.len());
                    let (authority, path) = rest.split_at(auth_end);
                    if let Some(at_pos) = authority.find('@') {
                        // remove userinfo (up to and including '@') from authority
                        let without_user = &authority[at_pos + 1..];
                        return format!("{}{}{}", scheme, without_user, path);
                    }
                    return raw;
                }

                // No scheme: handle scp-like "user@host:owner/repo.git" or "user@host/..."
                if let Some(at_pos) = raw.find('@') {
                    return raw[at_pos + 1..].to_string();
                }

                raw
            })
        });

        Some(GitInfo {
            branch,
            commit_hash,
            commit_author,
            commit_message,
            repository_url,
        })
    }

    pub async fn detect_language(&self, entity_path: &str) -> Option<String> {
        // Follow the same detection logic as src/lib/heartbeat-detection/language-mapping.ts:
        // 1) Try special filename matches (Dockerfile, Makefile, .gitignore, etc.)
        // 2) Try multi-part extensions first (e.g., .tar.gz, .log.gz)
        // 3) Try single final extension (including dot) and dot-only filenames (e.g., ".env")
        let entity = entity_path;
        let lower = entity.to_lowercase();

        // basename (filename)
        let filename = match entity.rsplit('/').next() {
            Some(b) => b,
            None => entity,
        };

        // 1) Exact filename matches (case-insensitive)
        if let Some(lang) = FILENAME_MAP.get(&filename.to_lowercase().as_str()) {
            return Some(lang.clone());
        }

        // If filename starts with a dot and has no other dots, treat it as an extension-only entry (e.g., ".env")
        let dot_only = filename.starts_with('.') && filename[1..].find('.').is_none();
        if dot_only {
            if let Some(lang) = EXTENSION_MAP.get(&filename.to_lowercase().as_str()) {
                return Some(lang.clone());
            }
        }

        // 2) Multi-part extensions (try longest-first)
        const MULTI_PART_EXTS: &[&str] = &[
            ".tar.gz", ".tar.bz2", ".tar.xz", ".log.gz", ".log.bz2", ".log.xz",
        ];

        for ext in MULTI_PART_EXTS.iter() {
            if lower.ends_with(ext) {
                if let Some(lang) = EXTENSION_MAP.get(*ext) {
                    return Some(lang.clone());
                }
            }
        }

        // 3) Last extension (including leading dot)
        if let Some(pos) = filename.rfind('.') {
            let ext = &filename[pos..].to_lowercase();
            if let Some(lang) = EXTENSION_MAP.get(ext.as_str()) {
                return Some(lang.clone());
            }
        }

        None
    }

    fn find_project_root(&self, path: &Path) -> Option<PathBuf> {
        let mut current = path.parent()?;

        while current.parent().is_some() {
            // Check for common project markers
            if current.join(".git").exists()
                || current.join(".wakatime-project").exists()
                || current.join("package.json").exists()
                || current.join("Cargo.toml").exists()
                || current.join("pyproject.toml").exists()
                || current.join("go.mod").exists()
            {
                return Some(current.to_path_buf());
            }
            current = current.parent()?;
        }

        None
    }

    fn find_git_root(&self, path: &Path) -> Option<PathBuf> {
        let mut current = path.parent()?;

        while current.parent().is_some() {
            if current.join(".git").exists() {
                return Some(current.to_path_buf());
            }
            current = current.parent()?;
        }

        None
    }

    fn extract_project_name(&self, root: &Path) -> String {
        // Try to get name from .wakatime-project file
        if let Ok(content) = std::fs::read_to_string(root.join(".wakatime-project")) {
            return content.trim().to_string();
        }

        // Try to get name from package.json
        if let Ok(content) = std::fs::read_to_string(root.join("package.json")) {
            if let Ok(package) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(name) = package.get("name").and_then(|n| n.as_str()) {
                    return name.to_string();
                }
            }
        }

        // Try to get name from Cargo.toml
        if let Ok(content) = std::fs::read_to_string(root.join("Cargo.toml")) {
            for line in content.lines() {
                if line.trim().starts_with("name =") {
                    if let Some(name) = line.split('=').nth(1) {
                        return name.trim().trim_matches('"').to_string();
                    }
                }
            }
        }

        // Fall back to directory name
        root.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    /// Resolves the main repository path when operating within a git worktree.
    ///
    /// When called from within a worktree, this method parses the `.git` file
    /// to find the path to the main repository. The `.git` file in a worktree
    /// contains a line like: `gitdir: /path/to/main/.git/worktrees/<name>`
    ///
    /// # Arguments
    /// * `path` - A path within the repository (can be the worktree root or any file inside)
    ///
    /// # Returns
    /// * `Some(PathBuf)` - The path to the main repository if we're in a worktree
    /// * `None` - If we're not in a worktree, or if resolution fails
    pub fn resolve_main_repo_path(&self, path: &Path) -> Option<PathBuf> {
        // Discover the repository from the given path
        let repo = Repository::discover(path).ok()?;

        // Check if this is a worktree
        if !repo.is_worktree() {
            return None;
        }

        // For worktrees, the .git file is at the worktree root
        // Get the worktree's working directory
        let worktree_root = repo.workdir()?;

        // Read the .git file which contains the gitdir reference
        let git_file_path = worktree_root.join(".git");
        let git_file_content = std::fs::read_to_string(&git_file_path).ok()?;

        // Parse the gitdir line: "gitdir: /path/to/main/.git/worktrees/<name>"
        let gitdir_line = git_file_content
            .lines()
            .find(|line| line.starts_with("gitdir:"))?;

        let gitdir_path = gitdir_line.strip_prefix("gitdir:").map(|s| s.trim())?;

        // The gitdir points to /path/to/main/.git/worktrees/<name>
        // We need to go up 2 directories to get to the main repo's .git directory
        // Then go up one more to get the main repo root
        let gitdir = PathBuf::from(gitdir_path);

        // Go up from worktrees/<name> to .git, then to main repo root
        let main_git_dir = gitdir
            .parent()? // Remove <name> -> worktrees/
            .parent()?; // Remove worktrees/ -> .git/

        // The main repo root is the parent of .git
        let main_repo_root = main_git_dir.parent()?;

        Some(main_repo_root.to_path_buf())
    }

    /// Gets the project root path, respecting worktree boundaries.
    ///
    /// When inside a git worktree, this returns the main repository's root path
    /// rather than the worktree's path. This ensures that project detection
    /// and naming work correctly across worktrees.
    ///
    /// # Arguments
    /// * `path` - A path within the repository
    ///
    /// # Returns
    /// The path to the project root (main repo if in worktree, otherwise current repo)
    #[allow(dead_code)] // Will be used in future tasks for project detection
    fn get_project_root_respecting_worktree(&self, path: &Path) -> PathBuf {
        // First, try to resolve the main repo path if we're in a worktree
        if let Some(main_repo_path) = self.resolve_main_repo_path(path) {
            return main_repo_path;
        }

        // Not a worktree, use normal repository discovery
        if let Ok(repo) = Repository::discover(path) {
            if let Some(workdir) = repo.workdir() {
                return workdir.to_path_buf();
            }
        }

        // Fallback: return the parent directory of the path
        path.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| path.to_path_buf())
    }
}

lazy_static! {
    // Map keys mirror the heartbeat-detection LANGUAGE_MAPPING.ts which uses leading dots for extensions
    static ref EXTENSION_MAP: HashMap<&'static str, String> = {
        let mut m = HashMap::new();

        // Multi-part and dot-prefixed extensions (match like ".tar.gz", ".log.gz")
        m.insert(".tar.gz", "Archive".to_string());
        m.insert(".tar.bz2", "Archive".to_string());
        m.insert(".tar.xz", "Archive".to_string());
        m.insert(".log.gz", "Log File".to_string());
        m.insert(".log.bz2", "Log File".to_string());
        m.insert(".log.xz", "Log File".to_string());

        // Common extensions (dot-prefixed)
        m.insert(".html", "HTML".to_string());
        m.insert(".htm", "HTML".to_string());
        m.insert(".css", "CSS".to_string());
        m.insert(".scss", "SCSS".to_string());
        m.insert(".sass", "Sass".to_string());
        m.insert(".less", "Less".to_string());

        // JS/TS variants
        m.insert(".js", "JavaScript".to_string());
        m.insert(".cjs", "JavaScript".to_string());
        m.insert(".mjs", "JavaScript".to_string());
        m.insert(".jsx", "JavaScript".to_string());
        m.insert(".ts", "TypeScript".to_string());
        m.insert(".tsx", "TypeScript".to_string());

        // Languages
        m.insert(".py", "Python".to_string());
        m.insert(".pyw", "Python".to_string());
        m.insert(".java", "Java".to_string());
        m.insert(".jsp", "Java Server Pages".to_string());
        m.insert(".cpp", "C++".to_string());
        m.insert(".cc", "C++".to_string());
        m.insert(".cxx", "C++".to_string());
        m.insert(".c", "C".to_string());
        m.insert(".h", "C Header".to_string());
        m.insert(".hpp", "C++ Header".to_string());
        m.insert(".go", "Go".to_string());
        m.insert(".rs", "Rust".to_string());
        m.insert(".rb", "Ruby".to_string());
        m.insert(".php", "PHP".to_string());
        m.insert(".kt", "Kotlin".to_string());
        m.insert(".kts", "Kotlin Script".to_string());
        m.insert(".swift", "Swift".to_string());
        m.insert(".dart", "Dart".to_string());
        m.insert(".jl", "Julia".to_string());
        m.insert(".r", "R".to_string());
        m.insert(".hs", "Haskell".to_string());
        m.insert(".ex", "Elixir".to_string());
        m.insert(".exs", "Elixir Script".to_string());
        m.insert(".el", "Emacs Lisp".to_string());
        m.insert(".clj", "Clojure".to_string());
        m.insert(".scala", "Scala".to_string());
        m.insert(".jl", "Julia".to_string());

        // Data / config
        m.insert(".json", "JSON".to_string());
        m.insert(".yaml", "YAML".to_string());
        m.insert(".yml", "YAML".to_string());
        m.insert(".toml", "TOML".to_string());
        m.insert(".md", "Markdown".to_string());
        m.insert(".markdown", "Markdown".to_string());
        m.insert(".mdx", "Markdown".to_string());
        m.insert(".sql", "SQL".to_string());
        m.insert(".xml", "XML".to_string());
        m.insert(".csv", "CSV".to_string());
        m.insert(".txt", "Plain Text".to_string());
        m.insert(".ini", "INI".to_string());
        m.insert(".cfg", "Configuration".to_string());
        m.insert(".conf", "Configuration".to_string());

        // Special cases / dotfiles
        m.insert(".gitignore", "Git Ignore".to_string());
        m.insert(".env", "Environment Variables".to_string());
        m.insert(".env.local", "Environment Variables".to_string());
        m.insert(".env.development", "Environment Variables".to_string());
        m.insert(".env.test", "Environment Variables".to_string());
        m.insert(".env.production", "Environment Variables".to_string());
        m.insert(".env.example", "Environment Variables".to_string());
        m.insert(".editorconfig", "EditorConfig".to_string());

        // Makefile / small extensions
        m.insert(".mk", "Makefile".to_string());
        m.insert(".r", "R".to_string());
        m.insert(".m", "MATLAB".to_string());
        m.insert(".lua", "Lua".to_string());
        m.insert(".pl", "Perl".to_string());

        // Misc
        m.insert(".tf", "Terraform".to_string());
        m.insert(".graphql", "GraphQL".to_string());
        m.insert(".gql", "GraphQL".to_string());
        m.insert(".sol", "Solidity".to_string());
        m.insert(".styl", "Stylus".to_string());
        m.insert(".zig", "Zig".to_string());

        m
    };

    // Filename (basename) -> language name (case-insensitive lookup via lowercasing before use)
    static ref FILENAME_MAP: HashMap<&'static str, String> = {
        let mut m = HashMap::new();
        m.insert("dockerfile", "Dockerfile".to_string());
        m.insert("makefile", "Makefile".to_string());
        m.insert("readme", "Plain Text".to_string());
        m.insert("license", "Plain Text".to_string());
        m.insert("gemfile", "Ruby".to_string());
        m.insert("rakefile", "Ruby".to_string());
        m.insert("procfile", "Config".to_string());
        m.insert("package", "JSON".to_string());
        m.insert("dockerfile.dev", "Dockerfile".to_string());
        m
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_language_detection() {
        let collector = DataCollector::new();

        assert_eq!(
            tokio_test::block_on(collector.detect_language("test.rs")),
            Some("Rust".to_string())
        );
        assert_eq!(
            tokio_test::block_on(collector.detect_language("test.js")),
            Some("JavaScript".to_string())
        );
        assert_eq!(
            tokio_test::block_on(collector.detect_language("test.unknown")),
            None
        );
    }

    #[test]
    fn test_project_detection() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().join("my-project");
        fs::create_dir(&project_dir).unwrap();

        // Create a package.json file
        fs::write(
            project_dir.join("package.json"),
            r#"{"name": "test-project"}"#,
        )
        .unwrap();

        let test_file = project_dir.join("src").join("test.js");
        fs::create_dir_all(test_file.parent().unwrap()).unwrap();
        fs::write(&test_file, "// test").unwrap();

        let collector = DataCollector::new();
        let project_info =
            tokio_test::block_on(collector.detect_project(test_file.to_str().unwrap())).unwrap();

        assert_eq!(project_info.name, "test-project");
        assert_eq!(project_info.root, project_dir);
    }

    #[test]
    fn test_project_detection_fallback_to_parent_name() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().join("chronova-revised");
        fs::create_dir_all(&project_dir).unwrap();
        let file_path = project_dir.join("tmp.rs");
        fs::write(&file_path, "// test").unwrap();

        let collector = DataCollector::new();
        let project_info =
            tokio_test::block_on(collector.detect_project(file_path.to_str().unwrap())).unwrap();

        assert_eq!(project_info.name, "chronova-revised");
        assert_eq!(project_info.root, project_dir);
    }

    #[test]
    fn test_extract_project_name() {
        let temp_dir = TempDir::new().unwrap();
        let collector = DataCollector::new();

        // Test .wakatime-project file
        fs::write(
            temp_dir.path().join(".wakatime-project"),
            "my-custom-project",
        )
        .unwrap();
        assert_eq!(
            collector.extract_project_name(temp_dir.path()),
            "my-custom-project"
        );

        // Test directory name fallback
        let named_dir = TempDir::new().unwrap();
        let dir_name = named_dir.path().file_name().unwrap().to_str().unwrap();
        assert_eq!(collector.extract_project_name(named_dir.path()), dir_name);
    }

    #[test]
    fn test_detect_git_info_non_git() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file.txt");
        fs::write(&file_path, "content").unwrap();

        let collector = DataCollector::new();
        let res = tokio_test::block_on(collector.detect_git_info(file_path.to_str().unwrap()));
        assert!(
            res.is_none(),
            "detect_git_info should return None for non-git directories"
        );
    }

    #[test]
    fn test_detect_git_info_repository_metadata() {
        use git2::{Repository, Signature};

        let temp_dir = TempDir::new().unwrap();
        let repo_dir = temp_dir.path().join("repo");
        fs::create_dir_all(&repo_dir).unwrap();

        // Initialize repository and make initial commit
        let repo = Repository::init(&repo_dir).expect("init repo");
        // create a file
        let file_path = repo_dir.join("README.md");
        fs::write(&file_path, "hello").unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new("README.md")).unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();

        let sig = Signature::now("Test Author", "author@example.com").unwrap();
        let commit_oid = repo
            .commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
            .unwrap();

        // Add remote origin
        repo.remote("origin", "https://example.com/repo.git")
            .unwrap();

        let collector = DataCollector::new();
        let res = tokio_test::block_on(collector.detect_git_info(file_path.to_str().unwrap()));
        assert!(
            res.is_some(),
            "detect_git_info should detect git repo metadata"
        );

        let info = res.unwrap();
        assert!(info.commit_hash.is_some(), "commit_hash should be present");
        assert_eq!(info.commit_hash.unwrap(), commit_oid.to_string());
        assert!(info.commit_author.is_some());
        assert_eq!(info.commit_author.unwrap(), "Test Author".to_string());
        assert!(info.commit_message.is_some());
        assert_eq!(info.commit_message.unwrap(), "initial commit".to_string());
        assert!(info.repository_url.is_some());
        assert_eq!(
            info.repository_url.unwrap(),
            "https://example.com/repo.git".to_string()
        );
    }

    #[test]
    fn test_detect_git_info_sanitizes_remote_url() {
        use git2::{Repository, Signature};
        let temp_dir = TempDir::new().unwrap();
        let repo_dir = temp_dir.path().join("repo2");
        fs::create_dir_all(&repo_dir).unwrap();

        // Init repo and commit
        let repo = Repository::init(&repo_dir).expect("init repo");
        let file_path = repo_dir.join("README.md");
        fs::write(&file_path, "hello").unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new("README.md")).unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();

        let sig = Signature::now("Test Author", "author@example.com").unwrap();
        let _commit_oid = repo
            .commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
            .unwrap();

        let collector = DataCollector::new();

        // 1) HTTPS with user:password@ -> scheme preserved, userinfo removed
        repo.remote("origin", "https://user:password@github.com/owner/repo.git")
            .unwrap();
        let res = tokio_test::block_on(collector.detect_git_info(file_path.to_str().unwrap()));
        assert!(res.is_some());
        let info = res.unwrap();
        assert_eq!(
            info.repository_url.unwrap(),
            "https://github.com/owner/repo.git".to_string()
        );

        // 2) HTTPS with token@ -> remove token
        repo.remote_delete("origin").ok();
        repo.remote("origin", "https://token123@bitbucket.org/owner/repo.git")
            .unwrap();
        let res2 = tokio_test::block_on(collector.detect_git_info(file_path.to_str().unwrap()));
        assert!(res2.is_some());
        let info2 = res2.unwrap();
        assert_eq!(
            info2.repository_url.unwrap(),
            "https://bitbucket.org/owner/repo.git".to_string()
        );

        // 3) scp-like "git@host:owner/repo.git" -> strip leading "git@"
        repo.remote_delete("origin").ok();
        repo.remote("origin", "git@github.com:owner/repo.git")
            .unwrap();
        let res3 = tokio_test::block_on(collector.detect_git_info(file_path.to_str().unwrap()));
        assert!(res3.is_some());
        let info3 = res3.unwrap();
        assert_eq!(
            info3.repository_url.unwrap(),
            "github.com:owner/repo.git".to_string()
        );
    }

    #[test]
    fn test_extract_project_name_edge_cases() {
        let temp_dir = TempDir::new().unwrap();
        let collector = DataCollector::new();

        // Directory name with dots and trailing slash behavior
        let project_dir = temp_dir.path().join("my.project.name");
        fs::create_dir_all(&project_dir).unwrap();
        assert_eq!(
            collector.extract_project_name(&project_dir),
            "my.project.name".to_string()
        );
    }

    #[test]
    fn test_resolve_main_repo_path_not_worktree() {
        use git2::{Repository, Signature};

        // Create a normal git repository (not a worktree)
        let temp_dir = TempDir::new().unwrap();
        let repo_dir = temp_dir.path().join("main-repo");
        fs::create_dir_all(&repo_dir).unwrap();

        // Initialize repository and make initial commit
        let repo = Repository::init(&repo_dir).expect("init repo");
        let file_path = repo_dir.join("README.md");
        fs::write(&file_path, "hello").unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new("README.md")).unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();

        let sig = Signature::now("Test Author", "author@example.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
            .unwrap();

        // Test: For a normal repo, resolve_main_repo_path should return None
        let collector = DataCollector::new();
        let result = collector.resolve_main_repo_path(&repo_dir);

        assert!(
            result.is_none(),
            "resolve_main_repo_path should return None for a non-worktree repository"
        );
    }

    #[test]
    fn test_resolve_main_repo_path_with_worktree() {
        use git2::{Repository, Signature};

        // Create a main repository
        let temp_dir = TempDir::new().unwrap();
        let main_repo_dir = temp_dir.path().join("main-repo");
        fs::create_dir_all(&main_repo_dir).unwrap();

        // Initialize main repository
        let main_repo = Repository::init(&main_repo_dir).expect("init main repo");
        let file_path = main_repo_dir.join("README.md");
        fs::write(&file_path, "hello").unwrap();

        let mut index = main_repo.index().unwrap();
        index.add_path(Path::new("README.md")).unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = main_repo.find_tree(tree_oid).unwrap();

        let sig = Signature::now("Test Author", "author@example.com").unwrap();
        main_repo
            .commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
            .unwrap();

        // Create a worktree
        let worktree_dir = temp_dir.path().join("worktree-repo");
        let worktree = main_repo
            .worktree("test-worktree", &worktree_dir, None)
            .expect("create worktree");

        // Test: For a worktree, resolve_main_repo_path should return the main repo path
        let collector = DataCollector::new();
        let result = collector.resolve_main_repo_path(&worktree_dir);

        assert!(
            result.is_some(),
            "resolve_main_repo_path should return Some for a worktree"
        );

        let resolved_path = result.unwrap();
        assert!(
            resolved_path.ends_with("main-repo"),
            "Resolved path should end with 'main-repo', got: {:?}",
            resolved_path
        );

        // Cleanup
        worktree.prune(None).ok();
    }

    #[test]
    fn test_get_project_root_respecting_worktree() {
        use git2::{Repository, Signature};

        // Create a main repository
        let temp_dir = TempDir::new().unwrap();
        let main_repo_dir = temp_dir.path().join("main-repo");
        fs::create_dir_all(&main_repo_dir).unwrap();

        // Initialize main repository
        let main_repo = Repository::init(&main_repo_dir).expect("init main repo");
        let file_path = main_repo_dir.join("README.md");
        fs::write(&file_path, "hello").unwrap();

        let mut index = main_repo.index().unwrap();
        index.add_path(Path::new("README.md")).unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = main_repo.find_tree(tree_oid).unwrap();

        let sig = Signature::now("Test Author", "author@example.com").unwrap();
        main_repo
            .commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
            .unwrap();

        // Create a worktree
        let worktree_dir = temp_dir.path().join("worktree-repo");
        let worktree = main_repo
            .worktree("test-worktree", &worktree_dir, None)
            .expect("create worktree");

        // Test: get_project_root_respecting_worktree should return main repo path for worktree
        let collector = DataCollector::new();
        let result = collector.get_project_root_respecting_worktree(&worktree_dir);

        assert!(
            result.ends_with("main-repo"),
            "Project root should be main-repo, got: {:?}",
            result
        );

        // Test: For a normal repo, it should return that repo's workdir
        let normal_result = collector.get_project_root_respecting_worktree(&main_repo_dir);
        assert!(
            normal_result.ends_with("main-repo"),
            "Normal repo should return its own workdir, got: {:?}",
            normal_result
        );

        // Cleanup
        worktree.prune(None).ok();
    }
}
