//! Self-update mechanism for the chronova-cli binary.
//!
//! This module queries the GitHub releases API for the latest published version of
//! `chronova-cli`, compares it to the running binary's version (compiled in via
//! `CARGO_PKG_VERSION`), and — if newer — downloads the platform-specific release
//! archive, extracts the binary, and atomically replaces the running executable.
//!
//! # Release artifact layout
//!
//! Releases on GitHub follow a fixed layout that this module depends on:
//!
//! - Repository: `nx-solutions-ug/chronova-cli`
//! - Tag format: `v.{version}` (note the dot after the `v`)
//! - Asset name: `chronova-cli-{tag}-{target_triple}.{tar.gz|zip}`
//!
//! Example asset URL:
//! `https://github.com/nx-solutions-ug/chronova-cli/releases/download/v.1.2.0/chronova-cli-v.1.2.0-x86_64-unknown-linux-gnu.tar.gz`
//!
//! # Update flow
//!
//! 1. `check_for_update` queries `/repos/.../releases/latest` and returns `Some(UpdateInfo)`
//!    only if the latest version is strictly greater than the running version.
//! 2. `perform_update` downloads the asset to a temp file, extracts it via `tar`,
//!    then atomically replaces the running executable with the extracted binary.
//! 3. `check_and_update` is a convenience that combines both steps.
//!
//! # Atomicity & platform notes
//!
//! - On Unix the running binary's inode can be replaced while it is executing:
//!   we write the new bytes to `<current_exe>.new` and `rename(2)` over the
//!   original. The kernel keeps the old inode alive until the process exits.
//! - On Windows the running executable is locked, so we rename the original to
//!   `<current_exe>.old` first, then move the new binary into place. A leftover
//!   `.old` file from a previous successful update is silently removed.
//!
//! # Errors
//!
//! All fallible operations return [`anyhow::Result`]. The typed
//! [`UpdaterError`] enum captures the failure modes callers may want to
//! pattern-match on (network, IO, version parsing, unsupported platform).

use std::cmp::Ordering;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;
use tokio::process::Command;
use tracing::{debug, info, warn};

/// Current package version, baked in at compile time.
const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// GitHub owner/repo of this project.
const REPO: &str = "nx-solutions-ug/chronova-cli";

/// User-Agent header value for outbound HTTP requests.
const USER_AGENT: &str = concat!("chronova-cli/", env!("CARGO_PKG_VERSION"));

/// Minimal RAII temp directory built on top of `std::env::temp_dir()`.
///
/// `tempfile` is a dev-dependency, so the library cannot pull it in. This
/// shim generates a unique-per-process subdirectory and removes it on drop,
/// which is all `perform_update` needs.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> std::io::Result<Self> {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = env::temp_dir().join(format!("chronova-cli-update-{pid}-{nanos}"));
        fs::create_dir_all(&dir)?;
        Ok(Self { path: dir })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Errors that can occur while checking for or applying an update.
#[derive(Error, Debug)]
pub enum UpdaterError {
    /// Failed to talk to the GitHub API or download a release asset.
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    /// Failed to parse the GitHub release JSON payload.
    #[error("failed to parse release metadata: {0}")]
    Parse(String),

    /// A release tag or version string did not match the expected format.
    #[error("invalid version string: {0}")]
    InvalidVersion(String),

    /// The GitHub release did not contain an asset for the current platform.
    #[error("no release asset available for platform `{0}`")]
    UnsupportedPlatform(String),

    /// An IO / filesystem operation failed (extract, chmod, rename, etc.).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Information about an available update.
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    /// Bare version string, e.g. `"1.2.1"`.
    pub version: String,
    /// Full tag as published on GitHub, e.g. `"v.1.2.1"`.
    pub tag: String,
    /// Direct download URL of the asset for the current platform.
    pub download_url: String,
    /// Filename of the asset on the release page.
    pub asset_name: String,
}

/// Self-update driver.
///
/// Holds the [`reqwest::Client`] used for both the release-metadata query and
/// the asset download. Reuse a single `Updater` instance to benefit from the
/// client's connection pool.
#[derive(Debug, Clone)]
pub struct Updater {
    client: Client,
    repo: String,
    current_version: String,
    target_triple: String,
}

impl Updater {
    /// Build an `Updater` configured for the current host.
    ///
    /// Constructs a fresh `reqwest::Client` (with rustls, since the project's
    /// reqwest dep is pinned to `rustls-tls` only) and pins the User-Agent
    /// header used for both the API and download calls.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client fails to build, or if the current
    /// platform has no mapping in [`target_triple_for_host`].
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self {
            client,
            repo: REPO.to_string(),
            current_version: PKG_VERSION.to_string(),
            target_triple: target_triple_for_host()?,
        })
    }

    /// Build an `Updater` with a pre-configured client. Useful for tests that
    /// point at a mock server.
    pub fn with_client(client: Client) -> Result<Self> {
        Ok(Self {
            client,
            repo: REPO.to_string(),
            current_version: PKG_VERSION.to_string(),
            target_triple: target_triple_for_host()?,
        })
    }

    /// Override the repository (owner/name) used for release lookups. Defaults
    /// to `nx-solutions-ug/chronova-cli`.
    #[must_use]
    pub fn with_repo(mut self, repo: impl Into<String>) -> Self {
        self.repo = repo.into();
        self
    }

    /// Override the version string that is compared against the latest
    /// release. Defaults to `CARGO_PKG_VERSION`.
    #[must_use]
    pub fn with_current_version(mut self, version: impl Into<String>) -> Self {
        self.current_version = version.into();
        self
    }

    /// Read-only access to the configured target triple.
    #[must_use]
    pub fn target_triple(&self) -> &str {
        &self.target_triple
    }

    /// Read-only access to the running binary's version.
    #[must_use]
    pub fn current_version(&self) -> &str {
        &self.current_version
    }

    /// Query GitHub for the latest release and compare against the running
    /// binary's version.
    ///
    /// Returns:
    /// - `Ok(Some(UpdateInfo))` when a strictly newer version is available.
    /// - `Ok(None)` when the running binary is up-to-date, or when the latest
    ///   release carries the same version (pre-release tags, rollback, etc.).
    /// - `Err(_)` on network, parse, or platform errors.
    pub async fn check_for_update(&self) -> Result<Option<UpdateInfo>> {
        let url = format!("https://api.github.com/repos/{}/releases/latest", self.repo);
        debug!(url = %url, "Querying latest release");

        let response = self
            .client
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .context("failed to fetch latest release metadata")?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "GitHub returned status {} for latest release",
                response.status()
            ));
        }

        let release: ReleasePayload = response
            .json()
            .await
            .map_err(|e| UpdaterError::Parse(e.to_string()))?;

        let info = find_asset_for_release(&release, &self.target_triple)?
            .ok_or_else(|| UpdaterError::UnsupportedPlatform(self.target_triple.clone()))?;

        match compare_versions(&self.current_version, &info.version)? {
            Ordering::Less => Ok(Some(info)),
            Ordering::Equal | Ordering::Greater => {
                debug!(
                    current = %self.current_version,
                    latest = %info.version,
                    "Already on the latest version"
                );
                Ok(None)
            }
        }
    }

    /// Download the latest release asset, extract the binary, and replace the
    /// currently-running executable.
    ///
    /// The `UpdateInfo` is normally the value returned by
    /// [`Updater::check_for_update`], but it can also be constructed by hand
    /// for advanced uses.
    pub async fn perform_update(&self, info: &UpdateInfo) -> Result<()> {
        info!(version = %info.version, tag = %info.tag, asset = %info.asset_name, "Downloading update");

        let temp_dir = TempDir::new().context("failed to create temp directory")?;
        let temp_path = temp_dir.path().to_path_buf();

        let archive_path = download_archive(
            &self.client,
            &info.download_url,
            &info.asset_name,
            &temp_path,
        )
        .await?;
        debug!(archive = %archive_path.display(), "Archive downloaded");

        let binary_name = binary_name_for_host();
        let extracted = extract_archive(&archive_path, &temp_path, binary_name).await?;
        debug!(binary = %extracted.display(), "Archive extracted");

        replace_running_binary(&extracted).await?;

        info!(version = %info.version, "Update applied successfully");
        drop(temp_dir);
        Ok(())
    }

    /// Convenience: check, and if a newer version is available, apply it.
    ///
    /// Returns `true` when an update was installed, `false` when the running
    /// binary was already current.
    pub async fn check_and_update(&self) -> Result<bool> {
        match self.check_for_update().await? {
            Some(info) => {
                self.perform_update(&info).await?;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ReleasePayload {
    tag_name: String,
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

/// Strip a leading `v` / `v.` prefix from a tag and return the bare version.
///
/// Examples:
/// - `v.1.2.0` -> `1.2.0`
/// - `v1.2.0`  -> `1.2.0`
/// - `1.2.0`   -> `1.2.0`
pub fn parse_version_from_tag(tag: &str) -> Option<String> {
    let trimmed = tag.trim_start_matches('v');
    let trimmed = trimmed.trim_start_matches('.');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Parse a `major.minor.patch` version string into `(u64, u64, u64)`.
pub fn parse_semver(version: &str) -> Result<(u64, u64, u64)> {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3 {
        return Err(UpdaterError::InvalidVersion(version.to_string()).into());
    }

    let major = parts[0]
        .parse::<u64>()
        .with_context(|| format!("invalid major component in `{version}`"))?;
    let minor = parts[1]
        .parse::<u64>()
        .with_context(|| format!("invalid minor component in `{version}`"))?;
    let patch = parts[2]
        .parse::<u64>()
        .with_context(|| format!("invalid patch component in `{version}`"))?;

    Ok((major, minor, patch))
}

/// Compare two semver strings element-wise.
///
/// Returns `Ordering::Less` when `a < b`, `Greater` when `a > b`, and
/// `Equal` when they parse identically.
pub fn compare_versions(a: &str, b: &str) -> Result<Ordering> {
    let pa = parse_semver(a).with_context(|| format!("failed to parse current version `{a}`"))?;
    let pb = parse_semver(b).with_context(|| format!("failed to parse latest version `{b}`"))?;
    Ok(pa.cmp(&pb))
}

/// Build the Rust-style target triple for the host we are running on.
///
/// Returns an [`UpdaterError::UnsupportedPlatform`] when no mapping is defined
/// for the current `OS`/`ARCH` pair (e.g. `powerpc64-unknown-linux-gnu`).
pub fn target_triple_for_host() -> Result<String> {
    target_triple_for(env::consts::OS, env::consts::ARCH)
}

/// Pure form of [`target_triple_for_host`] used by tests and the constructor.
pub fn target_triple_for(os: &str, arch: &str) -> Result<String> {
    match (os, arch) {
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu".to_string()),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu".to_string()),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin".to_string()),
        ("macos", "aarch64") => Ok("aarch64-apple-darwin".to_string()),
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc".to_string()),
        ("windows", "aarch64") => Ok("aarch64-pc-windows-msvc".to_string()),
        _ => Err(UpdaterError::UnsupportedPlatform(format!("{arch}-{os}")).into()),
    }
}

/// The expected name of the binary inside the release archive on the host
/// platform.
#[cfg(windows)]
fn binary_name_for_host() -> &'static str {
    "chronova-cli.exe"
}
#[cfg(not(windows))]
fn binary_name_for_host() -> &'static str {
    "chronova-cli"
}

/// Pick the release asset whose filename ends with the expected target triple
/// and archive extension. Returns `None` when no asset matches.
fn find_asset_for_release(release: &ReleasePayload, target: &str) -> Result<Option<UpdateInfo>> {
    let version = parse_version_from_tag(&release.tag_name)
        .ok_or_else(|| UpdaterError::InvalidVersion(release.tag_name.clone()))?;

    let asset = release.assets.iter().find(|a| {
        a.name.ends_with(&format!("-{target}.tar.gz"))
            || a.name.ends_with(&format!("-{target}.zip"))
    });

    let Some(asset) = asset else { return Ok(None) };

    Ok(Some(UpdateInfo {
        version,
        tag: release.tag_name.clone(),
        download_url: asset.browser_download_url.clone(),
        asset_name: asset.name.clone(),
    }))
}

async fn download_archive(
    client: &Client,
    url: &str,
    asset_name: &str,
    dest_dir: &Path,
) -> Result<PathBuf> {
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to download {url}"))?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "asset download returned status {} for {url}",
            response.status()
        ));
    }

    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("failed to read response body for {url}"))?;

    let dest = dest_dir.join(asset_name);
    tokio::fs::write(&dest, &bytes)
        .await
        .with_context(|| format!("failed to write archive to {}", dest.display()))?;
    Ok(dest)
}

async fn extract_archive(archive: &Path, dest_dir: &Path, binary_name: &str) -> Result<PathBuf> {
    let mut cmd = Command::new("tar");

    if cfg!(windows) {
        cmd.arg("-xf").arg(archive);
    } else {
        cmd.arg("xzf").arg(archive);
    }
    cmd.arg("-C").arg(dest_dir);

    debug!(?cmd, "Extracting archive");
    let output = cmd
        .output()
        .await
        .with_context(|| format!("failed to spawn tar to extract {}", archive.display()))?;

    if !output.status.success() {
        return Err(anyhow!(
            "tar extraction failed (status {:?}): {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let extracted = dest_dir.join(binary_name);
    if !extracted.exists() {
        return Err(anyhow!(
            "extraction succeeded but {} is missing from {}",
            binary_name,
            dest_dir.display()
        ));
    }
    Ok(extracted)
}

/// Resolve the canonical install path for the chronova-cli binary.
///
/// Prefers `~/.chronova/chronova-cli` when it exists (the standard install location
/// that `~/.local/bin/chronova-cli` and `~/.wakatime/wakatime-cli` symlink to).
/// Falls back to `env::current_exe()` canonicalized when the install path doesn't
/// exist (e.g. running a dev build or a custom install location).
async fn resolve_install_path() -> Result<PathBuf> {
    let install_path = tokio::task::spawn_blocking({
        move || -> Option<PathBuf> {
            let home = dirs::home_dir()?;
            let candidate = home.join(".chronova").join("chronova-cli");
            if candidate.exists() {
                Some(candidate)
            } else {
                None
            }
        }
    })
    .await
    .context("install path check task panicked")?;

    if let Some(path) = install_path {
        debug!(path = %path.display(), "Using canonical install path");
        return Ok(path);
    }

    // Fall back to current_exe for non-standard installs
    let raw_exe = tokio::task::spawn_blocking(env::current_exe)
        .await
        .context("current_exe task panicked")?
        .context("failed to resolve current_exe path")?;
    let canonical = tokio::task::spawn_blocking({
        let p = raw_exe.clone();
        move || fs::canonicalize(&p)
    })
    .await
    .context("canonicalize task panicked")?
    .map_err(UpdaterError::Io);
    Ok(canonical.unwrap_or(raw_exe))
}

async fn replace_running_binary(new_binary: &Path) -> Result<()> {
    let current_exe = resolve_install_path().await?;
    let staging = staging_path_for(&current_exe);
    let backup = backup_path_for(&current_exe);

    tokio::fs::copy(new_binary, &staging)
        .await
        .with_context(|| format!("failed to stage new binary at {}", staging.display()))?;

    tokio::task::spawn_blocking({
        let staging = staging.clone();
        move || -> Result<()> { set_executable_bit(&staging) }
    })
    .await
    .context("chmod task panicked")??;

    let rename_result = tokio::task::spawn_blocking({
        let current_exe = current_exe.clone();
        let staging = staging.clone();
        let backup = backup.clone();
        move || -> Result<()> { atomic_swap(&current_exe, &staging, &backup) }
    })
    .await
    .context("rename task panicked")?;

    if let Err(e) = rename_result {
        let _ = tokio::fs::remove_file(&staging).await;
        return Err(e);
    }

    info!(path = %current_exe.display(), "Replaced running binary");
    Ok(())
}

/// Compute the staging path used while writing the new binary in place.
fn staging_path_for(current_exe: &Path) -> PathBuf {
    let mut s = current_exe.as_os_str().to_owned();
    s.push(".new");
    PathBuf::from(s)
}

/// Compute the backup path used on Windows to free the original name.
fn backup_path_for(current_exe: &Path) -> PathBuf {
    let mut s = current_exe.as_os_str().to_owned();
    s.push(".old");
    PathBuf::from(s)
}

/// Set the executable bit on Unix. No-op on Windows.
#[cfg(unix)]
fn set_executable_bit(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata =
        fs::metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    let mut perms = metadata.permissions();
    perms.set_mode(perms.mode() | 0o755);
    fs::set_permissions(path, perms)
        .with_context(|| format!("failed to chmod {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable_bit(_path: &Path) -> Result<()> {
    Ok(())
}

/// Perform the final atomic swap of the staged binary onto the running path.
///
/// - On Unix this is a single `rename(staging -> current_exe)` call. The
///   kernel keeps the running process's inode alive until exit.
/// - On Windows we first remove any leftover `.old`, then rename
///   `current_exe -> .old`, then rename `staging -> current_exe`.
fn atomic_swap(current_exe: &Path, staging: &Path, backup: &Path) -> Result<()> {
    if cfg!(windows) {
        if backup.exists() {
            fs::remove_file(backup).with_context(|| {
                format!("failed to remove leftover backup {}", backup.display())
            })?;
        }
        fs::rename(current_exe, backup).with_context(|| {
            format!(
                "failed to rename {} to {}",
                current_exe.display(),
                backup.display()
            )
        })?;
        fs::rename(staging, current_exe).with_context(|| {
            format!(
                "failed to rename staged {} to {}",
                staging.display(),
                current_exe.display()
            )
        })?;
        if let Err(e) = fs::remove_file(backup) {
            warn!(
                path = %backup.display(),
                error = %e,
                "failed to remove .old after successful update"
            );
        }
    } else {
        let staging_str = staging.to_string_lossy().to_string();
        if staging != current_exe && Path::new(&staging_str).exists() {
            fs::rename(staging, current_exe).with_context(|| {
                format!(
                    "failed to rename {} to {}",
                    staging.display(),
                    current_exe.display()
                )
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_from_tag() {
        assert_eq!(parse_version_from_tag("v.1.2.0"), Some("1.2.0".to_string()));
        assert_eq!(parse_version_from_tag("v1.2.0"), Some("1.2.0".to_string()));
        assert_eq!(parse_version_from_tag("1.2.0"), Some("1.2.0".to_string()));
        assert_eq!(
            parse_version_from_tag("v.10.20.30"),
            Some("10.20.30".to_string())
        );
        assert_eq!(parse_version_from_tag(""), None);
        assert_eq!(parse_version_from_tag("v"), None);
        assert_eq!(parse_version_from_tag("v."), None);
    }

    #[test]
    fn test_version_comparison() {
        assert_eq!(compare_versions("1.2.0", "1.2.1").unwrap(), Ordering::Less);
        assert_eq!(compare_versions("1.2.0", "1.3.0").unwrap(), Ordering::Less);
        assert_eq!(compare_versions("1.2.0", "2.0.0").unwrap(), Ordering::Less);

        assert_eq!(
            compare_versions("1.2.1", "1.2.0").unwrap(),
            Ordering::Greater
        );
        assert_eq!(
            compare_versions("2.0.0", "1.9.9").unwrap(),
            Ordering::Greater
        );

        assert_eq!(compare_versions("1.2.0", "1.2.0").unwrap(), Ordering::Equal);

        assert_eq!(compare_versions("1.2.9", "1.2.10").unwrap(), Ordering::Less);
    }

    #[test]
    fn test_compare_versions_rejects_invalid() {
        assert!(compare_versions("not-a-version", "1.0.0").is_err());
        assert!(compare_versions("1.0.0", "1.0").is_err());
        assert!(compare_versions("1.0.0", "1.0.0.0").is_err());
        assert!(parse_semver("1.2").is_err());
        assert!(parse_semver("abc.def.ghi").is_err());
    }

    #[test]
    fn test_platform_target_triple() {
        assert_eq!(
            target_triple_for("linux", "x86_64").unwrap(),
            "x86_64-unknown-linux-gnu"
        );
        assert_eq!(
            target_triple_for("linux", "aarch64").unwrap(),
            "aarch64-unknown-linux-gnu"
        );
        assert_eq!(
            target_triple_for("macos", "x86_64").unwrap(),
            "x86_64-apple-darwin"
        );
        assert_eq!(
            target_triple_for("macos", "aarch64").unwrap(),
            "aarch64-apple-darwin"
        );
        assert_eq!(
            target_triple_for("windows", "x86_64").unwrap(),
            "x86_64-pc-windows-msvc"
        );
        assert_eq!(
            target_triple_for("windows", "aarch64").unwrap(),
            "aarch64-pc-windows-msvc"
        );

        let err = target_triple_for("freebsd", "x86_64").unwrap_err();
        assert!(matches!(
            err.downcast_ref::<UpdaterError>(),
            Some(UpdaterError::UnsupportedPlatform(_))
        ));
    }

    fn make_release(tag: &str, assets: &[(&str, &str)]) -> ReleasePayload {
        ReleasePayload {
            tag_name: tag.to_string(),
            assets: assets
                .iter()
                .map(|(name, url)| ReleaseAsset {
                    name: (*name).to_string(),
                    browser_download_url: (*url).to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn test_find_asset_for_release_picks_correct_target() {
        let release = make_release(
            "v.1.2.0",
            &[
                (
                    "chronova-cli-v.1.2.0-x86_64-unknown-linux-gnu.tar.gz",
                    "https://example.com/linux",
                ),
                (
                    "chronova-cli-v.1.2.0-aarch64-apple-darwin.tar.gz",
                    "https://example.com/mac",
                ),
                (
                    "chronova-cli-v.1.2.0-x86_64-pc-windows-msvc.zip",
                    "https://example.com/win",
                ),
                ("checksums.txt", "https://example.com/sums"),
            ],
        );

        let linux_info = find_asset_for_release(&release, "x86_64-unknown-linux-gnu")
            .unwrap()
            .unwrap();
        assert_eq!(linux_info.version, "1.2.0");
        assert_eq!(linux_info.tag, "v.1.2.0");
        assert_eq!(linux_info.download_url, "https://example.com/linux");
        assert_eq!(
            linux_info.asset_name,
            "chronova-cli-v.1.2.0-x86_64-unknown-linux-gnu.tar.gz"
        );

        let mac_info = find_asset_for_release(&release, "aarch64-apple-darwin")
            .unwrap()
            .unwrap();
        assert_eq!(mac_info.download_url, "https://example.com/mac");

        let win_info = find_asset_for_release(&release, "x86_64-pc-windows-msvc")
            .unwrap()
            .unwrap();
        assert_eq!(win_info.download_url, "https://example.com/win");

        let missing = find_asset_for_release(&release, "powerpc64-unknown-linux-gnu").unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn test_find_asset_for_release_rejects_bad_tag() {
        let release = make_release("v.", &[]);
        let err = find_asset_for_release(&release, "x86_64-unknown-linux-gnu").unwrap_err();
        assert!(matches!(
            err.downcast_ref::<UpdaterError>(),
            Some(UpdaterError::InvalidVersion(_))
        ));
    }

    #[test]
    fn test_atomic_swap_replaces_running_binary() {
        let tmp = tempfile::tempdir().unwrap();
        let current = tmp.path().join("chronova-cli");
        let staging = tmp.path().join("chronova-cli.new");
        let backup = tmp.path().join("chronova-cli.old");

        fs::write(&current, b"old").unwrap();
        fs::write(&staging, b"new").unwrap();

        atomic_swap(&current, &staging, &backup).unwrap();

        let now = fs::read(&current).unwrap();
        assert_eq!(now, b"new");

        if cfg!(windows) {
            assert!(!backup.exists());
        } else {
            assert!(!staging.exists());
            assert!(!backup.exists());
        }
    }

    #[test]
    fn test_staging_and_backup_paths() {
        let exe = Path::new("/usr/local/bin/chronova-cli");
        assert_eq!(
            staging_path_for(exe),
            PathBuf::from("/usr/local/bin/chronova-cli.new")
        );
        assert_eq!(
            backup_path_for(exe),
            PathBuf::from("/usr/local/bin/chronova-cli.old")
        );
    }
}
