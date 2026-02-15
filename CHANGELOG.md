# Changelog

All notable changes to Chronova CLI will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Advanced Offline Heartbeats Support**: Complete offline-first synchronization system
- **Background Sync**: Automatic synchronization every 5 minutes when online
- **Retry Strategy**: Exponential backoff with jitter for failed sync attempts
- **Storage Management**: Automatic cleanup with configurable retention policies
- **Network Connectivity Detection**: Smart connectivity monitoring and caching
- **Performance Metrics**: Detailed observability with structured logging and metrics
- **CLI Commands**: `--sync-offline-activity`, `--offline-count`, `--force-sync`
- **Sync Configuration**: Configurable sync settings in `.chronova.cfg`
- **Error Recovery**: Automatic database corruption detection and recovery
- **Heartbeat Deduplication**: Prevention of duplicate heartbeat submissions
- **Queue Monitoring**: Real-time queue size tracking and utilization warnings

### Changed
- **Enhanced Offline Queue**: Extended database schema with sync status tracking
- **Improved Heartbeat Processing**: Offline-first strategy with automatic fallback
- **Updated API Client**: Added connectivity checking and batch sending capabilities
- **Enhanced Configuration**: Added sync-specific configuration options
- **Better Error Handling**: Comprehensive error types and recovery mechanisms

### Fixed
- URL construction issues for compatibility endpoints
- Configuration file parsing for WakaTime-specific options
- Authentication handling for different API key formats
- Database integrity and corruption handling
- Network failure scenarios and recovery

## [0.1.0] - 2025-11-27

### Added
- Initial release of Chronova CLI
- Basic heartbeat tracking functionality
- Configuration file support
- Offline queue implementation
- Editor plugin compatibility layer
- WakaTime CLI argument compatibility
- Comprehensive test suite
- Documentation for editor integration
- Migration guide from WakaTime

### Features
- High-performance Rust implementation
- Full WakaTime plugin compatibility
- SQLite-based offline queue
- Automatic project, language, and Git branch detection
- Configurable ignore patterns
- Support for all WakaTime configuration options
- Cross-platform support (Windows, macOS, Linux)
- Detailed logging and debugging capabilities

### Compatibility
- Full drop-in replacement for wakatime-cli
- Support for all WakaTime command-line arguments
- Compatible with existing .chronova.cfg configuration files
- Works with all major editor plugins (VS Code, IntelliJ, Vim, Sublime Text, Atom, Emacs)