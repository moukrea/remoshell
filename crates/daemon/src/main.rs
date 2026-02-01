//! RemoShell Daemon
//!
//! Headless service for remote shell connections.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use daemon::config::Config;
use daemon::ipc::{get_daemon_pid, is_daemon_running};
use daemon::orchestrator::{DaemonOrchestrator, OrchestratorEvent, OrchestratorState};
use daemon::ui::qr::{generate_png_qr, generate_terminal_qr, PairingInfo};

/// RemoShell Daemon - headless service for remote shell connections.
#[derive(Parser, Debug)]
#[command(name = "remoshell")]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Path to configuration file
    #[arg(short, long, global = true, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Commands,
}

/// Available commands for the daemon.
#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Start the RemoShell daemon
    Start {
        /// Run with TUI interface
        #[arg(long)]
        tui: bool,

        /// Run in systemd service mode (no TUI, structured logging)
        #[arg(long)]
        systemd: bool,
    },

    /// Stop the running daemon
    Stop,

    /// Show daemon status
    Status,

    /// Manage connected devices
    #[command(subcommand)]
    Devices(DevicesCommands),

    /// Manage active sessions
    #[command(subcommand)]
    Sessions(SessionsCommands),

    /// Generate a pairing code for device authentication
    Pair {
        /// Output format for the pairing code
        #[arg(long, short, value_enum, default_value = "terminal")]
        format: PairFormat,

        /// Output file path for PNG format (defaults to ./pairing-qr.png)
        #[arg(long, short)]
        output: Option<PathBuf>,

        /// Relay/signaling server URL
        #[arg(long, default_value = "wss://remoshell-signaling.moukrea.workers.dev")]
        relay_url: String,

        /// Expiry time in seconds (default: 300 = 5 minutes)
        #[arg(long, default_value = "300")]
        expiry: u64,
    },
}

/// Subcommands for device management.
#[derive(Subcommand, Debug, Clone)]
pub enum DevicesCommands {
    /// List all known devices
    List,

    /// Trust a device by its ID
    Trust {
        /// Device ID to trust
        device_id: String,
    },

    /// Revoke trust for a device
    Revoke {
        /// Device ID to revoke
        device_id: String,
    },
}

/// Subcommands for session management.
#[derive(Subcommand, Debug, Clone)]
pub enum SessionsCommands {
    /// List all active sessions
    List,

    /// Kill an active session
    Kill {
        /// Session ID to kill
        session_id: String,
    },
}

/// Output format for pairing codes.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairFormat {
    /// Display as text in terminal
    Terminal,
    /// Generate a PNG QR code
    Png,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    let filter = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    tracing::info!("RemoShell daemon starting...");

    // Load configuration
    let mut config = if let Some(config_path) = &cli.config {
        tracing::info!("Using config file: {:?}", config_path);
        Config::load(config_path)?
    } else {
        Config::load_default()?
    };

    // Apply environment variable overrides
    config.apply_env_overrides();

    // Validate configuration
    config.validate()?;

    // Handle commands
    match cli.command {
        Commands::Start { tui, systemd } => {
            if tui && systemd {
                anyhow::bail!("Cannot use both --tui and --systemd flags");
            }

            // Check for existing daemon BEFORE starting
            if is_daemon_running() {
                let pid = get_daemon_pid().unwrap_or(0);
                eprintln!("Error: Daemon already running (PID: {})", pid);
                eprintln!();
                eprintln!("To stop the existing daemon, run:");
                eprintln!("  remoshell-daemon stop");
                eprintln!();
                eprintln!("To check daemon status, run:");
                eprintln!("  remoshell-daemon status");
                std::process::exit(1);
            }

            // Create the orchestrator
            let mut orchestrator = DaemonOrchestrator::new(config)?;
            let device_id = orchestrator.device_id_fingerprint();

            if tui {
                tracing::info!("Starting with TUI interface");
                run_with_tui(&mut orchestrator).await?;
            } else if systemd {
                tracing::info!("Starting in systemd mode");
                run_systemd_mode(&mut orchestrator).await?;
            } else {
                tracing::info!("Starting daemon (device ID: {})", device_id);
                run_headless(&mut orchestrator).await?;
            }
        }
        Commands::Stop => {
            tracing::info!("Stopping daemon");
            // Send shutdown signal to running daemon via IPC
            println!("Stop command not yet implemented - use SIGTERM to stop the daemon");
        }
        Commands::Status => {
            tracing::info!("Checking daemon status");
            // Query running daemon status via IPC
            println!("Status command not yet implemented");
        }
        Commands::Devices(cmd) => {
            let trust_store = daemon::TrustStore::with_default_path();
            trust_store.load()?;

            match cmd {
                DevicesCommands::List => {
                    let devices = trust_store.list_devices()?;
                    if devices.is_empty() {
                        println!("No devices registered.");
                    } else {
                        println!("Registered devices:");
                        for device in devices {
                            println!(
                                "  {} - {} ({:?})",
                                device.device_id, device.name, device.trust_level
                            );
                        }
                    }
                }
                DevicesCommands::Trust { device_id } => {
                    // Parse device ID from fingerprint format
                    let did = parse_device_id(&device_id)?;
                    trust_store.set_trust_level(&did, daemon::TrustLevel::Trusted)?;
                    trust_store.save()?;
                    println!("Device {} is now trusted", device_id);
                }
                DevicesCommands::Revoke { device_id } => {
                    let did = parse_device_id(&device_id)?;
                    trust_store.set_trust_level(&did, daemon::TrustLevel::Revoked)?;
                    trust_store.save()?;
                    println!("Device {} has been revoked", device_id);
                }
            }
        }
        Commands::Sessions(cmd) => {
            // Sessions commands require a running daemon
            match cmd {
                SessionsCommands::List => {
                    println!("Sessions command requires a running daemon");
                }
                SessionsCommands::Kill { session_id: _ } => {
                    println!("Sessions kill requires a running daemon");
                }
            }
        }
        Commands::Pair {
            format,
            output,
            relay_url,
            expiry,
        } => {
            tracing::info!("Generating pairing code with format: {:?}", format);

            // Load or generate device identity
            let data_dir = config.daemon.data_dir;
            let identity_path = data_dir.join("identity.key");
            let identity = if identity_path.exists() {
                let bytes = std::fs::read(&identity_path)?;
                if bytes.len() != 32 {
                    anyhow::bail!("Invalid identity file");
                }
                let mut key = [0u8; 32];
                key.copy_from_slice(&bytes);
                protocol::DeviceIdentity::from_secret_key_bytes(&key)
            } else {
                let identity = protocol::DeviceIdentity::generate();
                std::fs::create_dir_all(&data_dir)?;
                std::fs::write(&identity_path, identity.secret_key_bytes())?;
                tracing::info!("Generated new device identity");
                identity
            };

            tracing::info!("Device identity: {}", identity.device_id().fingerprint());

            // Create pairing info
            let pairing_info = PairingInfo::from_identity(&identity, relay_url, Some(expiry));

            match format {
                PairFormat::Terminal => {
                    // Generate and print terminal QR code
                    match generate_terminal_qr(&pairing_info) {
                        Ok(qr) => {
                            println!("\nScan this QR code to pair:\n");
                            println!("{}", qr);
                            println!("Device ID: {}", pairing_info.device_id);
                            println!(
                                "Expires in: {} seconds",
                                pairing_info.seconds_until_expiry()
                            );
                        }
                        Err(e) => {
                            tracing::error!("Failed to generate QR code: {}", e);
                            anyhow::bail!("Failed to generate QR code: {}", e);
                        }
                    }
                }
                PairFormat::Png => {
                    // Determine output path
                    let output_path = output.unwrap_or_else(|| PathBuf::from("pairing-qr.png"));

                    match generate_png_qr(&pairing_info, &output_path) {
                        Ok(()) => {
                            println!("QR code saved to: {}", output_path.display());
                            println!("Device ID: {}", pairing_info.device_id);
                            println!(
                                "Expires in: {} seconds",
                                pairing_info.seconds_until_expiry()
                            );
                        }
                        Err(e) => {
                            tracing::error!("Failed to generate QR code: {}", e);
                            anyhow::bail!("Failed to generate QR code: {}", e);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Parse a device ID from its fingerprint format.
fn parse_device_id(fingerprint: &str) -> anyhow::Result<protocol::DeviceId> {
    // Remove colons and decode hex
    let hex_str: String = fingerprint.chars().filter(|c| *c != ':').collect();

    if hex_str.len() != 32 {
        anyhow::bail!("Invalid device ID format: expected 32 hex chars");
    }

    let bytes = hex::decode(&hex_str)?;
    if bytes.len() != 16 {
        anyhow::bail!("Invalid device ID: expected 16 bytes");
    }

    let mut arr = [0u8; 16];
    arr.copy_from_slice(&bytes);
    Ok(protocol::DeviceId::from_bytes(arr))
}

/// Run the daemon in headless mode.
async fn run_headless(orchestrator: &mut DaemonOrchestrator) -> anyhow::Result<()> {
    // Start the orchestrator
    orchestrator.start().await?;

    // Subscribe to orchestrator events for logging
    let mut events = orchestrator.subscribe();

    // Spawn event logging task
    tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            match event {
                OrchestratorEvent::StateChanged(state) => {
                    tracing::info!("Orchestrator state: {:?}", state);
                }
                OrchestratorEvent::PeerConnected { device_id } => {
                    tracing::info!("Peer connected: {}", device_id);
                }
                OrchestratorEvent::PeerDisconnected { device_id, reason } => {
                    tracing::info!("Peer disconnected: {} ({})", device_id, reason);
                }
                OrchestratorEvent::SignalingStateChanged(state) => {
                    tracing::debug!("Signaling state: {:?}", state);
                }
                OrchestratorEvent::Error { message } => {
                    tracing::error!("Orchestrator error: {}", message);
                }
            }
        }
    });

    // Wait for shutdown signal (SIGTERM or SIGINT)
    wait_for_shutdown_signal().await;
    tracing::info!("Received shutdown signal");

    // Stop the orchestrator
    orchestrator.stop().await?;

    Ok(())
}

/// Wait for a shutdown signal (SIGTERM or SIGINT).
async fn wait_for_shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to register SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("Failed to register SIGINT handler");

    tokio::select! {
        _ = sigterm.recv() => {
            tracing::info!("Received SIGTERM");
        }
        _ = sigint.recv() => {
            tracing::info!("Received SIGINT");
        }
    }
}

/// Run the daemon in systemd mode.
async fn run_systemd_mode(orchestrator: &mut DaemonOrchestrator) -> anyhow::Result<()> {
    // Start the orchestrator
    orchestrator.start().await?;

    // Notify systemd that we're ready
    daemon::notify_ready();

    // Subscribe to orchestrator events
    let mut events = orchestrator.subscribe();

    // Spawn event handling task
    tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            match event {
                OrchestratorEvent::StateChanged(state) => {
                    let status = match state {
                        OrchestratorState::Starting => "Starting",
                        OrchestratorState::Running => "Running",
                        OrchestratorState::ShuttingDown => "Shutting down",
                        OrchestratorState::Stopped => "Stopped",
                    };
                    daemon::notify_status(status);
                }
                OrchestratorEvent::Error { message } => {
                    tracing::error!("Orchestrator error: {}", message);
                }
                _ => {}
            }
        }
    });

    // Wait for shutdown signal (SIGTERM or SIGINT)
    wait_for_shutdown_signal().await;
    tracing::info!("Received shutdown signal");

    // Notify systemd we're stopping
    daemon::notify_stopping();

    // Stop the orchestrator
    orchestrator.stop().await?;

    Ok(())
}

/// Run the daemon with TUI interface.
async fn run_with_tui(orchestrator: &mut DaemonOrchestrator) -> anyhow::Result<()> {
    // Start the orchestrator
    orchestrator.start().await?;

    // Get device ID for display
    let device_id = orchestrator.device_id_fingerprint();

    // Create TUI app
    // Note: TuiApp would need to be adapted to work with the orchestrator
    // For now, we'll use a simplified approach
    tracing::info!("TUI mode - Device ID: {}", device_id);
    tracing::info!("Press Ctrl+C to stop");

    // Wait for shutdown signal (SIGTERM or SIGINT)
    wait_for_shutdown_signal().await;
    tracing::info!("Received shutdown signal");

    // Stop the orchestrator
    orchestrator.stop().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_debug_assert() {
        // Verify the CLI structure is valid
        Cli::command().debug_assert();
    }

    #[test]
    fn test_start_command() {
        let cli = Cli::try_parse_from(["remoshell", "start"]).unwrap();
        match cli.command {
            Commands::Start { tui, systemd } => {
                assert!(!tui);
                assert!(!systemd);
            }
            _ => panic!("Expected Start command"),
        }
    }

    #[test]
    fn test_start_with_tui() {
        let cli = Cli::try_parse_from(["remoshell", "start", "--tui"]).unwrap();
        match cli.command {
            Commands::Start { tui, systemd } => {
                assert!(tui);
                assert!(!systemd);
            }
            _ => panic!("Expected Start command"),
        }
    }

    #[test]
    fn test_start_with_systemd() {
        let cli = Cli::try_parse_from(["remoshell", "start", "--systemd"]).unwrap();
        match cli.command {
            Commands::Start { tui, systemd } => {
                assert!(!tui);
                assert!(systemd);
            }
            _ => panic!("Expected Start command"),
        }
    }

    #[test]
    fn test_stop_command() {
        let cli = Cli::try_parse_from(["remoshell", "stop"]).unwrap();
        assert!(matches!(cli.command, Commands::Stop));
    }

    #[test]
    fn test_status_command() {
        let cli = Cli::try_parse_from(["remoshell", "status"]).unwrap();
        assert!(matches!(cli.command, Commands::Status));
    }

    #[test]
    fn test_devices_list() {
        let cli = Cli::try_parse_from(["remoshell", "devices", "list"]).unwrap();
        match cli.command {
            Commands::Devices(DevicesCommands::List) => {}
            _ => panic!("Expected Devices List command"),
        }
    }

    #[test]
    fn test_devices_trust() {
        let cli = Cli::try_parse_from(["remoshell", "devices", "trust", "device123"]).unwrap();
        match cli.command {
            Commands::Devices(DevicesCommands::Trust { device_id }) => {
                assert_eq!(device_id, "device123");
            }
            _ => panic!("Expected Devices Trust command"),
        }
    }

    #[test]
    fn test_devices_revoke() {
        let cli = Cli::try_parse_from(["remoshell", "devices", "revoke", "device456"]).unwrap();
        match cli.command {
            Commands::Devices(DevicesCommands::Revoke { device_id }) => {
                assert_eq!(device_id, "device456");
            }
            _ => panic!("Expected Devices Revoke command"),
        }
    }

    #[test]
    fn test_sessions_list() {
        let cli = Cli::try_parse_from(["remoshell", "sessions", "list"]).unwrap();
        match cli.command {
            Commands::Sessions(SessionsCommands::List) => {}
            _ => panic!("Expected Sessions List command"),
        }
    }

    #[test]
    fn test_sessions_kill() {
        let cli = Cli::try_parse_from(["remoshell", "sessions", "kill", "session789"]).unwrap();
        match cli.command {
            Commands::Sessions(SessionsCommands::Kill { session_id }) => {
                assert_eq!(session_id, "session789");
            }
            _ => panic!("Expected Sessions Kill command"),
        }
    }

    #[test]
    fn test_pair_default_format() {
        let cli = Cli::try_parse_from(["remoshell", "pair"]).unwrap();
        match cli.command {
            Commands::Pair {
                format,
                output,
                relay_url,
                expiry,
            } => {
                assert_eq!(format, PairFormat::Terminal);
                assert!(output.is_none());
                assert_eq!(relay_url, "wss://remoshell-signaling.moukrea.workers.dev");
                assert_eq!(expiry, 300);
            }
            _ => panic!("Expected Pair command"),
        }
    }

    #[test]
    fn test_pair_terminal_format() {
        let cli = Cli::try_parse_from(["remoshell", "pair", "--format", "terminal"]).unwrap();
        match cli.command {
            Commands::Pair { format, .. } => {
                assert_eq!(format, PairFormat::Terminal);
            }
            _ => panic!("Expected Pair command"),
        }
    }

    #[test]
    fn test_pair_png_format() {
        let cli = Cli::try_parse_from(["remoshell", "pair", "--format", "png"]).unwrap();
        match cli.command {
            Commands::Pair { format, .. } => {
                assert_eq!(format, PairFormat::Png);
            }
            _ => panic!("Expected Pair command"),
        }
    }

    #[test]
    fn test_pair_short_format_flag() {
        let cli = Cli::try_parse_from(["remoshell", "pair", "-f", "png"]).unwrap();
        match cli.command {
            Commands::Pair { format, .. } => {
                assert_eq!(format, PairFormat::Png);
            }
            _ => panic!("Expected Pair command"),
        }
    }

    #[test]
    fn test_pair_with_output() {
        let cli =
            Cli::try_parse_from(["remoshell", "pair", "-f", "png", "--output", "/tmp/qr.png"])
                .unwrap();
        match cli.command {
            Commands::Pair { format, output, .. } => {
                assert_eq!(format, PairFormat::Png);
                assert_eq!(output, Some(PathBuf::from("/tmp/qr.png")));
            }
            _ => panic!("Expected Pair command"),
        }
    }

    #[test]
    fn test_pair_with_relay_url() {
        let cli =
            Cli::try_parse_from(["remoshell", "pair", "--relay-url", "wss://custom.relay.io"])
                .unwrap();
        match cli.command {
            Commands::Pair { relay_url, .. } => {
                assert_eq!(relay_url, "wss://custom.relay.io");
            }
            _ => panic!("Expected Pair command"),
        }
    }

    #[test]
    fn test_pair_with_expiry() {
        let cli = Cli::try_parse_from(["remoshell", "pair", "--expiry", "600"]).unwrap();
        match cli.command {
            Commands::Pair { expiry, .. } => {
                assert_eq!(expiry, 600);
            }
            _ => panic!("Expected Pair command"),
        }
    }

    #[test]
    fn test_global_verbose_flag() {
        let cli = Cli::try_parse_from(["remoshell", "--verbose", "status"]).unwrap();
        assert!(cli.verbose);
    }

    #[test]
    fn test_global_short_verbose_flag() {
        let cli = Cli::try_parse_from(["remoshell", "-v", "status"]).unwrap();
        assert!(cli.verbose);
    }

    #[test]
    fn test_global_config_flag() {
        let cli = Cli::try_parse_from(["remoshell", "--config", "/path/to/config.toml", "status"])
            .unwrap();
        assert_eq!(cli.config, Some(PathBuf::from("/path/to/config.toml")));
    }

    #[test]
    fn test_global_short_config_flag() {
        let cli =
            Cli::try_parse_from(["remoshell", "-c", "/path/to/config.toml", "status"]).unwrap();
        assert_eq!(cli.config, Some(PathBuf::from("/path/to/config.toml")));
    }

    #[test]
    fn test_config_path_resolves() {
        let cli =
            Cli::try_parse_from(["remoshell", "-c", "./relative/path.toml", "status"]).unwrap();
        assert_eq!(cli.config, Some(PathBuf::from("./relative/path.toml")));
    }

    #[test]
    fn test_invalid_command_fails() {
        let result = Cli::try_parse_from(["remoshell", "invalid"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_subcommand_fails() {
        let result = Cli::try_parse_from(["remoshell"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_devices_without_subcommand_fails() {
        let result = Cli::try_parse_from(["remoshell", "devices"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_sessions_without_subcommand_fails() {
        let result = Cli::try_parse_from(["remoshell", "sessions"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_pair_format_fails() {
        let result = Cli::try_parse_from(["remoshell", "pair", "--format", "invalid"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_devices_trust_requires_id() {
        let result = Cli::try_parse_from(["remoshell", "devices", "trust"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_sessions_kill_requires_id() {
        let result = Cli::try_parse_from(["remoshell", "sessions", "kill"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_help_available() {
        let result = Cli::try_parse_from(["remoshell", "--help"]);
        // --help causes an early exit, which is treated as an error by try_parse
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    #[test]
    fn test_start_help_available() {
        let result = Cli::try_parse_from(["remoshell", "start", "--help"]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    #[test]
    fn test_verbose_after_command() {
        // Global flags can also come after the command
        let cli = Cli::try_parse_from(["remoshell", "status", "--verbose"]).unwrap();
        assert!(cli.verbose);
    }

    #[test]
    fn test_config_after_command() {
        let cli = Cli::try_parse_from(["remoshell", "status", "--config", "/etc/remoshell.toml"])
            .unwrap();
        assert_eq!(cli.config, Some(PathBuf::from("/etc/remoshell.toml")));
    }
}
