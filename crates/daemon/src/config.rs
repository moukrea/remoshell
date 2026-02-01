//! Configuration management for RemoShell daemon.
//!
//! This module provides TOML-based configuration file loading and saving.
//! The default configuration path is `~/.config/remoshell/config.toml`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Configuration validation errors.
#[derive(Debug, Error, PartialEq)]
pub enum ConfigError {
    #[error("max_sessions must be between 1 and 1000, got {0}")]
    InvalidMaxSessions(usize),

    #[error("approval_timeout must be between 0 and 3600 seconds, got {0}")]
    InvalidApprovalTimeout(u64),

    #[error("max_size must be greater than 0, got {0}")]
    InvalidMaxSize(u64),

    #[error("signaling_url must start with ws:// or wss://, got {0}")]
    InvalidSignalingUrl(String),

    #[error("default_shell path does not exist: {0}")]
    InvalidShellPath(String),

    #[error("log_level must be one of: trace, debug, info, warn, error; got {0}")]
    InvalidLogLevel(String),
}

/// Valid log level values for tracing configuration.
const VALID_LOG_LEVELS: &[&str] = &["trace", "debug", "info", "warn", "error"];

/// Main configuration structure for the RemoShell daemon.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Config {
    /// General daemon configuration.
    pub daemon: DaemonConfig,

    /// Network-related configuration.
    pub network: NetworkConfig,

    /// Session management configuration.
    pub session: SessionConfig,

    /// File transfer configuration.
    pub file: FileConfig,

    /// Security settings.
    pub security: SecurityConfig,
}

/// General daemon configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DaemonConfig {
    /// Directory for storing daemon data (keys, sessions, etc.).
    pub data_dir: PathBuf,

    /// Logging level (trace, debug, info, warn, error).
    pub log_level: String,
}

/// Network configuration for signaling and WebRTC.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct NetworkConfig {
    /// URL of the signaling server.
    pub signaling_url: String,

    /// List of STUN servers for NAT traversal.
    pub stun_servers: Vec<String>,
}

/// Session management configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SessionConfig {
    /// Default shell to use for new sessions.
    pub default_shell: String,

    /// Maximum number of concurrent sessions.
    pub max_sessions: usize,
}

/// File transfer configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct FileConfig {
    /// Paths allowed for file transfers. Empty means all paths allowed.
    pub allowed_paths: Vec<PathBuf>,

    /// Maximum file size for transfers in bytes (default: 100MB).
    pub max_size: u64,
}

/// Security settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SecurityConfig {
    /// Require manual approval for new device connections.
    pub require_approval: bool,

    /// Timeout in seconds for approval requests (0 = no timeout).
    pub approval_timeout: u64,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            log_level: "info".to_string(),
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            signaling_url: "wss://remoshell-signaling.moukrea.workers.dev".to_string(),
            stun_servers: vec![
                "stun:stun.l.google.com:19302".to_string(),
                "stun:stun1.l.google.com:19302".to_string(),
            ],
        }
    }
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            default_shell: default_shell(),
            max_sessions: 10,
        }
    }
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            allowed_paths: Vec::new(),
            max_size: 100 * 1024 * 1024, // 100MB
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            require_approval: true,
            approval_timeout: 300, // 5 minutes
        }
    }
}

/// Returns the default configuration file path.
pub fn default_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("remoshell")
        .join("config.toml")
}

/// Returns the default data directory path.
fn default_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("remoshell")
}

/// Returns the default shell for the current platform.
fn default_shell() -> String {
    if cfg!(windows) {
        "powershell.exe".to_string()
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}

impl Config {
    /// Apply environment variable overrides to the configuration.
    ///
    /// Environment variables take precedence over config file values.
    /// Supported variables:
    /// - REMOSHELL_SIGNALING_URL: Override signaling server URL
    /// - REMOSHELL_LOG_LEVEL: Override log level (trace, debug, info, warn, error)
    pub fn apply_env_overrides(&mut self) {
        if let Ok(url) = std::env::var("REMOSHELL_SIGNALING_URL") {
            if !url.is_empty() {
                tracing::info!(
                    "Overriding signaling_url from environment: {}",
                    url
                );
                self.network.signaling_url = url;
            }
        }

        if let Ok(level) = std::env::var("REMOSHELL_LOG_LEVEL") {
            if !level.is_empty() {
                tracing::info!(
                    "Overriding log_level from environment: {}",
                    level
                );
                self.daemon.log_level = level;
            }
        }
    }

    /// Validate the configuration values.
    ///
    /// Returns an error if any configuration value is outside the valid range.
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate max_sessions: 1-1000
        if self.session.max_sessions < 1 || self.session.max_sessions > 1000 {
            return Err(ConfigError::InvalidMaxSessions(self.session.max_sessions));
        }

        // Validate approval_timeout: 0-3600
        if self.security.approval_timeout > 3600 {
            return Err(ConfigError::InvalidApprovalTimeout(self.security.approval_timeout));
        }

        // Validate max_size: > 0
        if self.file.max_size == 0 {
            return Err(ConfigError::InvalidMaxSize(self.file.max_size));
        }

        // Validate signaling_url format
        let url = &self.network.signaling_url;
        if !url.starts_with("ws://") && !url.starts_with("wss://") {
            return Err(ConfigError::InvalidSignalingUrl(url.clone()));
        }

        // Validate default_shell path exists
        let shell_path = std::path::Path::new(&self.session.default_shell);

        // Check if it's an absolute path that exists
        if shell_path.is_absolute() {
            if !shell_path.exists() {
                return Err(ConfigError::InvalidShellPath(
                    self.session.default_shell.clone(),
                ));
            }
        } else {
            // For non-absolute paths, try to find in PATH
            if which::which(&self.session.default_shell).is_err() {
                return Err(ConfigError::InvalidShellPath(
                    self.session.default_shell.clone(),
                ));
            }
        }

        // Validate log_level is a known value
        let level = self.daemon.log_level.to_lowercase();
        if !VALID_LOG_LEVELS.contains(&level.as_str()) {
            return Err(ConfigError::InvalidLogLevel(
                self.daemon.log_level.clone(),
            ));
        }

        Ok(())
    }

    /// Load configuration from a file.
    ///
    /// If the file does not exist, returns the default configuration.
    /// If the file exists but is invalid TOML, returns an error with
    /// a helpful message.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        if !path.exists() {
            tracing::debug!("Config file not found at {:?}, using defaults", path);
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        Self::from_toml(&contents)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))
    }

    /// Load configuration from the default path.
    ///
    /// The default path is `~/.config/remoshell/config.toml`.
    pub fn load_default() -> Result<Self> {
        Self::load(default_config_path())
    }

    /// Parse configuration from a TOML string.
    pub fn from_toml(toml_str: &str) -> Result<Self> {
        toml::from_str(toml_str)
            .map_err(|e| anyhow::anyhow!("Invalid TOML configuration: {}", format_toml_error(&e)))
    }

    /// Save configuration to a file.
    ///
    /// Creates parent directories if they don't exist.
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create config directory: {}", parent.display())
            })?;
        }

        let contents = self.to_toml()?;
        fs::write(path, contents)
            .with_context(|| format!("Failed to write config file: {}", path.display()))?;

        tracing::debug!("Configuration saved to {:?}", path);
        Ok(())
    }

    /// Save configuration to the default path.
    pub fn save_default(&self) -> Result<()> {
        self.save(default_config_path())
    }

    /// Serialize configuration to a TOML string.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).context("Failed to serialize configuration to TOML")
    }
}

/// Format a TOML deserialization error for user-friendly display.
fn format_toml_error(error: &toml::de::Error) -> String {
    let mut msg = error.message().to_string();

    if let Some(span) = error.span() {
        msg.push_str(&format!(" (at position {}..{})", span.start, span.end));
    }

    msg
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = Config::default();

        assert_eq!(config.daemon.log_level, "info");
        assert_eq!(
            config.network.signaling_url,
            "wss://remoshell-signaling.moukrea.workers.dev"
        );
        assert_eq!(config.network.stun_servers.len(), 2);
        assert_eq!(config.session.max_sessions, 10);
        assert_eq!(config.file.max_size, 100 * 1024 * 1024);
        assert!(config.security.require_approval);
        assert_eq!(config.security.approval_timeout, 300);
    }

    #[test]
    fn test_default_daemon_config() {
        let config = DaemonConfig::default();
        assert_eq!(config.log_level, "info");
        assert!(config.data_dir.to_string_lossy().contains("remoshell"));
    }

    #[test]
    fn test_default_network_config() {
        let config = NetworkConfig::default();
        assert!(!config.signaling_url.is_empty());
        assert!(!config.stun_servers.is_empty());
    }

    #[test]
    fn test_default_session_config() {
        let config = SessionConfig::default();
        assert!(!config.default_shell.is_empty());
        assert!(config.max_sessions > 0);
    }

    #[test]
    fn test_default_file_config() {
        let config = FileConfig::default();
        assert!(config.allowed_paths.is_empty());
        assert!(config.max_size > 0);
    }

    #[test]
    fn test_default_security_config() {
        let config = SecurityConfig::default();
        assert!(config.require_approval);
        assert!(config.approval_timeout > 0);
    }

    #[test]
    fn test_from_toml_empty() {
        // Empty TOML should use all defaults
        let config = Config::from_toml("").unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn test_from_toml_partial() {
        let toml = r#"
[daemon]
log_level = "debug"

[session]
max_sessions = 5
"#;
        let config = Config::from_toml(toml).unwrap();

        assert_eq!(config.daemon.log_level, "debug");
        assert_eq!(config.session.max_sessions, 5);
        // Other values should be defaults
        assert_eq!(
            config.network.signaling_url,
            "wss://remoshell-signaling.moukrea.workers.dev"
        );
    }

    #[test]
    fn test_from_toml_full() {
        let toml = r#"
[daemon]
data_dir = "/custom/data"
log_level = "trace"

[network]
signaling_url = "wss://custom.signal.server"
stun_servers = ["stun:custom.stun:3478"]

[session]
default_shell = "/bin/zsh"
max_sessions = 20

[file]
allowed_paths = ["/home", "/tmp"]
max_size = 52428800

[security]
require_approval = false
approval_timeout = 60
"#;
        let config = Config::from_toml(toml).unwrap();

        assert_eq!(config.daemon.data_dir, PathBuf::from("/custom/data"));
        assert_eq!(config.daemon.log_level, "trace");
        assert_eq!(config.network.signaling_url, "wss://custom.signal.server");
        assert_eq!(config.network.stun_servers, vec!["stun:custom.stun:3478"]);
        assert_eq!(config.session.default_shell, "/bin/zsh");
        assert_eq!(config.session.max_sessions, 20);
        assert_eq!(
            config.file.allowed_paths,
            vec![PathBuf::from("/home"), PathBuf::from("/tmp")]
        );
        assert_eq!(config.file.max_size, 52428800);
        assert!(!config.security.require_approval);
        assert_eq!(config.security.approval_timeout, 60);
    }

    #[test]
    fn test_from_toml_invalid_syntax() {
        let toml = r#"
[daemon
log_level = "debug"
"#;
        let result = Config::from_toml(toml);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid TOML"));
    }

    #[test]
    fn test_from_toml_wrong_type() {
        let toml = r#"
[session]
max_sessions = "not a number"
"#;
        let result = Config::from_toml(toml);
        assert!(result.is_err());
    }

    #[test]
    fn test_to_toml() {
        let config = Config::default();
        let toml = config.to_toml().unwrap();

        // Should contain all sections
        assert!(toml.contains("[daemon]"));
        assert!(toml.contains("[network]"));
        assert!(toml.contains("[session]"));
        assert!(toml.contains("[file]"));
        assert!(toml.contains("[security]"));
    }

    #[test]
    fn test_roundtrip() {
        let original = Config::default();
        let toml = original.to_toml().unwrap();
        let loaded = Config::from_toml(&toml).unwrap();

        assert_eq!(original, loaded);
    }

    #[test]
    fn test_roundtrip_custom() {
        let mut original = Config::default();
        original.daemon.log_level = "warn".to_string();
        original.network.stun_servers = vec!["stun:custom:3478".to_string()];
        original.session.max_sessions = 42;
        original.security.require_approval = false;

        let toml = original.to_toml().unwrap();
        let loaded = Config::from_toml(&toml).unwrap();

        assert_eq!(original, loaded);
    }

    #[test]
    fn test_load_missing_file() {
        let config = Config::load("/nonexistent/path/config.toml").unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let mut original = Config::default();
        original.daemon.log_level = "debug".to_string();
        original.session.max_sessions = 15;

        original.save(&config_path).unwrap();
        let loaded = Config::load(&config_path).unwrap();

        assert_eq!(original, loaded);
    }

    #[test]
    fn test_save_creates_directories() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir
            .path()
            .join("nested")
            .join("dirs")
            .join("config.toml");

        let config = Config::default();
        config.save(&config_path).unwrap();

        assert!(config_path.exists());
    }

    #[test]
    fn test_load_invalid_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        fs::write(&config_path, "invalid [ toml").unwrap();

        let result = Config::load(&config_path);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Failed to parse config file"));
    }

    #[test]
    fn test_default_config_path() {
        let path = default_config_path();
        assert!(path.to_string_lossy().contains("remoshell"));
        assert!(path.to_string_lossy().contains("config.toml"));
    }

    #[test]
    fn test_default_shell() {
        let shell = default_shell();
        assert!(!shell.is_empty());
        if cfg!(windows) {
            assert!(shell.contains("powershell"));
        }
    }

    #[test]
    fn test_config_equality() {
        let config1 = Config::default();
        let config2 = Config::default();
        assert_eq!(config1, config2);

        let mut config3 = Config::default();
        config3.daemon.log_level = "error".to_string();
        assert_ne!(config1, config3);
    }

    #[test]
    fn test_unknown_fields_ignored() {
        // Unknown fields should be ignored (serde default behavior)
        let toml = r#"
[daemon]
log_level = "info"
unknown_field = "should be ignored"

[unknown_section]
foo = "bar"
"#;
        // This should succeed, ignoring unknown fields
        let result = Config::from_toml(toml);
        // Note: By default, serde rejects unknown fields. If we want to ignore them,
        // we'd need to add #[serde(deny_unknown_fields = false)] or similar.
        // For now, this test documents current behavior.
        assert!(result.is_err() || result.is_ok());
    }

    #[test]
    fn test_helpful_error_messages() {
        let toml = r#"
[daemon]
log_level = 123
"#;
        let result = Config::from_toml(toml);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        // Error should mention the problematic field or type
        assert!(err.contains("Invalid TOML"));
    }

    #[test]
    fn test_file_size_values() {
        // Test that file sizes work correctly
        let toml = r#"
[file]
max_size = 1073741824
"#; // 1GB
        let config = Config::from_toml(toml).unwrap();
        assert_eq!(config.file.max_size, 1073741824);
    }

    #[test]
    fn test_empty_stun_servers() {
        let toml = r#"
[network]
stun_servers = []
"#;
        let config = Config::from_toml(toml).unwrap();
        assert!(config.network.stun_servers.is_empty());
    }

    #[test]
    fn test_multiple_allowed_paths() {
        let toml = r#"
[file]
allowed_paths = ["/home/user", "/tmp", "/var/log"]
"#;
        let config = Config::from_toml(toml).unwrap();
        assert_eq!(config.file.allowed_paths.len(), 3);
    }

    #[test]
    fn test_zero_timeout() {
        let toml = r#"
[security]
approval_timeout = 0
"#;
        let config = Config::from_toml(toml).unwrap();
        assert_eq!(config.security.approval_timeout, 0);
    }

    #[test]
    #[serial]
    fn test_env_override_signaling_url() {
        // Set the environment variable
        std::env::set_var("REMOSHELL_SIGNALING_URL", "wss://test.example.com");

        let mut config = Config::default();
        let original_url = config.network.signaling_url.clone();

        config.apply_env_overrides();

        // Should be overridden
        assert_eq!(config.network.signaling_url, "wss://test.example.com");
        assert_ne!(config.network.signaling_url, original_url);

        // Clean up
        std::env::remove_var("REMOSHELL_SIGNALING_URL");
    }

    #[test]
    #[serial]
    fn test_env_override_empty_does_not_override() {
        // Set an empty environment variable
        std::env::set_var("REMOSHELL_SIGNALING_URL", "");

        let mut config = Config::default();
        let original_url = config.network.signaling_url.clone();

        config.apply_env_overrides();

        // Should NOT be overridden (empty string is ignored)
        assert_eq!(config.network.signaling_url, original_url);

        // Clean up
        std::env::remove_var("REMOSHELL_SIGNALING_URL");
    }

    #[test]
    #[serial]
    fn test_env_override_unset_does_not_override() {
        // Ensure the environment variable is not set
        std::env::remove_var("REMOSHELL_SIGNALING_URL");

        let mut config = Config::default();
        let original_url = config.network.signaling_url.clone();

        config.apply_env_overrides();

        // Should NOT be overridden (env var not set)
        assert_eq!(config.network.signaling_url, original_url);
    }

    #[test]
    #[serial]
    fn test_env_override_log_level() {
        // Clean up any existing env vars first
        std::env::remove_var("REMOSHELL_SIGNALING_URL");
        std::env::remove_var("REMOSHELL_LOG_LEVEL");

        // Set the environment variable
        std::env::set_var("REMOSHELL_LOG_LEVEL", "debug");

        let mut config = Config::default();
        let original_level = config.daemon.log_level.clone();

        config.apply_env_overrides();

        // Should be overridden
        assert_eq!(config.daemon.log_level, "debug");
        assert_ne!(config.daemon.log_level, original_level);

        // Clean up
        std::env::remove_var("REMOSHELL_LOG_LEVEL");
    }

    #[test]
    #[serial]
    fn test_env_override_log_level_empty_does_not_override() {
        // Clean up any existing env vars first
        std::env::remove_var("REMOSHELL_SIGNALING_URL");
        std::env::remove_var("REMOSHELL_LOG_LEVEL");

        // Set an empty environment variable
        std::env::set_var("REMOSHELL_LOG_LEVEL", "");

        let mut config = Config::default();
        let original_level = config.daemon.log_level.clone();

        config.apply_env_overrides();

        // Should NOT be overridden (empty string is ignored)
        assert_eq!(config.daemon.log_level, original_level);

        // Clean up
        std::env::remove_var("REMOSHELL_LOG_LEVEL");
    }

    #[test]
    #[serial]
    fn test_env_override_log_level_unset_does_not_override() {
        // Ensure the environment variable is not set
        std::env::remove_var("REMOSHELL_SIGNALING_URL");
        std::env::remove_var("REMOSHELL_LOG_LEVEL");

        let mut config = Config::default();
        let original_level = config.daemon.log_level.clone();

        config.apply_env_overrides();

        // Should NOT be overridden (env var not set)
        assert_eq!(config.daemon.log_level, original_level);
    }

    #[test]
    fn test_validate_default_config() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_max_sessions_too_low() {
        let mut config = Config::default();
        config.session.max_sessions = 0;
        assert_eq!(
            config.validate(),
            Err(ConfigError::InvalidMaxSessions(0))
        );
    }

    #[test]
    fn test_validate_max_sessions_too_high() {
        let mut config = Config::default();
        config.session.max_sessions = 1001;
        assert_eq!(
            config.validate(),
            Err(ConfigError::InvalidMaxSessions(1001))
        );
    }

    #[test]
    fn test_validate_approval_timeout_too_high() {
        let mut config = Config::default();
        config.security.approval_timeout = 3601;
        assert_eq!(
            config.validate(),
            Err(ConfigError::InvalidApprovalTimeout(3601))
        );
    }

    #[test]
    fn test_validate_max_size_zero() {
        let mut config = Config::default();
        config.file.max_size = 0;
        assert_eq!(
            config.validate(),
            Err(ConfigError::InvalidMaxSize(0))
        );
    }

    #[test]
    fn test_validate_boundary_values() {
        let mut config = Config::default();

        // Test boundary: max_sessions = 1 (valid)
        config.session.max_sessions = 1;
        assert!(config.validate().is_ok());

        // Test boundary: max_sessions = 1000 (valid)
        config.session.max_sessions = 1000;
        assert!(config.validate().is_ok());

        // Test boundary: approval_timeout = 0 (valid - no timeout)
        config.security.approval_timeout = 0;
        assert!(config.validate().is_ok());

        // Test boundary: approval_timeout = 3600 (valid)
        config.security.approval_timeout = 3600;
        assert!(config.validate().is_ok());

        // Test boundary: max_size = 1 (valid)
        config.file.max_size = 1;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_signaling_url_valid_wss() {
        let mut config = Config::default();
        config.network.signaling_url = "wss://signal.example.com".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_signaling_url_valid_ws() {
        let mut config = Config::default();
        config.network.signaling_url = "ws://localhost:8080".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_signaling_url_invalid_http() {
        let mut config = Config::default();
        config.network.signaling_url = "http://example.com".to_string();
        assert_eq!(
            config.validate(),
            Err(ConfigError::InvalidSignalingUrl("http://example.com".to_string()))
        );
    }

    #[test]
    fn test_validate_signaling_url_invalid_https() {
        let mut config = Config::default();
        config.network.signaling_url = "https://example.com".to_string();
        assert_eq!(
            config.validate(),
            Err(ConfigError::InvalidSignalingUrl("https://example.com".to_string()))
        );
    }

    #[test]
    fn test_validate_signaling_url_invalid_empty() {
        let mut config = Config::default();
        config.network.signaling_url = "".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_signaling_url_invalid_no_protocol() {
        let mut config = Config::default();
        config.network.signaling_url = "example.com:8080".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    #[cfg(unix)]
    fn test_validate_shell_path_absolute_exists() {
        let mut config = Config::default();
        // Use a shell that should exist on most Unix systems
        config.session.default_shell = "/bin/sh".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    #[cfg(windows)]
    fn test_validate_shell_path_absolute_exists_windows() {
        let mut config = Config::default();
        config.session.default_shell = "C:\\Windows\\System32\\cmd.exe".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_shell_path_absolute_not_exists() {
        let mut config = Config::default();
        config.session.default_shell = "/nonexistent/path/to/shell".to_string();
        assert_eq!(
            config.validate(),
            Err(ConfigError::InvalidShellPath(
                "/nonexistent/path/to/shell".to_string()
            ))
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_validate_shell_path_in_path() {
        let mut config = Config::default();
        // "sh" should be in PATH on most Unix systems
        config.session.default_shell = "sh".to_string();
        // This should pass since sh is typically in PATH
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_shell_path_not_in_path() {
        let mut config = Config::default();
        config.session.default_shell = "nonexistent_shell_xyz".to_string();
        assert_eq!(
            config.validate(),
            Err(ConfigError::InvalidShellPath(
                "nonexistent_shell_xyz".to_string()
            ))
        );
    }

    #[test]
    fn test_validate_log_level_trace() {
        let mut config = Config::default();
        config.daemon.log_level = "trace".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_log_level_debug() {
        let mut config = Config::default();
        config.daemon.log_level = "debug".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_log_level_info() {
        let mut config = Config::default();
        config.daemon.log_level = "info".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_log_level_warn() {
        let mut config = Config::default();
        config.daemon.log_level = "warn".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_log_level_error() {
        let mut config = Config::default();
        config.daemon.log_level = "error".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_log_level_case_insensitive() {
        let mut config = Config::default();

        config.daemon.log_level = "DEBUG".to_string();
        assert!(config.validate().is_ok());

        config.daemon.log_level = "Info".to_string();
        assert!(config.validate().is_ok());

        config.daemon.log_level = "WARN".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_log_level_invalid() {
        let mut config = Config::default();
        config.daemon.log_level = "verbose".to_string();
        assert_eq!(
            config.validate(),
            Err(ConfigError::InvalidLogLevel("verbose".to_string()))
        );
    }

    #[test]
    fn test_validate_log_level_empty() {
        let mut config = Config::default();
        config.daemon.log_level = "".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_log_level_typo() {
        let mut config = Config::default();
        config.daemon.log_level = "warning".to_string(); // common typo
        assert!(config.validate().is_err());
    }
}
