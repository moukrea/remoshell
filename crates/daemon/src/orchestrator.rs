//! Daemon orchestrator for wiring together all components.
//!
//! This module provides the `DaemonOrchestrator` that initializes and coordinates
//! all daemon subsystems: session management, device trust, message routing,
//! network handlers (WebRTC/QUIC), and signaling.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use protocol::crypto::DeviceIdentity;
use protocol::DeviceId;
use tokio::sync::{broadcast, oneshot, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::devices::TrustStore;
use crate::files::{DirectoryBrowser, FileTransfer, PathPermissions};
use crate::ipc::{get_socket_path, IpcRequest, IpcResponse, IpcServer, IpcSessionInfo};
use crate::network::{
    signaling::{
        ConnectionState, SignalingClient, SignalingConfig, SignalingEvent, WebSocketSignalingClient,
    },
    webrtc::{WebRtcConfig, WebRtcConnectionHandler},
    Connection,
};
use crate::router::MessageRouter;
use crate::session::{SessionManager, SessionManagerImpl};

/// Default cleanup interval for sessions (in seconds).
const SESSION_CLEANUP_INTERVAL_SECS: u64 = 60;

/// Default cleanup interval for expired pending approvals (in seconds).
const APPROVAL_CLEANUP_INTERVAL_SECS: u64 = 60;

/// Daemon orchestrator state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrchestratorState {
    /// Initial state, not started.
    Stopped,
    /// Starting up, initializing components.
    Starting,
    /// Running and accepting connections.
    Running,
    /// Shutting down gracefully.
    ShuttingDown,
}

/// Connection information for an active peer.
pub struct ActiveConnection {
    /// Device ID of the peer.
    pub device_id: String,
    /// The WebRTC connection handler.
    pub handler: WebRtcConnectionHandler,
    /// Whether the Noise handshake is complete.
    pub noise_complete: bool,
}

/// Events emitted by the orchestrator.
#[derive(Debug, Clone)]
pub enum OrchestratorEvent {
    /// Orchestrator state changed.
    StateChanged(OrchestratorState),
    /// A new peer connected.
    PeerConnected { device_id: String },
    /// A peer disconnected.
    PeerDisconnected { device_id: String, reason: String },
    /// Signaling connection state changed.
    SignalingStateChanged(ConnectionState),
    /// Error occurred.
    Error { message: String },
}

/// Daemon orchestrator that manages all subsystems.
pub struct DaemonOrchestrator {
    /// Configuration.
    config: Config,
    /// Device identity for this daemon.
    identity: DeviceIdentity,
    /// Current state.
    state: Arc<RwLock<OrchestratorState>>,
    /// Session manager for PTY sessions.
    session_manager: Arc<SessionManagerImpl>,
    /// Trust store for device management.
    trust_store: Arc<TrustStore>,
    /// Directory browser for file listing.
    directory_browser: Arc<DirectoryBrowser>,
    /// File transfer handler.
    file_transfer: Arc<FileTransfer>,
    /// Message router.
    router: Arc<MessageRouter<SessionManagerImpl>>,
    /// Signaling client.
    signaling_client: Option<Arc<WebSocketSignalingClient>>,
    /// Active connections by device ID.
    connections: Arc<RwLock<std::collections::HashMap<String, ActiveConnection>>>,
    /// Cancellation token for graceful shutdown.
    shutdown_token: CancellationToken,
    /// Event sender.
    event_tx: broadcast::Sender<OrchestratorEvent>,
    /// Start time for uptime tracking.
    start_time: Option<Instant>,
    /// IPC server shutdown sender.
    ipc_shutdown_tx: Option<oneshot::Sender<()>>,
}

impl DaemonOrchestrator {
    /// Creates a new daemon orchestrator.
    pub fn new(config: Config) -> Result<Self> {
        // Load or generate device identity
        let identity_path = config.daemon.data_dir.join("identity.key");
        let identity = Self::load_or_generate_identity(&identity_path)?;

        info!("Daemon identity: {}", identity.device_id().fingerprint());

        // Initialize session manager
        let session_manager = Arc::new(SessionManagerImpl::new());

        // Initialize trust store
        let trust_store_path = config.daemon.data_dir.join("trusted_devices.json");
        let trust_store = Arc::new(TrustStore::new(&trust_store_path));
        trust_store.load().context("Failed to load trust store")?;

        // Initialize directory browser with allowed paths
        let allowed_paths = if config.file.allowed_paths.is_empty() {
            // Default to home directory if no paths specified
            vec![
                dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")),
                PathBuf::from("/tmp"),
            ]
        } else {
            config.file.allowed_paths.clone()
        };
        let directory_browser = Arc::new(DirectoryBrowser::new(allowed_paths.clone()));

        // Initialize file transfer handler
        let browser_for_transfer = DirectoryBrowser::new(allowed_paths.clone());
        let file_transfer = Arc::new(
            FileTransfer::new(browser_for_transfer, config.file.max_size)
                .with_temp_dir(config.daemon.data_dir.join("tmp")),
        );

        // Initialize path permissions
        let permissions_path = config.daemon.data_dir.join("permissions.json");
        let path_permissions = Arc::new(PathPermissions::new(permissions_path, allowed_paths));
        path_permissions
            .load()
            .context("Failed to load path permissions")?;

        // Initialize message router
        let router = Arc::new(MessageRouter::new(
            Arc::clone(&session_manager),
            Arc::clone(&file_transfer),
            Arc::clone(&directory_browser),
            Arc::clone(&trust_store),
            Arc::clone(&path_permissions),
        ));

        let (event_tx, _) = broadcast::channel(256);

        Ok(Self {
            config,
            identity,
            state: Arc::new(RwLock::new(OrchestratorState::Stopped)),
            session_manager,
            trust_store,
            directory_browser,
            file_transfer,
            router,
            signaling_client: None,
            connections: Arc::new(RwLock::new(std::collections::HashMap::new())),
            shutdown_token: CancellationToken::new(),
            event_tx,
            start_time: None,
            ipc_shutdown_tx: None,
        })
    }

    /// Loads or generates the device identity.
    fn load_or_generate_identity(path: &PathBuf) -> Result<DeviceIdentity> {
        if path.exists() {
            // Load existing identity
            let bytes = std::fs::read(path)
                .with_context(|| format!("Failed to read identity file: {}", path.display()))?;
            if bytes.len() != 32 {
                anyhow::bail!(
                    "Invalid identity file: expected 32 bytes, got {}",
                    bytes.len()
                );
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            Ok(DeviceIdentity::from_secret_key_bytes(&key))
        } else {
            // Generate new identity
            let identity = DeviceIdentity::generate();

            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
            }

            // Save the identity
            std::fs::write(path, identity.secret_key_bytes())
                .with_context(|| format!("Failed to write identity file: {}", path.display()))?;

            info!("Generated new device identity and saved to {:?}", path);
            Ok(identity)
        }
    }

    /// Returns the device ID fingerprint.
    pub fn device_id_fingerprint(&self) -> String {
        self.identity.device_id().fingerprint()
    }

    /// Returns the current state.
    pub async fn state(&self) -> OrchestratorState {
        *self.state.read().await
    }

    /// Returns a receiver for orchestrator events.
    pub fn subscribe(&self) -> broadcast::Receiver<OrchestratorEvent> {
        self.event_tx.subscribe()
    }

    /// Starts the daemon orchestrator.
    pub async fn start(&mut self) -> Result<()> {
        // Check current state
        {
            let mut state = self.state.write().await;
            if *state != OrchestratorState::Stopped {
                anyhow::bail!("Orchestrator is already running");
            }
            *state = OrchestratorState::Starting;
        }
        self.emit_event(OrchestratorEvent::StateChanged(OrchestratorState::Starting));

        info!("Starting daemon orchestrator...");

        // Record start time for uptime tracking
        self.start_time = Some(Instant::now());

        // Create PID file
        let pid_file = self.get_pid_file_path();
        self.create_pid_file(&pid_file)
            .context("Failed to create PID file")?;
        debug!("Created PID file at {:?}", pid_file);

        // Start IPC server
        let socket_path = get_socket_path();
        let ipc_server = IpcServer::bind(&socket_path)
            .await
            .context("Failed to start IPC server")?;
        info!("Started IPC server at {:?}", socket_path);

        // Spawn IPC handler task
        let (ipc_shutdown_tx, ipc_shutdown_rx) = oneshot::channel();
        self.ipc_shutdown_tx = Some(ipc_shutdown_tx);

        let session_manager_for_ipc = Arc::clone(&self.session_manager);
        let start_time_for_ipc = self.start_time;
        let shutdown_token_for_ipc = self.shutdown_token.clone();
        let connections_for_ipc = Arc::clone(&self.connections);

        tokio::spawn(async move {
            Self::handle_ipc_requests(
                ipc_server,
                session_manager_for_ipc,
                start_time_for_ipc,
                shutdown_token_for_ipc,
                connections_for_ipc,
                ipc_shutdown_rx,
            )
            .await;
        });

        // Start session cleanup task
        let session_manager = Arc::clone(&self.session_manager);
        session_manager.start_cleanup_task(SESSION_CLEANUP_INTERVAL_SECS);
        debug!("Started session cleanup task");

        // Start approval cleanup task
        let trust_store_for_cleanup = Arc::clone(&self.trust_store);
        let approval_timeout = self.config.security.approval_timeout;
        let shutdown_token_for_cleanup = self.shutdown_token.clone();
        tokio::spawn(async move {
            Self::run_approval_cleanup_task(
                trust_store_for_cleanup,
                approval_timeout,
                shutdown_token_for_cleanup,
            )
            .await;
        });
        debug!("Started approval cleanup task");

        // Initialize signaling client
        let signaling_config = SignalingConfig::new(&self.config.network.signaling_url)
            .with_auto_reconnect(true)
            .with_initial_backoff(Duration::from_millis(500))
            .with_max_backoff(Duration::from_secs(30));

        let signaling_client = Arc::new(WebSocketSignalingClient::new(signaling_config));
        self.signaling_client = Some(Arc::clone(&signaling_client));

        // Start the signaling client
        Arc::clone(&signaling_client).start();
        info!("Started signaling client");

        // Spawn task to handle signaling events
        let event_tx = self.event_tx.clone();
        let shutdown_token = self.shutdown_token.clone();
        let identity = self.identity.clone();
        let connections = Arc::clone(&self.connections);
        let router = Arc::clone(&self.router);
        let config = self.config.clone();

        tokio::spawn(async move {
            Self::handle_signaling_loop(
                signaling_client,
                event_tx,
                shutdown_token,
                identity,
                connections,
                router,
                config,
            )
            .await;
        });

        // Update state to running
        {
            let mut state = self.state.write().await;
            *state = OrchestratorState::Running;
        }
        self.emit_event(OrchestratorEvent::StateChanged(OrchestratorState::Running));

        info!("Daemon orchestrator started successfully");
        Ok(())
    }

    /// Handles the signaling event loop.
    async fn handle_signaling_loop(
        signaling_client: Arc<WebSocketSignalingClient>,
        event_tx: broadcast::Sender<OrchestratorEvent>,
        shutdown_token: CancellationToken,
        identity: DeviceIdentity,
        connections: Arc<RwLock<std::collections::HashMap<String, ActiveConnection>>>,
        router: Arc<MessageRouter<SessionManagerImpl>>,
        config: Config,
    ) {
        let Some(mut events) = signaling_client.events() else {
            error!("Failed to get signaling events receiver - already taken");
            return;
        };

        loop {
            tokio::select! {
                _ = shutdown_token.cancelled() => {
                    info!("Signaling loop received shutdown signal");
                    break;
                }
                Some(event) = events.recv() => {
                    match event {
                        SignalingEvent::StateChanged(state) => {
                            debug!("Signaling state changed: {:?}", state);
                            let _ = event_tx.send(OrchestratorEvent::SignalingStateChanged(state));

                            // Join room when connected
                            if state == ConnectionState::Connected {
                                let device_id = identity.device_id().fingerprint();
                                let room_id = device_id.clone(); // Use device ID as room ID
                                if let Err(e) = signaling_client.join_room(&device_id, &room_id).await {
                                    error!("Failed to join signaling room: {}", e);
                                }
                            }
                        }
                        SignalingEvent::OfferReceived { sdp, from_device_id } => {
                            info!("Received offer from {:?}", from_device_id);
                            Self::handle_offer(
                                &signaling_client,
                                &identity,
                                &connections,
                                &router,
                                &event_tx,
                                &shutdown_token,
                                &config,
                                sdp,
                                from_device_id,
                            )
                            .await;
                        }
                        SignalingEvent::AnswerReceived { sdp, from_device_id } => {
                            info!("Received answer from {:?}", from_device_id);
                            Self::handle_answer(&connections, sdp, from_device_id).await;
                        }
                        SignalingEvent::IceCandidateReceived {
                            candidate,
                            sdp_mid,
                            sdp_mline_index,
                            from_device_id,
                        } => {
                            debug!("Received ICE candidate from {:?}", from_device_id);
                            Self::handle_ice_candidate(
                                &connections,
                                candidate,
                                sdp_mid,
                                sdp_mline_index,
                                from_device_id,
                            )
                            .await;
                        }
                        SignalingEvent::PeerJoined { device_id } => {
                            info!("Peer joined: {}", device_id);
                        }
                        SignalingEvent::PeerLeft { device_id } => {
                            info!("Peer left: {}", device_id);
                            let mut conns = connections.write().await;
                            if conns.remove(&device_id).is_some() {
                                let _ = event_tx.send(OrchestratorEvent::PeerDisconnected {
                                    device_id,
                                    reason: "Peer left signaling room".to_string(),
                                });
                            }
                        }
                        SignalingEvent::RoomJoined { room_id } => {
                            info!("Joined room: {}", room_id);
                        }
                        SignalingEvent::Error { message } => {
                            error!("Signaling error: {}", message);
                            let _ = event_tx.send(OrchestratorEvent::Error { message });
                        }
                    }
                }
            }
        }
    }

    /// Handles an incoming WebRTC offer.
    #[allow(clippy::too_many_arguments)]
    async fn handle_offer(
        signaling_client: &Arc<WebSocketSignalingClient>,
        identity: &DeviceIdentity,
        connections: &Arc<RwLock<std::collections::HashMap<String, ActiveConnection>>>,
        router: &Arc<MessageRouter<SessionManagerImpl>>,
        event_tx: &broadcast::Sender<OrchestratorEvent>,
        shutdown_token: &CancellationToken,
        config: &Config,
        sdp: String,
        from_device_id: Option<String>,
    ) {
        let device_id = from_device_id
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        // Create WebRTC handler
        let ice_servers = config
            .network
            .stun_servers
            .iter()
            .map(crate::network::webrtc::IceServer::stun)
            .collect();
        let webrtc_config = WebRtcConfig::with_ice_servers(ice_servers);

        let handler = match WebRtcConnectionHandler::new(webrtc_config, identity.clone()).await {
            Ok(h) => h,
            Err(e) => {
                error!("Failed to create WebRTC handler: {}", e);
                return;
            }
        };

        // Set up incoming data channels
        handler.setup_incoming_data_channels().await;

        // Parse and set remote description (offer)
        let offer =
            match webrtc::peer_connection::sdp::session_description::RTCSessionDescription::offer(
                sdp,
            ) {
                Ok(o) => o,
                Err(e) => {
                    error!("Invalid SDP offer: {}", e);
                    return;
                }
            };
        if let Err(e) = handler.set_remote_description(offer).await {
            error!("Failed to set remote description: {}", e);
            return;
        }

        // Create answer
        let _answer = match handler.create_answer().await {
            Ok(a) => a,
            Err(e) => {
                error!("Failed to create answer: {}", e);
                return;
            }
        };

        // Gather ICE candidates
        let answer_with_candidates = match handler.gather_ice_candidates().await {
            Ok(a) => a,
            Err(e) => {
                error!("Failed to gather ICE candidates: {}", e);
                return;
            }
        };

        // Send answer
        if let Err(e) = signaling_client
            .send_answer(&answer_with_candidates.sdp, from_device_id.as_deref())
            .await
        {
            error!("Failed to send answer: {}", e);
            return;
        }

        // Store connection
        let connection = ActiveConnection {
            device_id: device_id.clone(),
            handler,
            noise_complete: false,
        };

        let mut conns = connections.write().await;
        conns.insert(device_id.clone(), connection);
        drop(conns); // Release lock before spawning task

        // Notify subscribers that a peer connected
        let _ = event_tx.send(OrchestratorEvent::PeerConnected {
            device_id: device_id.clone(),
        });

        // Spawn a task to handle messages from this connection
        let connections_for_handler = Arc::clone(connections);
        let router_for_handler = Arc::clone(router);
        let event_tx_for_handler = event_tx.clone();
        let shutdown_token_for_handler = shutdown_token.clone();
        let device_id_for_handler = device_id.clone();

        tokio::spawn(async move {
            Self::handle_connection_messages(
                device_id_for_handler,
                connections_for_handler,
                router_for_handler,
                event_tx_for_handler,
                shutdown_token_for_handler,
            )
            .await;
        });
    }

    /// Handles incoming messages from a WebRTC connection.
    ///
    /// This task runs for the lifetime of the connection, receiving messages from
    /// the control and files channels, routing them through the MessageRouter,
    /// and sending responses back to the client.
    async fn handle_connection_messages(
        device_id: String,
        connections: Arc<RwLock<std::collections::HashMap<String, ActiveConnection>>>,
        router: Arc<MessageRouter<SessionManagerImpl>>,
        event_tx: broadcast::Sender<OrchestratorEvent>,
        shutdown_token: CancellationToken,
    ) {
        use crate::network::ChannelType;
        use protocol::messages::{Envelope, Message};

        info!(device_id = %device_id, "Starting message handler for connection");

        // Parse the device ID from fingerprint format
        let parsed_device_id = match Self::parse_device_id_from_fingerprint(&device_id) {
            Some(id) => id,
            None => {
                warn!(device_id = %device_id, "Failed to parse device ID, using placeholder");
                DeviceId::from_bytes([0u8; 16])
            }
        };

        let mut sequence: u64 = 1;

        // Channels to try, in order of priority
        let channels = [ChannelType::Control, ChannelType::Files];
        let mut current_channel_idx = 0;

        loop {
            // Check for shutdown
            if shutdown_token.is_cancelled() {
                info!(device_id = %device_id, "Message handler received shutdown signal");
                break;
            }

            // Round-robin between channels to handle messages from all channels
            let channel_type = channels[current_channel_idx];
            current_channel_idx = (current_channel_idx + 1) % channels.len();

            // Try to receive a message from the current channel
            let recv_result = {
                let mut conns = connections.write().await;
                let Some(conn) = conns.get_mut(&device_id) else {
                    info!(device_id = %device_id, "Connection no longer exists, stopping message handler");
                    break;
                };

                // Use a short timeout to allow checking other channels
                tokio::select! {
                    biased;
                    _ = shutdown_token.cancelled() => {
                        info!(device_id = %device_id, "Shutdown during recv");
                        return; // Exit directly since we're in a spawned task
                    }
                    result = conn.handler.recv(channel_type) => {
                        Some(result)
                    }
                    _ = tokio::time::sleep(Duration::from_millis(10)) => {
                        None // Timeout, try next channel
                    }
                }
            };

            let Some(result) = recv_result else {
                continue;
            };

            let data = match result {
                Ok(data) => data,
                Err(e) => {
                    // Check if this is a "no data" error or a real connection error
                    let error_str = e.to_string();
                    if error_str.contains("channel closed")
                        || error_str.contains("connection closed")
                    {
                        // Connection might be closed or had an error
                        debug!(device_id = %device_id, error = %e, "Error receiving message, connection may be closed");
                        // Remove connection and emit disconnect event
                        let mut conns = connections.write().await;
                        if conns.remove(&device_id).is_some() {
                            let _ = event_tx.send(OrchestratorEvent::PeerDisconnected {
                                device_id: device_id.clone(),
                                reason: format!("Receive error: {}", e),
                            });
                        }
                        break;
                    }
                    // Other errors - log and continue
                    debug!(device_id = %device_id, error = %e, channel = ?channel_type, "Receive error");
                    continue;
                }
            };

            // Decode the envelope
            let envelope = match Envelope::from_msgpack(&data) {
                Ok(env) => env,
                Err(e) => {
                    warn!(device_id = %device_id, error = %e, "Failed to decode message envelope");
                    continue;
                }
            };

            debug!(
                device_id = %device_id,
                sequence = envelope.sequence,
                channel = ?channel_type,
                "Received message"
            );

            // Get the authenticated public key from the connection for device verification
            let authenticated_public_key = {
                let conns = connections.read().await;
                conns
                    .get(&device_id)
                    .and_then(|conn| conn.handler.peer_public_key())
            };

            // Route the message through the router
            let response = router
                .route(
                    envelope.payload,
                    &parsed_device_id,
                    authenticated_public_key.as_ref(),
                )
                .await;

            // Handle the routing result
            match response {
                Ok(Some(response_msg)) => {
                    // Send the response back on the same channel
                    let response_envelope = Envelope::new(sequence, response_msg);
                    sequence += 1;

                    match response_envelope.to_msgpack() {
                        Ok(response_data) => {
                            let mut conns = connections.write().await;
                            if let Some(conn) = conns.get_mut(&device_id) {
                                if let Err(e) =
                                    conn.handler.send(channel_type, &response_data).await
                                {
                                    warn!(device_id = %device_id, error = %e, "Failed to send response");
                                }
                            }
                        }
                        Err(e) => {
                            error!(device_id = %device_id, error = %e, "Failed to encode response");
                        }
                    }
                }
                Ok(None) => {
                    // No response needed (e.g., for data messages or acknowledgments)
                    debug!(device_id = %device_id, "Message handled, no response needed");
                }
                Err(e) => {
                    // Send an error response
                    warn!(device_id = %device_id, error = %e, "Error routing message");
                    let error_msg = e.to_error_message(None);
                    let error_response = Message::Error(error_msg);
                    let response_envelope = Envelope::new(sequence, error_response);
                    sequence += 1;

                    if let Ok(response_data) = response_envelope.to_msgpack() {
                        let mut conns = connections.write().await;
                        if let Some(conn) = conns.get_mut(&device_id) {
                            let _ = conn.handler.send(channel_type, &response_data).await;
                        }
                    }
                }
            }
        }

        info!(device_id = %device_id, "Message handler stopped");
    }

    /// Parse a device ID from its fingerprint format.
    fn parse_device_id_from_fingerprint(fingerprint: &str) -> Option<DeviceId> {
        // Remove colons and decode hex
        let hex_str: String = fingerprint.chars().filter(|c| *c != ':').collect();

        if hex_str.len() != 32 {
            // DeviceId is 16 bytes = 32 hex chars
            return None;
        }

        let bytes = hex::decode(&hex_str).ok()?;
        if bytes.len() != 16 {
            return None;
        }

        let mut arr = [0u8; 16];
        arr.copy_from_slice(&bytes);
        Some(DeviceId::from_bytes(arr))
    }

    /// Handles an incoming WebRTC answer.
    async fn handle_answer(
        connections: &Arc<RwLock<std::collections::HashMap<String, ActiveConnection>>>,
        sdp: String,
        from_device_id: Option<String>,
    ) {
        let device_id = from_device_id.unwrap_or_else(|| "unknown".to_string());

        let mut conns = connections.write().await;
        if let Some(conn) = conns.get_mut(&device_id) {
            let answer = match webrtc::peer_connection::sdp::session_description::RTCSessionDescription::answer(sdp) {
                Ok(a) => a,
                Err(e) => {
                    error!("Invalid SDP answer from device {}: {}", device_id, e);
                    return;
                }
            };
            if let Err(e) = conn.handler.set_remote_description(answer).await {
                error!("Failed to set remote description for answer: {}", e);
            }
        } else {
            warn!("Received answer from unknown device: {}", device_id);
        }
    }

    /// Handles an incoming ICE candidate.
    async fn handle_ice_candidate(
        connections: &Arc<RwLock<std::collections::HashMap<String, ActiveConnection>>>,
        candidate: String,
        sdp_mid: Option<String>,
        sdp_mline_index: Option<u16>,
        from_device_id: Option<String>,
    ) {
        let device_id = from_device_id.unwrap_or_else(|| "unknown".to_string());

        let conns = connections.read().await;
        if let Some(conn) = conns.get(&device_id) {
            // Add ICE candidate to the peer connection
            let candidate_init = webrtc::ice_transport::ice_candidate::RTCIceCandidateInit {
                candidate,
                sdp_mid,
                sdp_mline_index,
                ..Default::default()
            };

            if let Err(e) = conn
                .handler
                .peer_connection()
                .add_ice_candidate(candidate_init)
                .await
            {
                error!("Failed to add ICE candidate: {}", e);
            }
        } else {
            warn!("Received ICE candidate from unknown device: {}", device_id);
        }
    }

    /// Runs the periodic approval cleanup task.
    ///
    /// This task runs at `APPROVAL_CLEANUP_INTERVAL_SECS` intervals and removes
    /// pending approvals that have exceeded the configured timeout.
    async fn run_approval_cleanup_task(
        trust_store: Arc<TrustStore>,
        timeout_secs: u64,
        shutdown_token: CancellationToken,
    ) {
        // Skip cleanup if timeout is 0 (disabled)
        if timeout_secs == 0 {
            debug!("Approval timeout is 0, cleanup task disabled");
            return;
        }

        let mut interval =
            tokio::time::interval(Duration::from_secs(APPROVAL_CLEANUP_INTERVAL_SECS));

        loop {
            tokio::select! {
                _ = shutdown_token.cancelled() => {
                    debug!("Approval cleanup task received shutdown signal");
                    break;
                }
                _ = interval.tick() => {
                    match trust_store.cleanup_expired_approvals(timeout_secs) {
                        Ok(expired) => {
                            for device_id in expired {
                                info!(?device_id, "Pending approval expired");
                            }
                        }
                        Err(e) => {
                            warn!("Failed to cleanup expired approvals: {}", e);
                        }
                    }
                }
            }
        }
    }

    /// Stops the daemon orchestrator gracefully.
    pub async fn stop(&mut self) -> Result<()> {
        // Check current state
        {
            let mut state = self.state.write().await;
            if *state == OrchestratorState::Stopped {
                return Ok(());
            }
            if *state == OrchestratorState::ShuttingDown {
                anyhow::bail!("Orchestrator is already shutting down");
            }
            *state = OrchestratorState::ShuttingDown;
        }
        self.emit_event(OrchestratorEvent::StateChanged(
            OrchestratorState::ShuttingDown,
        ));

        info!("Stopping daemon orchestrator...");

        // Signal IPC server to stop
        if let Some(tx) = self.ipc_shutdown_tx.take() {
            let _ = tx.send(());
        }

        // Remove socket file
        let socket_path = get_socket_path();
        if let Err(e) = std::fs::remove_file(&socket_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!("Failed to remove socket file: {}", e);
            }
        }

        // Remove PID file
        let pid_file = self.get_pid_file_path();
        self.remove_pid_file(&pid_file);
        debug!("Removed PID file");

        // Signal shutdown to all tasks
        self.shutdown_token.cancel();

        // Disconnect signaling client
        if let Some(ref client) = self.signaling_client {
            let _ = client.disconnect().await;
        }

        // Close all connections
        {
            let mut conns = self.connections.write().await;
            for (device_id, mut conn) in conns.drain() {
                debug!("Closing connection to {}", device_id);
                if let Err(e) = Connection::close(&mut conn.handler).await {
                    warn!("Error closing connection to {}: {}", device_id, e);
                }
            }
        }

        // Kill all sessions
        let sessions = self.session_manager.list();
        for session_info in sessions {
            debug!("Killing session {}", session_info.id);
            if let Err(e) = self.session_manager.kill(&session_info.id, Some(9)).await {
                warn!("Error killing session {}: {}", session_info.id, e);
            }
        }

        // Save trust store
        if let Err(e) = self.trust_store.save() {
            warn!("Error saving trust store: {}", e);
        }

        // Update state
        {
            let mut state = self.state.write().await;
            *state = OrchestratorState::Stopped;
        }
        self.emit_event(OrchestratorEvent::StateChanged(OrchestratorState::Stopped));

        info!("Daemon orchestrator stopped");
        Ok(())
    }

    /// Gets the path to the PID file.
    fn get_pid_file_path(&self) -> PathBuf {
        self.config.daemon.data_dir.join("daemon.pid")
    }

    /// Creates the PID file with the current process ID.
    fn create_pid_file(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, std::process::id().to_string())
    }

    /// Removes the PID file.
    fn remove_pid_file(&self, path: &Path) {
        if let Err(e) = std::fs::remove_file(path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!("Failed to remove PID file: {}", e);
            }
        }
    }

    /// Handles IPC requests in a separate task.
    async fn handle_ipc_requests(
        server: IpcServer,
        session_manager: Arc<SessionManagerImpl>,
        start_time: Option<Instant>,
        shutdown_token: CancellationToken,
        connections: Arc<RwLock<std::collections::HashMap<String, ActiveConnection>>>,
        mut shutdown_rx: oneshot::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    debug!("IPC server received shutdown signal");
                    break;
                }
                result = server.accept() => {
                    match result {
                        Ok(mut conn) => {
                            let session_manager = Arc::clone(&session_manager);
                            let shutdown_token = shutdown_token.clone();
                            let connections = Arc::clone(&connections);
                            tokio::spawn(async move {
                                while let Ok(Some(request)) = conn.read_request().await {
                                    let response = Self::handle_ipc_request(
                                        &request,
                                        &session_manager,
                                        start_time,
                                        &shutdown_token,
                                        &connections,
                                    )
                                    .await;
                                    if conn.send_response(&response).await.is_err() {
                                        break;
                                    }
                                    // If we just processed a Stop request, break the loop
                                    if matches!(request, IpcRequest::Stop) {
                                        break;
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept IPC connection: {}", e);
                        }
                    }
                }
            }
        }
    }

    /// Handles a single IPC request and returns the response.
    async fn handle_ipc_request(
        request: &IpcRequest,
        session_manager: &Arc<SessionManagerImpl>,
        start_time: Option<Instant>,
        shutdown_token: &CancellationToken,
        connections: &Arc<RwLock<std::collections::HashMap<String, ActiveConnection>>>,
    ) -> IpcResponse {
        match request {
            IpcRequest::Ping => IpcResponse::Pong,
            IpcRequest::Status => {
                let uptime_secs = start_time.map(|t| t.elapsed().as_secs()).unwrap_or(0);
                let session_count = session_manager.count();
                let device_count = connections.read().await.len();
                IpcResponse::Status {
                    running: true,
                    uptime_secs,
                    session_count,
                    device_count,
                }
            }
            IpcRequest::Stop => {
                info!("Received stop request via IPC");
                shutdown_token.cancel();
                IpcResponse::Stopping
            }
            IpcRequest::ListSessions => {
                let sessions = session_manager
                    .list()
                    .into_iter()
                    .map(|s| IpcSessionInfo {
                        id: s.id.to_string(),
                        connected_at: 0, // TODO: Track actual connection time
                        peer_id: None,   // TODO: Track peer ID when available
                    })
                    .collect();
                IpcResponse::Sessions { sessions }
            }
            IpcRequest::KillSession { session_id, signal } => {
                // Use provided signal or default to SIGTERM (15)
                let sig = signal.unwrap_or(15);
                match session_manager.kill(session_id, Some(sig)).await {
                    Ok(_) => IpcResponse::SessionKilled {
                        session_id: session_id.clone(),
                    },
                    Err(e) => IpcResponse::Error {
                        message: e.to_string(),
                    },
                }
            }
        }
    }

    /// Emits an orchestrator event.
    fn emit_event(&self, event: OrchestratorEvent) {
        let _ = self.event_tx.send(event);
    }

    /// Returns the session manager.
    pub fn session_manager(&self) -> &Arc<SessionManagerImpl> {
        &self.session_manager
    }

    /// Returns the trust store.
    pub fn trust_store(&self) -> &Arc<TrustStore> {
        &self.trust_store
    }

    /// Returns the message router.
    pub fn router(&self) -> &Arc<MessageRouter<SessionManagerImpl>> {
        &self.router
    }

    /// Returns the number of active connections.
    pub async fn connection_count(&self) -> usize {
        self.connections.read().await.len()
    }

    /// Returns a shared reference to the active connections map.
    pub fn connections(&self) -> Arc<RwLock<std::collections::HashMap<String, ActiveConnection>>> {
        Arc::clone(&self.connections)
    }

    /// Returns the shutdown token for external tasks to observe shutdown.
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown_token.clone()
    }

    /// Returns the directory browser for file listing operations.
    pub fn directory_browser(&self) -> &Arc<DirectoryBrowser> {
        &self.directory_browser
    }

    /// Returns the file transfer handler for upload/download operations.
    pub fn file_transfer(&self) -> &Arc<FileTransfer> {
        &self.file_transfer
    }

    /// Returns the device identity.
    pub fn identity(&self) -> &DeviceIdentity {
        &self.identity
    }

    /// Returns the signaling URL.
    pub fn signaling_url(&self) -> &str {
        &self.config.network.signaling_url
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_config(temp_dir: &TempDir) -> Config {
        let mut config = Config::default();
        config.daemon.data_dir = temp_dir.path().to_path_buf();
        config.file.allowed_paths = vec![temp_dir.path().to_path_buf()];
        config
    }

    #[tokio::test]
    async fn test_orchestrator_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);

        let orchestrator = DaemonOrchestrator::new(config);
        assert!(orchestrator.is_ok());

        let orchestrator = orchestrator.unwrap();
        assert_eq!(orchestrator.state().await, OrchestratorState::Stopped);
    }

    #[tokio::test]
    async fn test_identity_generation() {
        let temp_dir = TempDir::new().unwrap();
        let identity_path = temp_dir.path().join("identity.key");

        // First call should generate new identity
        let identity1 = DaemonOrchestrator::load_or_generate_identity(&identity_path).unwrap();
        assert!(identity_path.exists());

        // Second call should load existing identity
        let identity2 = DaemonOrchestrator::load_or_generate_identity(&identity_path).unwrap();
        assert_eq!(identity1.device_id(), identity2.device_id());
    }

    #[tokio::test]
    async fn test_orchestrator_state_transitions() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);

        let orchestrator = DaemonOrchestrator::new(config).unwrap();

        // Initial state should be Stopped
        assert_eq!(orchestrator.state().await, OrchestratorState::Stopped);

        // Note: Full start/stop tests require a running signaling server
    }

    #[tokio::test]
    async fn test_device_id_fingerprint() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);

        let orchestrator = DaemonOrchestrator::new(config).unwrap();
        let fingerprint = orchestrator.device_id_fingerprint();

        // Fingerprint should be in the format "xxxx:xxxx:xxxx:xxxx:xxxx:xxxx:xxxx:xxxx"
        assert!(fingerprint.contains(':'));
        assert!(!fingerprint.is_empty());
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);

        let orchestrator = DaemonOrchestrator::new(config).unwrap();
        let _receiver = orchestrator.subscribe();

        // Just verify we can subscribe without errors
    }

    #[tokio::test]
    async fn test_connection_count() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);

        let orchestrator = DaemonOrchestrator::new(config).unwrap();
        assert_eq!(orchestrator.connection_count().await, 0);
    }
}
