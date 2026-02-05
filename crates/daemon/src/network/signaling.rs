//! Signaling client for WebRTC negotiation.
//!
//! This module provides a client for connecting to the signaling server,
//! which coordinates WebRTC peer connections through:
//! - Room management (URL-path-based: `/room/{roomId}`)
//! - SDP offer/answer exchange
//! - ICE candidate relay
//!
//! The server uses a `data` envelope for messages and `peerId` (UUID assigned
//! by the server) for peer identification. Message types use kebab-case.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::config::DEFAULT_SIGNALING_URL;

use futures_util::{SinkExt, StreamExt};
use protocol::error::{ProtocolError, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Error as WsError, Message as WsMessage},
};
use url::Url;

/// Default reconnection settings.
const INITIAL_BACKOFF_MS: u64 = 100;
const MAX_BACKOFF_MS: u64 = 30_000;
const BACKOFF_MULTIPLIER: f64 = 2.0;

// ---------------------------------------------------------------------------
// Outgoing messages (daemon → signaling server)
// ---------------------------------------------------------------------------

/// SDP data wrapped in the `data` envelope for offer/answer messages.
#[derive(Debug, Clone, Serialize)]
pub struct SdpData {
    pub sdp: String,
    #[serde(rename = "type")]
    pub sdp_type: String,
}

/// ICE candidate data wrapped in the `data` envelope.
#[derive(Debug, Clone, Serialize)]
pub struct IceCandidateData {
    pub candidate: String,
    #[serde(rename = "sdpMid", skip_serializing_if = "Option::is_none")]
    pub sdp_mid: Option<String>,
    #[serde(rename = "sdpMLineIndex", skip_serializing_if = "Option::is_none")]
    pub sdp_mline_index: Option<u16>,
}

/// Messages sent from the daemon to the signaling server.
///
/// The server only accepts `offer`, `answer`, and `ice` — room membership
/// is handled by connecting to `/room/{roomId}`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum OutgoingMessage {
    Offer { data: SdpData },
    Answer { data: SdpData },
    Ice { data: IceCandidateData },
}

// ---------------------------------------------------------------------------
// Incoming messages (signaling server → daemon)
// ---------------------------------------------------------------------------

/// Join data received when the server confirms room membership.
#[derive(Debug, Clone, Deserialize)]
pub struct JoinData {
    pub peers: Vec<String>,
}

/// SDP data received from a peer (inside the `data` envelope).
#[derive(Debug, Clone, Deserialize)]
pub struct IncomingSdpData {
    pub sdp: String,
    #[serde(rename = "type")]
    pub sdp_type: Option<String>,
}

/// ICE candidate data received from a peer.
#[derive(Debug, Clone, Deserialize)]
pub struct IncomingIceCandidateData {
    pub candidate: String,
    #[serde(rename = "sdpMid")]
    pub sdp_mid: Option<String>,
    #[serde(rename = "sdpMLineIndex")]
    pub sdp_mline_index: Option<u16>,
}

/// Error data from the server.
#[derive(Debug, Clone, Deserialize)]
pub struct ErrorData {
    pub message: String,
}

/// Messages received from the signaling server.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum IncomingMessage {
    /// Server confirms room join and provides our peer ID + existing peers.
    Join {
        #[serde(rename = "peerId")]
        peer_id: String,
        data: JoinData,
    },
    /// A new peer joined the room.
    PeerJoined {
        #[serde(rename = "peerId")]
        peer_id: String,
    },
    /// A peer left the room.
    PeerLeft {
        #[serde(rename = "peerId")]
        peer_id: String,
    },
    /// SDP offer relayed from another peer.
    Offer {
        #[serde(rename = "peerId")]
        peer_id: String,
        data: IncomingSdpData,
    },
    /// SDP answer relayed from another peer.
    Answer {
        #[serde(rename = "peerId")]
        peer_id: String,
        data: IncomingSdpData,
    },
    /// ICE candidate relayed from another peer.
    Ice {
        #[serde(rename = "peerId")]
        peer_id: String,
        data: IncomingIceCandidateData,
    },
    /// Error from the server.
    Error { data: ErrorData },
}

/// Connection state for the signaling client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Not connected to the signaling server.
    Disconnected,
    /// Attempting to connect.
    Connecting,
    /// Connected and ready.
    Connected,
    /// Reconnecting after a disconnect.
    Reconnecting,
}

/// Events emitted by the signaling client.
#[derive(Debug, Clone)]
pub enum SignalingEvent {
    /// Connection state changed.
    StateChanged(ConnectionState),
    /// Successfully joined a room. Contains our server-assigned peer ID
    /// and the list of peers already in the room.
    RoomJoined {
        peer_id: String,
        existing_peers: Vec<String>,
    },
    /// Received an offer from a peer.
    OfferReceived { sdp: String, from_peer_id: String },
    /// Received an answer from a peer.
    AnswerReceived { sdp: String, from_peer_id: String },
    /// Received an ICE candidate from a peer.
    IceCandidateReceived {
        candidate: String,
        sdp_mid: Option<String>,
        sdp_mline_index: Option<u16>,
        from_peer_id: String,
    },
    /// A peer joined the room.
    PeerJoined { peer_id: String },
    /// A peer left the room.
    PeerLeft { peer_id: String },
    /// Error occurred.
    Error { message: String },
}

/// Trait for signaling operations.
///
/// This trait abstracts the signaling client interface, allowing for
/// different implementations (e.g., WebSocket, mock for testing).
pub trait SignalingClient: Send + Sync {
    /// Connects to the signaling server.
    fn connect(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>>;

    /// Disconnects from the signaling server.
    fn disconnect(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>>;

    /// Joins a room by connecting to `/room/{room_id}` on the server.
    /// The server assigns a peer ID upon joining.
    fn join_room(
        &self,
        room_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>>;

    /// Leaves the current room.
    fn leave_room(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>>;

    /// Sends an SDP offer to all peers in the room.
    fn send_offer(
        &self,
        sdp: &str,
        target_peer_id: Option<&str>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>>;

    /// Sends an SDP answer to all peers in the room.
    fn send_answer(
        &self,
        sdp: &str,
        target_peer_id: Option<&str>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>>;

    /// Sends an ICE candidate to all peers in the room.
    fn send_ice_candidate(
        &self,
        candidate: &str,
        sdp_mid: Option<&str>,
        sdp_mline_index: Option<u16>,
        target_peer_id: Option<&str>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>>;

    /// Returns the current connection state.
    fn state(&self) -> ConnectionState;

    /// Returns a receiver for signaling events.
    /// Returns None if the receiver has already been taken or if the lock is contended.
    fn events(&self) -> Option<mpsc::Receiver<SignalingEvent>>;
}

/// Configuration for the WebSocket signaling client.
#[derive(Debug, Clone)]
pub struct SignalingConfig {
    /// The WebSocket URL of the signaling server (base, without room path).
    pub server_url: String,
    /// Initial backoff duration for reconnection.
    pub initial_backoff: Duration,
    /// Maximum backoff duration for reconnection.
    pub max_backoff: Duration,
    /// Multiplier for exponential backoff.
    pub backoff_multiplier: f64,
    /// Whether to automatically reconnect on disconnect.
    pub auto_reconnect: bool,
    /// Interval between heartbeat pings.
    pub heartbeat_interval: Duration,
    /// Timeout for heartbeat pong response.
    pub heartbeat_timeout: Duration,
}

impl Default for SignalingConfig {
    fn default() -> Self {
        Self {
            server_url: DEFAULT_SIGNALING_URL.to_string(),
            initial_backoff: Duration::from_millis(INITIAL_BACKOFF_MS),
            max_backoff: Duration::from_millis(MAX_BACKOFF_MS),
            backoff_multiplier: BACKOFF_MULTIPLIER,
            auto_reconnect: true,
            heartbeat_interval: Duration::from_secs(30),
            heartbeat_timeout: Duration::from_secs(10),
        }
    }
}

impl SignalingConfig {
    /// Creates a new configuration with the specified server URL.
    pub fn new(server_url: impl Into<String>) -> Self {
        Self {
            server_url: server_url.into(),
            ..Default::default()
        }
    }

    /// Sets whether to automatically reconnect on disconnect.
    pub fn with_auto_reconnect(mut self, auto_reconnect: bool) -> Self {
        self.auto_reconnect = auto_reconnect;
        self
    }

    /// Sets the initial backoff duration.
    pub fn with_initial_backoff(mut self, duration: Duration) -> Self {
        self.initial_backoff = duration;
        self
    }

    /// Sets the maximum backoff duration.
    pub fn with_max_backoff(mut self, duration: Duration) -> Self {
        self.max_backoff = duration;
        self
    }
}

/// Internal state for the WebSocket signaling client.
struct ClientState {
    /// Current connection state.
    connection_state: ConnectionState,
    /// Current room ID (if joined).
    current_room: Option<String>,
    /// Server-assigned peer ID for this connection.
    server_peer_id: Option<String>,
    /// Sender for outgoing messages.
    message_tx: Option<mpsc::Sender<OutgoingMessage>>,
    /// Current backoff duration for reconnection.
    current_backoff: Duration,
    /// Whether a shutdown has been requested.
    shutdown_requested: bool,
}

impl Default for ClientState {
    fn default() -> Self {
        Self {
            connection_state: ConnectionState::Disconnected,
            current_room: None,
            server_peer_id: None,
            message_tx: None,
            current_backoff: Duration::from_millis(INITIAL_BACKOFF_MS),
            shutdown_requested: false,
        }
    }
}

/// WebSocket-based signaling client implementation.
pub struct WebSocketSignalingClient {
    /// Configuration for the client.
    config: SignalingConfig,
    /// Internal state.
    state: Arc<RwLock<ClientState>>,
    /// Sender for events (to be cloned for the event receiver).
    event_tx: mpsc::Sender<SignalingEvent>,
    /// Receiver for events (returned by events()).
    event_rx: Arc<RwLock<Option<mpsc::Receiver<SignalingEvent>>>>,
}

impl WebSocketSignalingClient {
    /// Creates a new WebSocket signaling client.
    pub fn new(config: SignalingConfig) -> Self {
        let (event_tx, event_rx) = mpsc::channel(256);

        Self {
            config,
            state: Arc::new(RwLock::new(ClientState::default())),
            event_tx,
            event_rx: Arc::new(RwLock::new(Some(event_rx))),
        }
    }

    /// Updates the connection state and emits an event.
    ///
    /// If the state hasn't changed, no event is emitted (early return).
    async fn set_state(&self, new_state: ConnectionState) {
        let should_emit = {
            let mut state = self.state.write().await;
            let old_state = state.connection_state;

            if old_state == new_state {
                return;
            }

            state.connection_state = new_state;
            true
        };

        if should_emit {
            if let Err(e) = self
                .event_tx
                .send(SignalingEvent::StateChanged(new_state))
                .await
            {
                tracing::warn!(error = %e, state = ?new_state, "Failed to send StateChanged event - receiver may be dropped");
            }
        }
    }

    /// Sends an outgoing message to the signaling server.
    async fn send_message(&self, message: OutgoingMessage) -> Result<()> {
        let state = self.state.read().await;
        if let Some(ref tx) = state.message_tx {
            tx.send(message).await.map_err(|e| {
                ProtocolError::TransferFailed(format!("failed to send message: {}", e))
            })?;
            Ok(())
        } else {
            Err(ProtocolError::ConnectionClosed(
                "not connected to signaling server".to_string(),
            ))
        }
    }

    /// Handles an incoming message from the server.
    async fn handle_message(&self, message: IncomingMessage) {
        match message {
            IncomingMessage::Join { peer_id, data } => {
                {
                    let mut state = self.state.write().await;
                    state.server_peer_id = Some(peer_id.clone());
                }
                if let Err(e) = self
                    .event_tx
                    .send(SignalingEvent::RoomJoined {
                        peer_id: peer_id.clone(),
                        existing_peers: data.peers,
                    })
                    .await
                {
                    tracing::warn!(error = %e, peer_id = %peer_id, "Failed to send RoomJoined event - receiver may be dropped");
                }
            }
            IncomingMessage::Offer { peer_id, data } => {
                if let Err(e) = self
                    .event_tx
                    .send(SignalingEvent::OfferReceived {
                        sdp: data.sdp,
                        from_peer_id: peer_id,
                    })
                    .await
                {
                    tracing::warn!(error = %e, "Failed to send OfferReceived event - receiver may be dropped");
                }
            }
            IncomingMessage::Answer { peer_id, data } => {
                if let Err(e) = self
                    .event_tx
                    .send(SignalingEvent::AnswerReceived {
                        sdp: data.sdp,
                        from_peer_id: peer_id,
                    })
                    .await
                {
                    tracing::warn!(error = %e, "Failed to send AnswerReceived event - receiver may be dropped");
                }
            }
            IncomingMessage::Ice { peer_id, data } => {
                if let Err(e) = self
                    .event_tx
                    .send(SignalingEvent::IceCandidateReceived {
                        candidate: data.candidate,
                        sdp_mid: data.sdp_mid,
                        sdp_mline_index: data.sdp_mline_index,
                        from_peer_id: peer_id,
                    })
                    .await
                {
                    tracing::warn!(error = %e, "Failed to send IceCandidateReceived event - receiver may be dropped");
                }
            }
            IncomingMessage::PeerJoined { peer_id } => {
                if let Err(e) = self
                    .event_tx
                    .send(SignalingEvent::PeerJoined {
                        peer_id: peer_id.clone(),
                    })
                    .await
                {
                    tracing::warn!(error = %e, peer_id = %peer_id, "Failed to send PeerJoined event - receiver may be dropped");
                }
            }
            IncomingMessage::PeerLeft { peer_id } => {
                if let Err(e) = self
                    .event_tx
                    .send(SignalingEvent::PeerLeft {
                        peer_id: peer_id.clone(),
                    })
                    .await
                {
                    tracing::warn!(error = %e, peer_id = %peer_id, "Failed to send PeerLeft event - receiver may be dropped");
                }
            }
            IncomingMessage::Error { data } => {
                if let Err(e) = self
                    .event_tx
                    .send(SignalingEvent::Error {
                        message: data.message.clone(),
                    })
                    .await
                {
                    tracing::warn!(error = %e, signaling_error = %data.message, "Failed to send Error event - receiver may be dropped");
                }
            }
        }
    }

    /// Builds the WebSocket URL for connecting to a room.
    fn build_room_url(&self, room_id: &str) -> Result<String> {
        let base = self.config.server_url.trim_end_matches('/');
        let ws_url = format!("{}/room/{}", base, room_id);
        // Validate
        Url::parse(&ws_url)
            .map_err(|e| ProtocolError::HandshakeFailed(format!("invalid signaling URL: {}", e)))?;
        Ok(ws_url)
    }

    /// Runs the connection loop with reconnection support.
    async fn run_connection_loop(self: Arc<Self>) {
        loop {
            // Check if shutdown was requested
            {
                let state = self.state.read().await;
                if state.shutdown_requested {
                    break;
                }
            }

            // Get room ID to connect to
            let room_id = {
                let state = self.state.read().await;
                state.current_room.clone()
            };

            // Only attempt connection if we have a room to join
            let Some(room_id) = room_id else {
                // No room set yet, wait and check again
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            };

            // Attempt to connect
            self.set_state(ConnectionState::Connecting).await;

            match self.connect_internal(&room_id).await {
                Ok((message_tx, mut ws_rx, control_tx, last_pong)) => {
                    // Connection successful
                    {
                        let mut state = self.state.write().await;
                        state.message_tx = Some(message_tx);
                        state.current_backoff = self.config.initial_backoff;
                    }
                    self.set_state(ConnectionState::Connected).await;

                    // Set up heartbeat interval timer
                    let mut heartbeat_interval =
                        tokio::time::interval(self.config.heartbeat_interval);
                    // Skip the first immediate tick
                    heartbeat_interval.tick().await;

                    // Process incoming messages with heartbeat
                    loop {
                        tokio::select! {
                            _ = heartbeat_interval.tick() => {
                                // Check if we've received a pong recently
                                let last_pong_time = *last_pong.read().await;
                                if last_pong_time.elapsed() > self.config.heartbeat_timeout + self.config.heartbeat_interval {
                                    tracing::warn!("Heartbeat timeout, reconnecting...");
                                    break;
                                }

                                // Send ping
                                if let Err(e) = control_tx.send(WsMessage::Ping(vec![])).await {
                                    tracing::error!("Failed to send ping: {}", e);
                                    break;
                                }
                                tracing::debug!("Sent heartbeat ping");
                            }
                            Some(result) = ws_rx.recv() => {
                                match result {
                                    Ok(msg) => self.handle_message(msg).await,
                                    Err(e) => {
                                        tracing::error!("signaling receive error: {}", e);
                                        break;
                                    }
                                }
                            }
                            else => break,
                        }
                    }

                    // Connection lost
                    {
                        let mut state = self.state.write().await;
                        state.message_tx = None;
                        state.server_peer_id = None;
                    }
                }
                Err(e) => {
                    tracing::error!("signaling connection failed: {}", e);
                }
            }

            // Check if we should reconnect
            let should_reconnect = {
                let state = self.state.read().await;
                self.config.auto_reconnect && !state.shutdown_requested
            };

            if !should_reconnect {
                self.set_state(ConnectionState::Disconnected).await;
                break;
            }

            // Apply exponential backoff
            let backoff = {
                let mut state = self.state.write().await;
                let backoff = state.current_backoff;
                state.current_backoff = std::cmp::min(
                    Duration::from_secs_f64(
                        state.current_backoff.as_secs_f64() * self.config.backoff_multiplier,
                    ),
                    self.config.max_backoff,
                );
                backoff
            };

            self.set_state(ConnectionState::Reconnecting).await;
            tracing::info!("reconnecting in {:?}", backoff);
            tokio::time::sleep(backoff).await;
        }
    }

    /// Internal connection establishment.
    ///
    /// Connects to `{server_url}/room/{room_id}` via WebSocket.
    /// The server sends a `join` message upon connection with our peer ID.
    ///
    /// Returns:
    /// - Sender for outgoing messages
    /// - Receiver for incoming messages
    /// - Sender for WebSocket control frames (ping)
    /// - Shared timestamp of last pong received
    async fn connect_internal(
        &self,
        room_id: &str,
    ) -> Result<(
        mpsc::Sender<OutgoingMessage>,
        mpsc::Receiver<Result<IncomingMessage>>,
        mpsc::Sender<WsMessage>,
        Arc<RwLock<Instant>>,
    )> {
        let ws_url = self.build_room_url(room_id)?;

        tracing::info!("Connecting to signaling server: {}", ws_url);

        let (ws_stream, _) = connect_async(&ws_url).await.map_err(|e| match e {
            WsError::Io(io_err) => ProtocolError::from(io_err),
            _ => ProtocolError::ConnectionClosed(format!("WebSocket connection failed: {}", e)),
        })?;

        let (mut ws_sink, mut ws_stream) = ws_stream.split();

        // Create channels for message passing
        let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<OutgoingMessage>(256);
        let (incoming_tx, incoming_rx) = mpsc::channel::<Result<IncomingMessage>>(256);
        // Channel for raw WebSocket control frames (ping)
        let (control_tx, mut control_rx) = mpsc::channel::<WsMessage>(16);

        // Shared timestamp for last pong received (initialized to now for grace period)
        let last_pong = Arc::new(RwLock::new(Instant::now()));
        let last_pong_writer = last_pong.clone();

        // Spawn task to handle outgoing messages and control frames
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(msg) = outgoing_rx.recv() => {
                        match serde_json::to_string(&msg) {
                            Ok(json) => {
                                if let Err(e) = ws_sink.send(WsMessage::Text(json)).await {
                                    tracing::error!("failed to send WebSocket message: {}", e);
                                    break;
                                }
                            }
                            Err(e) => {
                                tracing::error!("failed to serialize message: {}", e);
                            }
                        }
                    }
                    Some(control_msg) = control_rx.recv() => {
                        if let Err(e) = ws_sink.send(control_msg).await {
                            tracing::error!("failed to send WebSocket control frame: {}", e);
                            break;
                        }
                    }
                    else => break,
                }
            }
        });

        // Spawn task to handle incoming messages
        tokio::spawn(async move {
            while let Some(result) = ws_stream.next().await {
                match result {
                    Ok(WsMessage::Text(text)) => {
                        match serde_json::from_str::<IncomingMessage>(&text) {
                            Ok(msg) => {
                                if incoming_tx.send(Ok(msg)).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "failed to parse signaling message: {} (raw: {})",
                                    e,
                                    text
                                );
                            }
                        }
                    }
                    Ok(WsMessage::Pong(_)) => {
                        // Update last pong timestamp for heartbeat tracking
                        *last_pong_writer.write().await = Instant::now();
                        tracing::debug!("Received heartbeat pong");
                    }
                    Ok(WsMessage::Close(_)) => {
                        let _ = incoming_tx
                            .send(Err(ProtocolError::ConnectionClosed(
                                "server closed connection".to_string(),
                            )))
                            .await;
                        break;
                    }
                    Err(e) => {
                        let _ = incoming_tx
                            .send(Err(ProtocolError::ConnectionClosed(format!(
                                "WebSocket error: {}",
                                e
                            ))))
                            .await;
                        break;
                    }
                    _ => {
                        // Ignore ping/binary messages
                    }
                }
            }
        });

        Ok((outgoing_tx, incoming_rx, control_tx, last_pong))
    }

    /// Starts the connection in the background.
    pub fn start(self: Arc<Self>) {
        let client = self.clone();
        tokio::spawn(async move {
            client.run_connection_loop().await;
        });
    }
}

impl SignalingClient for WebSocketSignalingClient {
    fn connect(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            {
                let mut state = self.state.write().await;
                state.shutdown_requested = false;
            }
            // Note: The actual connection is managed by start() and the connection loop
            Ok(())
        })
    }

    fn disconnect(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            let mut state = self.state.write().await;
            state.shutdown_requested = true;
            state.message_tx = None;
            state.server_peer_id = None;
            state.connection_state = ConnectionState::Disconnected;
            Ok(())
        })
    }

    fn join_room(
        &self,
        room_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        let room_id = room_id.to_string();

        Box::pin(async move {
            {
                let mut state = self.state.write().await;
                state.current_room = Some(room_id.clone());
            }

            // The connection loop will pick up the room and connect.
            // If we're already connected, we need to disconnect and reconnect
            // to the new room URL (the server uses URL-based room routing).
            let current_state = {
                let state = self.state.read().await;
                state.connection_state
            };

            if current_state == ConnectionState::Connected {
                // Force a reconnect to the new room by dropping the current message sender.
                // The connection loop will detect this and reconnect with the new room URL.
                let mut state = self.state.write().await;
                state.message_tx = None;
                state.server_peer_id = None;
            }

            Ok(())
        })
    }

    fn leave_room(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            {
                let mut state = self.state.write().await;
                state.current_room = None;
                state.server_peer_id = None;
                state.message_tx = None;
            }
            Ok(())
        })
    }

    fn send_offer(
        &self,
        sdp: &str,
        _target_peer_id: Option<&str>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        let sdp = sdp.to_string();

        Box::pin(async move {
            self.send_message(OutgoingMessage::Offer {
                data: SdpData {
                    sdp,
                    sdp_type: "offer".to_string(),
                },
            })
            .await
        })
    }

    fn send_answer(
        &self,
        sdp: &str,
        _target_peer_id: Option<&str>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        let sdp = sdp.to_string();

        Box::pin(async move {
            self.send_message(OutgoingMessage::Answer {
                data: SdpData {
                    sdp,
                    sdp_type: "answer".to_string(),
                },
            })
            .await
        })
    }

    fn send_ice_candidate(
        &self,
        candidate: &str,
        sdp_mid: Option<&str>,
        sdp_mline_index: Option<u16>,
        _target_peer_id: Option<&str>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        let candidate = candidate.to_string();
        let sdp_mid = sdp_mid.map(|s| s.to_string());

        Box::pin(async move {
            self.send_message(OutgoingMessage::Ice {
                data: IceCandidateData {
                    candidate,
                    sdp_mid,
                    sdp_mline_index,
                },
            })
            .await
        })
    }

    fn state(&self) -> ConnectionState {
        // Since we can't await in a sync function, we use try_read
        match self.state.try_read() {
            Ok(state) => state.connection_state,
            Err(_) => ConnectionState::Disconnected,
        }
    }

    fn events(&self) -> Option<mpsc::Receiver<SignalingEvent>> {
        // Returns None if already taken or if lock is contended
        match self.event_rx.try_write() {
            Ok(mut guard) => guard.take(),
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signaling_config_default() {
        let config = SignalingConfig::default();
        assert_eq!(config.server_url, DEFAULT_SIGNALING_URL);
        assert!(config.auto_reconnect);
        assert_eq!(config.initial_backoff, Duration::from_millis(100));
        assert_eq!(config.max_backoff, Duration::from_millis(30_000));
    }

    #[test]
    fn test_signaling_config_builder() {
        let config = SignalingConfig::new("wss://custom.server.com")
            .with_auto_reconnect(false)
            .with_initial_backoff(Duration::from_secs(1))
            .with_max_backoff(Duration::from_secs(60));

        assert_eq!(config.server_url, "wss://custom.server.com");
        assert!(!config.auto_reconnect);
        assert_eq!(config.initial_backoff, Duration::from_secs(1));
        assert_eq!(config.max_backoff, Duration::from_secs(60));
    }

    #[test]
    fn test_outgoing_message_serialization() {
        let offer = OutgoingMessage::Offer {
            data: SdpData {
                sdp: "v=0\r\n...".to_string(),
                sdp_type: "offer".to_string(),
            },
        };
        let json = serde_json::to_string(&offer).unwrap();
        assert!(json.contains("\"type\":\"offer\""));
        assert!(json.contains("\"data\":{"));
        assert!(json.contains("\"sdp\":\"v=0\\r\\n...\""));

        let answer = OutgoingMessage::Answer {
            data: SdpData {
                sdp: "v=0\r\n...".to_string(),
                sdp_type: "answer".to_string(),
            },
        };
        let json = serde_json::to_string(&answer).unwrap();
        assert!(json.contains("\"type\":\"answer\""));

        let ice = OutgoingMessage::Ice {
            data: IceCandidateData {
                candidate: "candidate:1 1 UDP 2130706431 ...".to_string(),
                sdp_mid: Some("0".to_string()),
                sdp_mline_index: Some(0),
            },
        };
        let json = serde_json::to_string(&ice).unwrap();
        assert!(json.contains("\"type\":\"ice\""));
        assert!(json.contains("\"candidate\":"));
        assert!(json.contains("\"sdpMid\":\"0\""));
        assert!(json.contains("\"sdpMLineIndex\":0"));
    }

    #[test]
    fn test_incoming_message_deserialization() {
        // Join message from server
        let json = r#"{"type":"join","peerId":"abc-123","data":{"peers":["peer-1","peer-2"]}}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        match msg {
            IncomingMessage::Join { peer_id, data } => {
                assert_eq!(peer_id, "abc-123");
                assert_eq!(data.peers, vec!["peer-1", "peer-2"]);
            }
            _ => panic!("unexpected message type"),
        }

        // Offer relayed from a peer
        let json = r#"{"type":"offer","peerId":"peer-1","data":{"sdp":"test-sdp","type":"offer"}}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        match msg {
            IncomingMessage::Offer { peer_id, data } => {
                assert_eq!(peer_id, "peer-1");
                assert_eq!(data.sdp, "test-sdp");
            }
            _ => panic!("unexpected message type"),
        }

        // Peer joined (kebab-case)
        let json = r#"{"type":"peer-joined","peerId":"peer-2"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        match msg {
            IncomingMessage::PeerJoined { peer_id } => {
                assert_eq!(peer_id, "peer-2");
            }
            _ => panic!("unexpected message type"),
        }

        // Peer left (kebab-case)
        let json = r#"{"type":"peer-left","peerId":"peer-3"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        match msg {
            IncomingMessage::PeerLeft { peer_id } => {
                assert_eq!(peer_id, "peer-3");
            }
            _ => panic!("unexpected message type"),
        }

        // Error from server
        let json = r#"{"type":"error","data":{"message":"Rate limit exceeded"}}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        match msg {
            IncomingMessage::Error { data } => {
                assert_eq!(data.message, "Rate limit exceeded");
            }
            _ => panic!("unexpected message type"),
        }

        // ICE candidate
        let json = r#"{"type":"ice","peerId":"peer-4","data":{"candidate":"candidate:1","sdpMid":"0","sdpMLineIndex":0}}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        match msg {
            IncomingMessage::Ice { peer_id, data } => {
                assert_eq!(peer_id, "peer-4");
                assert_eq!(data.candidate, "candidate:1");
                assert_eq!(data.sdp_mid, Some("0".to_string()));
                assert_eq!(data.sdp_mline_index, Some(0));
            }
            _ => panic!("unexpected message type"),
        }
    }

    #[tokio::test]
    async fn test_websocket_client_creation() {
        let config = SignalingConfig::new("wss://localhost:8080");
        let client = WebSocketSignalingClient::new(config);

        assert_eq!(client.state(), ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn test_connection_state_transitions() {
        let config = SignalingConfig::new("wss://localhost:8080").with_auto_reconnect(false);
        let client = WebSocketSignalingClient::new(config);

        // Initial state should be Disconnected
        assert_eq!(client.state(), ConnectionState::Disconnected);

        // Test state changes
        client.set_state(ConnectionState::Connecting).await;
        assert_eq!(client.state(), ConnectionState::Connecting);

        client.set_state(ConnectionState::Connected).await;
        assert_eq!(client.state(), ConnectionState::Connected);

        client.set_state(ConnectionState::Reconnecting).await;
        assert_eq!(client.state(), ConnectionState::Reconnecting);

        client.set_state(ConnectionState::Disconnected).await;
        assert_eq!(client.state(), ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn test_send_message_not_connected() {
        let config = SignalingConfig::new("wss://localhost:8080");
        let client = WebSocketSignalingClient::new(config);

        // Should fail when not connected
        let result = client
            .send_message(OutgoingMessage::Offer {
                data: SdpData {
                    sdp: "test".to_string(),
                    sdp_type: "offer".to_string(),
                },
            })
            .await;

        assert!(result.is_err());
        if let Err(ProtocolError::ConnectionClosed(msg)) = result {
            assert!(msg.contains("not connected"));
        } else {
            panic!("unexpected error type");
        }
    }

    #[tokio::test]
    async fn test_events_receiver() {
        let config = SignalingConfig::new("wss://localhost:8080");
        let client = WebSocketSignalingClient::new(config);

        // First call should succeed and return Some
        let events = client.events();
        assert!(
            events.is_some(),
            "First call to events() should return Some"
        );
        let _events = events.unwrap();

        // Second call should return None (already taken)
        let events_again = client.events();
        assert!(
            events_again.is_none(),
            "Second call to events() should return None"
        );

        // We can verify events are received by changing state
        client.set_state(ConnectionState::Connecting).await;
    }

    #[tokio::test]
    async fn test_join_room_stores_state() {
        let config = SignalingConfig::new("wss://localhost:8080");
        let client = WebSocketSignalingClient::new(config);

        // Join room (will store room but no connection available)
        let _ = client.join_room("room-1").await;

        // Verify state is stored
        let state = client.state.read().await;
        assert_eq!(state.current_room, Some("room-1".to_string()));
    }

    #[tokio::test]
    async fn test_disconnect() {
        let config = SignalingConfig::new("wss://localhost:8080");
        let client = WebSocketSignalingClient::new(config);

        // Disconnect should succeed even when not connected
        let result = client.disconnect().await;
        assert!(result.is_ok());

        // Verify state
        let state = client.state.read().await;
        assert!(state.shutdown_requested);
        assert_eq!(state.connection_state, ConnectionState::Disconnected);
    }

    #[test]
    fn test_connection_state_equality() {
        assert_eq!(ConnectionState::Disconnected, ConnectionState::Disconnected);
        assert_eq!(ConnectionState::Connecting, ConnectionState::Connecting);
        assert_eq!(ConnectionState::Connected, ConnectionState::Connected);
        assert_eq!(ConnectionState::Reconnecting, ConnectionState::Reconnecting);
        assert_ne!(ConnectionState::Disconnected, ConnectionState::Connected);
    }

    #[tokio::test]
    async fn test_set_state_no_event_on_same_state() {
        let config = SignalingConfig::new("wss://localhost:8080");
        let client = WebSocketSignalingClient::new(config);
        let mut events = client.events().expect("events() should return receiver");

        client.set_state(ConnectionState::Connecting).await;
        client.set_state(ConnectionState::Connecting).await;
        client.set_state(ConnectionState::Connected).await;

        let event1 = tokio::time::timeout(Duration::from_millis(100), events.recv()).await;
        assert!(event1.is_ok(), "Should receive first event");
        if let Ok(Some(SignalingEvent::StateChanged(state))) = event1 {
            assert_eq!(state, ConnectionState::Connecting);
        } else {
            panic!("Expected StateChanged(Connecting)");
        }

        let event2 = tokio::time::timeout(Duration::from_millis(100), events.recv()).await;
        assert!(event2.is_ok(), "Should receive second event");
        if let Ok(Some(SignalingEvent::StateChanged(state))) = event2 {
            assert_eq!(state, ConnectionState::Connected);
        } else {
            panic!("Expected StateChanged(Connected)");
        }

        let event3 = tokio::time::timeout(Duration::from_millis(100), events.recv()).await;
        assert!(
            event3.is_err(),
            "Should NOT receive a third event (duplicate was skipped)"
        );
    }

    #[tokio::test]
    async fn test_set_state_event_contains_correct_state() {
        let config = SignalingConfig::new("wss://localhost:8080");
        let client = WebSocketSignalingClient::new(config);
        let mut events = client.events().expect("events() should return receiver");

        let transitions = [
            ConnectionState::Connecting,
            ConnectionState::Connected,
            ConnectionState::Reconnecting,
            ConnectionState::Disconnected,
        ];

        for expected_state in transitions {
            client.set_state(expected_state).await;

            let event = tokio::time::timeout(Duration::from_millis(100), events.recv())
                .await
                .expect("Should receive event within timeout")
                .expect("Event channel should not be closed");

            match event {
                SignalingEvent::StateChanged(state) => {
                    assert_eq!(
                        state, expected_state,
                        "Event state should match the state that was set"
                    );
                }
                _ => panic!("Expected StateChanged event"),
            }

            assert_eq!(
                client.state(),
                expected_state,
                "Current state should match what was set"
            );
        }
    }

    #[test]
    fn test_build_room_url() {
        let config = SignalingConfig::new("wss://example.com");
        let client = WebSocketSignalingClient::new(config);
        let url = client.build_room_url("abc123").unwrap();
        assert_eq!(url, "wss://example.com/room/abc123");

        // With trailing slash
        let config = SignalingConfig::new("wss://example.com/");
        let client = WebSocketSignalingClient::new(config);
        let url = client.build_room_url("abc123").unwrap();
        assert_eq!(url, "wss://example.com/room/abc123");
    }

    /// Integration test for the signaling flow.
    ///
    /// Note: This test requires a running signaling server to pass.
    /// It is marked as ignore by default.
    #[tokio::test]
    #[ignore = "requires running signaling server"]
    async fn test_signaling_flow_integration() {
        let config = SignalingConfig::new("ws://localhost:8080")
            .with_auto_reconnect(false)
            .with_initial_backoff(Duration::from_millis(100));

        let client = Arc::new(WebSocketSignalingClient::new(config));
        let mut events = client
            .events()
            .expect("events() should return receiver on first call");

        // Join a room (this sets the room and the connection loop will connect)
        client.join_room("test-room").await.unwrap();

        // Start the client
        client.clone().start();

        // Wait for room joined event (the server sends join message automatically)
        tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(event) = events.recv().await {
                if let SignalingEvent::RoomJoined {
                    peer_id,
                    existing_peers,
                } = event
                {
                    assert!(!peer_id.is_empty());
                    // existing_peers can be empty if we're the first in the room
                    let _ = existing_peers;
                    break;
                }
            }
        })
        .await
        .expect("join timeout");

        // Send an offer
        client.send_offer("test-sdp-offer", None).await.unwrap();

        // Clean up
        client.disconnect().await.unwrap();
    }
}
