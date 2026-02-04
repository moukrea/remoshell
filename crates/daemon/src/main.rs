//! RemoShell Daemon
//!
//! Headless service for remote shell connections.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use daemon::config::{Config, DEFAULT_SIGNALING_URL};
use daemon::ipc::{get_daemon_pid, get_socket_path, is_daemon_running, IpcClient, IpcResponse};
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
    Stop {
        /// Force immediate termination (SIGKILL)
        #[arg(long, short)]
        force: bool,

        /// Timeout in seconds for graceful shutdown (default: 30)
        #[arg(long, default_value = "30")]
        timeout: u64,
    },

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
        #[arg(long, default_value = DEFAULT_SIGNALING_URL)]
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
    List {
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Kill an active session
    Kill {
        /// Session ID to kill
        session_id: String,

        /// Signal to send (default: SIGTERM)
        /// Common values: SIGTERM (15), SIGKILL (9), SIGHUP (1)
        #[arg(long, short, default_value = "SIGTERM")]
        signal: String,

        /// Force kill (equivalent to --signal SIGKILL)
        #[arg(long, short)]
        force: bool,
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
        Commands::Stop { force, timeout } => {
            tracing::info!("Stopping daemon (force: {})", force);

            if force {
                // Force stop using SIGKILL
                match force_stop_daemon() {
                    Ok(()) => {
                        println!("Daemon forcefully terminated");
                        std::process::exit(0);
                    }
                    Err(e) => {
                        eprintln!("Failed to stop daemon: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                // Graceful shutdown via IPC
                match graceful_stop_daemon(timeout).await {
                    Ok(()) => {
                        println!("Daemon stopped successfully");
                        std::process::exit(0);
                    }
                    Err(e) => {
                        eprintln!("Failed to stop daemon: {}", e);
                        eprintln!("Try: remoshell-daemon stop --force");
                        std::process::exit(1);
                    }
                }
            }
        }
        Commands::Status => {
            tracing::info!("Checking daemon status");

            match query_daemon_status().await {
                Ok(status) => {
                    println!(
                        "Daemon Status: {}",
                        if status.running { "running" } else { "stopped" }
                    );
                    println!("  Uptime:   {}", format_duration(status.uptime_secs));
                    println!("  Sessions: {}", status.session_count);
                    println!("  Devices:  {}", status.device_count);
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("Daemon is not running: {}", e);
                    std::process::exit(1);
                }
            }
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
                SessionsCommands::List { json } => match query_sessions_list().await {
                    Ok(sessions) => {
                        if json {
                            println!("{}", serde_json::to_string_pretty(&sessions).unwrap());
                        } else {
                            print_sessions_table(&sessions);
                        }
                        std::process::exit(0);
                    }
                    Err(e) => {
                        eprintln!("Failed to list sessions: {}", e);
                        std::process::exit(1);
                    }
                },
                SessionsCommands::Kill {
                    session_id,
                    signal,
                    force,
                } => {
                    // Determine the signal to send
                    let signal_to_send = if force { "SIGKILL".to_string() } else { signal };

                    // Parse the signal
                    let signal_num = match parse_signal(&signal_to_send) {
                        Ok(num) => num,
                        Err(e) => {
                            eprintln!("Invalid signal: {}", e);
                            std::process::exit(1);
                        }
                    };

                    match kill_session(&session_id, signal_num).await {
                        Ok(()) => {
                            println!(
                                "Session {} terminated with {} ({})",
                                session_id, signal_to_send, signal_num
                            );
                            std::process::exit(0);
                        }
                        Err(e) => {
                            eprintln!("Failed to kill session {}: {}", session_id, e);
                            std::process::exit(1);
                        }
                    }
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

/// Parse a signal string into a signal number.
///
/// Accepts:
/// - Numeric signals: "9", "15"
/// - Named signals: "SIGTERM", "SIGKILL", "TERM", "KILL"
fn parse_signal(signal_str: &str) -> anyhow::Result<i32> {
    // Handle numeric signals
    if let Ok(num) = signal_str.parse::<i32>() {
        if num > 0 && num < 32 {
            return Ok(num);
        }
        anyhow::bail!("Invalid signal number: {} (must be 1-31)", num);
    }

    // Handle named signals (with or without SIG prefix)
    let name = signal_str.to_uppercase();
    let name = name.strip_prefix("SIG").unwrap_or(&name);

    match name {
        "HUP" => Ok(1),
        "INT" => Ok(2),
        "QUIT" => Ok(3),
        "KILL" => Ok(9),
        "TERM" => Ok(15),
        "USR1" => Ok(10),
        "USR2" => Ok(12),
        _ => anyhow::bail!("Unknown signal: {}", signal_str),
    }
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

/// Status information returned from the daemon.
struct DaemonStatus {
    running: bool,
    uptime_secs: u64,
    session_count: usize,
    device_count: usize,
}

/// Query the daemon status via IPC.
async fn query_daemon_status() -> anyhow::Result<DaemonStatus> {
    use std::time::Duration;

    let socket_path = get_socket_path();

    // Connect with timeout
    let mut client = IpcClient::connect_with_timeout(&socket_path, Duration::from_secs(5))
        .await
        .map_err(|e| anyhow::anyhow!("Cannot connect to daemon: {}", e))?;

    // Send status request
    let response = client
        .status()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query status: {}", e))?;

    match response {
        IpcResponse::Status {
            running,
            uptime_secs,
            session_count,
            device_count,
        } => Ok(DaemonStatus {
            running,
            uptime_secs,
            session_count,
            device_count,
        }),
        IpcResponse::Error { message } => {
            anyhow::bail!("Daemon returned error: {}", message)
        }
        _ => anyhow::bail!("Unexpected response from daemon"),
    }
}

/// Query the list of active sessions from the daemon.
async fn query_sessions_list() -> anyhow::Result<Vec<daemon::ipc::IpcSessionInfo>> {
    use std::time::Duration;

    let socket_path = get_socket_path();

    // Connect with timeout
    let mut client = IpcClient::connect_with_timeout(&socket_path, Duration::from_secs(5))
        .await
        .map_err(|_| anyhow::anyhow!("Daemon is not running (cannot connect to socket)"))?;

    // Send list sessions request
    let response = client
        .list_sessions()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query sessions: {}", e))?;

    match response {
        IpcResponse::Sessions { sessions } => Ok(sessions),
        IpcResponse::Error { message } => {
            anyhow::bail!("Daemon returned error: {}", message)
        }
        _ => anyhow::bail!("Unexpected response from daemon"),
    }
}

/// Kill a specific session by ID via IPC.
///
/// # Arguments
///
/// * `session_id` - The ID of the session to kill.
/// * `signal` - The signal number to send.
async fn kill_session(session_id: &str, signal: i32) -> anyhow::Result<()> {
    use std::time::Duration;

    let socket_path = get_socket_path();

    // Connect with timeout
    let mut client = IpcClient::connect_with_timeout(&socket_path, Duration::from_secs(5))
        .await
        .map_err(|_| anyhow::anyhow!("Daemon is not running (cannot connect to socket)"))?;

    // Send kill session request
    let response = client
        .kill_session(session_id.to_string(), Some(signal))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to send kill request: {}", e))?;

    match response {
        IpcResponse::SessionKilled {
            session_id: killed_id,
        } => {
            if killed_id == session_id {
                Ok(())
            } else {
                anyhow::bail!("Unexpected session killed: {}", killed_id)
            }
        }
        IpcResponse::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response from daemon"),
    }
}

/// Print sessions in a formatted ASCII table.
fn print_sessions_table(sessions: &[daemon::ipc::IpcSessionInfo]) {
    if sessions.is_empty() {
        println!("No active sessions.");
        return;
    }

    // Calculate column widths
    let id_width = sessions
        .iter()
        .map(|s| s.id.len())
        .max()
        .unwrap_or(8)
        .max(8);
    let peer_width = sessions
        .iter()
        .map(|s| s.peer_id.as_ref().map(|p| p.len()).unwrap_or(4))
        .max()
        .unwrap_or(7)
        .max(7);

    // Print header
    println!(
        "{:<id_width$}  {:<peer_width$}  {:>12}",
        "ID",
        "PEER ID",
        "CONNECTED",
        id_width = id_width,
        peer_width = peer_width
    );
    println!("{}", "-".repeat(id_width + peer_width + 16));

    // Print rows
    for session in sessions {
        let peer_id = session.peer_id.as_deref().unwrap_or("-");
        let connected = format_relative_time(session.connected_at);

        println!(
            "{:<id_width$}  {:<peer_width$}  {:>12}",
            truncate_str(&session.id, id_width),
            truncate_str(peer_id, peer_width),
            connected,
            id_width = id_width,
            peer_width = peer_width
        );
    }

    println!();
    println!("Total: {} session(s)", sessions.len());
}

/// Format a Unix timestamp as relative time (e.g., "5m ago").
fn format_relative_time(timestamp: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let elapsed = now.saturating_sub(timestamp);

    if elapsed < 60 {
        format!("{}s ago", elapsed)
    } else if elapsed < 3600 {
        format!("{}m ago", elapsed / 60)
    } else if elapsed < 86400 {
        format!("{}h ago", elapsed / 3600)
    } else {
        format!("{}d ago", elapsed / 86400)
    }
}

/// Truncate a string to a maximum length, adding "..." if truncated.
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Gracefully stop the daemon via IPC.
///
/// Sends a shutdown request to the daemon and waits for acknowledgment.
async fn graceful_stop_daemon(timeout_secs: u64) -> anyhow::Result<()> {
    use daemon::ipc::get_pid_file_path;
    use std::time::Duration;

    let socket_path = get_socket_path();

    // Connect to daemon
    let mut client = IpcClient::connect_with_timeout(&socket_path, Duration::from_secs(5))
        .await
        .map_err(|_| anyhow::anyhow!("Daemon is not running (cannot connect to socket)"))?;

    println!("Sending shutdown request...");

    // Send shutdown request with custom timeout
    client.set_timeout(Duration::from_secs(timeout_secs));
    let response = client
        .stop()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to send stop request: {}", e))?;

    match response {
        IpcResponse::Stopping => {
            println!("Shutdown acknowledged, waiting for daemon to exit...");
        }
        IpcResponse::Error { message } => {
            anyhow::bail!("Daemon returned error: {}", message);
        }
        _ => {
            anyhow::bail!("Unexpected response from daemon");
        }
    }

    // Wait for daemon to actually exit by polling the socket
    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(timeout_secs);

    while start.elapsed() < timeout {
        // Check if socket is gone (daemon exited)
        if !socket_path.exists() {
            return Ok(());
        }

        // Try to connect - if it fails, daemon is shutting down
        if IpcClient::connect_with_timeout(&socket_path, Duration::from_millis(100))
            .await
            .is_err()
        {
            // Clean up stale PID file if it exists
            let pid_path = get_pid_file_path();
            let _ = std::fs::remove_file(&pid_path);
            return Ok(());
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    Err(anyhow::anyhow!(
        "Timeout waiting for daemon to exit ({}s)",
        timeout_secs
    ))
}

/// Force stop the daemon using SIGKILL.
///
/// Reads the daemon PID from the PID file and sends SIGKILL.
fn force_stop_daemon() -> anyhow::Result<()> {
    use daemon::ipc::get_pid_file_path;
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    let pid_path = get_pid_file_path();

    if !pid_path.exists() {
        return Err(anyhow::anyhow!(
            "Daemon PID file not found - is the daemon running?"
        ));
    }

    let pid_str = std::fs::read_to_string(&pid_path)
        .map_err(|e| anyhow::anyhow!("Failed to read PID file: {}", e))?;
    let pid: i32 = pid_str
        .trim()
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid PID in file: {}", e))?;

    // Send SIGKILL
    kill(Pid::from_raw(pid), Signal::SIGKILL)
        .map_err(|e| anyhow::anyhow!("Failed to kill daemon (PID {}): {}", pid, e))?;

    println!("Sent SIGKILL to daemon (PID {})", pid);

    // Clean up PID file
    let _ = std::fs::remove_file(&pid_path);

    // Clean up socket file
    let socket_path = get_socket_path();
    let _ = std::fs::remove_file(&socket_path);

    Ok(())
}

/// Format a duration in seconds to human-readable format.
fn format_duration(secs: u64) -> String {
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
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
    use daemon::ui::tui::{process_approval_result, TuiApp, TuiEvent};

    // Start the orchestrator
    orchestrator.start().await?;

    // Create TUI app
    let (mut tui_app, tui_tx) = TuiApp::new()?;

    // Take the approval receiver to handle approval results
    let mut approval_rx = tui_app
        .take_approval_receiver()
        .expect("Approval receiver should be available");

    // Get device ID for display
    let device_id = orchestrator.device_id_fingerprint();
    tracing::info!("TUI mode started - Device ID: {}", device_id);

    // Clone trust store for approval handling
    let trust_store = orchestrator.trust_store().clone();

    // Spawn task to process approval results
    let approval_trust_store = trust_store.clone();
    let approval_handle = tokio::spawn(async move {
        while let Some(result) = approval_rx.recv().await {
            if let Err(e) = process_approval_result(&result, &approval_trust_store) {
                tracing::error!("Failed to process approval result: {}", e);
            }
        }
    });

    // Spawn task to forward orchestrator events to TUI
    // This would require adding an event subscription mechanism to the orchestrator
    // For now, we'll use a periodic stats update
    let stats_tx = tui_tx.clone();
    let stats_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            interval.tick().await;
            // Send refresh event to update stats display
            if stats_tx.send(TuiEvent::Refresh).await.is_err() {
                break;
            }
        }
    });

    // Run the TUI event loop
    let result = tui_app.run().await;

    // Cleanup
    stats_handle.abort();
    approval_handle.abort();

    // Restore terminal
    tui_app.restore()?;

    // Stop the orchestrator
    orchestrator.stop().await?;

    result.map_err(|e| anyhow::anyhow!("TUI error: {}", e))
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
        match cli.command {
            Commands::Stop { force, timeout } => {
                assert!(!force);
                assert_eq!(timeout, 30);
            }
            _ => panic!("Expected Stop command"),
        }
    }

    #[test]
    fn test_stop_with_force() {
        let cli = Cli::try_parse_from(["remoshell", "stop", "--force"]).unwrap();
        match cli.command {
            Commands::Stop { force, timeout } => {
                assert!(force);
                assert_eq!(timeout, 30);
            }
            _ => panic!("Expected Stop command"),
        }
    }

    #[test]
    fn test_stop_with_short_force() {
        let cli = Cli::try_parse_from(["remoshell", "stop", "-f"]).unwrap();
        match cli.command {
            Commands::Stop { force, timeout } => {
                assert!(force);
                assert_eq!(timeout, 30);
            }
            _ => panic!("Expected Stop command"),
        }
    }

    #[test]
    fn test_stop_with_timeout() {
        let cli = Cli::try_parse_from(["remoshell", "stop", "--timeout", "60"]).unwrap();
        match cli.command {
            Commands::Stop { force, timeout } => {
                assert!(!force);
                assert_eq!(timeout, 60);
            }
            _ => panic!("Expected Stop command"),
        }
    }

    #[test]
    fn test_stop_with_force_and_timeout() {
        let cli = Cli::try_parse_from(["remoshell", "stop", "--force", "--timeout", "10"]).unwrap();
        match cli.command {
            Commands::Stop { force, timeout } => {
                assert!(force);
                assert_eq!(timeout, 10);
            }
            _ => panic!("Expected Stop command"),
        }
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
            Commands::Sessions(SessionsCommands::List { json }) => {
                assert!(!json);
            }
            _ => panic!("Expected Sessions List command"),
        }
    }

    #[test]
    fn test_sessions_list_json() {
        let cli = Cli::try_parse_from(["remoshell", "sessions", "list", "--json"]).unwrap();
        match cli.command {
            Commands::Sessions(SessionsCommands::List { json }) => {
                assert!(json);
            }
            _ => panic!("Expected Sessions List command"),
        }
    }

    #[test]
    fn test_sessions_kill() {
        let cli = Cli::try_parse_from(["remoshell", "sessions", "kill", "session789"]).unwrap();
        match cli.command {
            Commands::Sessions(SessionsCommands::Kill {
                session_id,
                signal,
                force,
            }) => {
                assert_eq!(session_id, "session789");
                assert_eq!(signal, "SIGTERM");
                assert!(!force);
            }
            _ => panic!("Expected Sessions Kill command"),
        }
    }

    #[test]
    fn test_sessions_kill_with_signal() {
        let cli = Cli::try_parse_from([
            "remoshell",
            "sessions",
            "kill",
            "session123",
            "--signal",
            "SIGKILL",
        ])
        .unwrap();
        match cli.command {
            Commands::Sessions(SessionsCommands::Kill {
                session_id,
                signal,
                force,
            }) => {
                assert_eq!(session_id, "session123");
                assert_eq!(signal, "SIGKILL");
                assert!(!force);
            }
            _ => panic!("Expected Sessions Kill command"),
        }
    }

    #[test]
    fn test_sessions_kill_with_signal_number() {
        let cli = Cli::try_parse_from([
            "remoshell",
            "sessions",
            "kill",
            "session123",
            "--signal",
            "9",
        ])
        .unwrap();
        match cli.command {
            Commands::Sessions(SessionsCommands::Kill {
                session_id,
                signal,
                force,
            }) => {
                assert_eq!(session_id, "session123");
                assert_eq!(signal, "9");
                assert!(!force);
            }
            _ => panic!("Expected Sessions Kill command"),
        }
    }

    #[test]
    fn test_sessions_kill_with_short_signal() {
        let cli = Cli::try_parse_from([
            "remoshell",
            "sessions",
            "kill",
            "session123",
            "-s",
            "SIGHUP",
        ])
        .unwrap();
        match cli.command {
            Commands::Sessions(SessionsCommands::Kill {
                session_id,
                signal,
                force,
            }) => {
                assert_eq!(session_id, "session123");
                assert_eq!(signal, "SIGHUP");
                assert!(!force);
            }
            _ => panic!("Expected Sessions Kill command"),
        }
    }

    #[test]
    fn test_sessions_kill_with_force() {
        let cli = Cli::try_parse_from(["remoshell", "sessions", "kill", "session123", "--force"])
            .unwrap();
        match cli.command {
            Commands::Sessions(SessionsCommands::Kill {
                session_id,
                signal: _,
                force,
            }) => {
                assert_eq!(session_id, "session123");
                assert!(force);
            }
            _ => panic!("Expected Sessions Kill command"),
        }
    }

    #[test]
    fn test_sessions_kill_with_short_force() {
        let cli =
            Cli::try_parse_from(["remoshell", "sessions", "kill", "session123", "-f"]).unwrap();
        match cli.command {
            Commands::Sessions(SessionsCommands::Kill {
                session_id,
                signal: _,
                force,
            }) => {
                assert_eq!(session_id, "session123");
                assert!(force);
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
                assert_eq!(relay_url, DEFAULT_SIGNALING_URL);
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

    #[test]
    fn test_parse_signal_named() {
        assert_eq!(parse_signal("SIGTERM").unwrap(), 15);
        assert_eq!(parse_signal("SIGKILL").unwrap(), 9);
        assert_eq!(parse_signal("SIGHUP").unwrap(), 1);
        assert_eq!(parse_signal("SIGINT").unwrap(), 2);
        assert_eq!(parse_signal("SIGQUIT").unwrap(), 3);
        assert_eq!(parse_signal("SIGUSR1").unwrap(), 10);
        assert_eq!(parse_signal("SIGUSR2").unwrap(), 12);
    }

    #[test]
    fn test_parse_signal_named_without_prefix() {
        assert_eq!(parse_signal("TERM").unwrap(), 15);
        assert_eq!(parse_signal("KILL").unwrap(), 9);
        assert_eq!(parse_signal("HUP").unwrap(), 1);
        assert_eq!(parse_signal("INT").unwrap(), 2);
    }

    #[test]
    fn test_parse_signal_named_lowercase() {
        assert_eq!(parse_signal("sigterm").unwrap(), 15);
        assert_eq!(parse_signal("term").unwrap(), 15);
        assert_eq!(parse_signal("kill").unwrap(), 9);
    }

    #[test]
    fn test_parse_signal_numeric() {
        assert_eq!(parse_signal("15").unwrap(), 15);
        assert_eq!(parse_signal("9").unwrap(), 9);
        assert_eq!(parse_signal("1").unwrap(), 1);
    }

    #[test]
    fn test_parse_signal_invalid() {
        assert!(parse_signal("INVALID").is_err());
        assert!(parse_signal("SIGFOO").is_err());
        assert!(parse_signal("0").is_err());
        assert!(parse_signal("32").is_err());
        assert!(parse_signal("-1").is_err());
    }
}
