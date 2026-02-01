//! Signaling client for WebRTC negotiation.
//!
//! This module provides a client for connecting to the signaling server,
//! which coordinates WebRTC peer connections through:
//! - Room management (join/leave)
//! - SDP offer/answer exchange
//! - ICE candidate relay

use std::sync::Arc;
use std::time::Duration;

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

/// Signaling message types exchanged with the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignalingMessage {
    /// Join a room with device ID.
    Join { device_id: String, room_id: String },
    /// Leave the current room.
    Leave,
    /// Send an SDP offer.
    Offer {
        sdp: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        target_device_id: Option<String>,
    },
    /// Send an SDP answer.
    Answer {
        sdp: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        target_device_id: Option<String>,
    },
    /// Send an ICE candidate.
    Ice {
        candidate: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        sdp_mid: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sdp_mline_index: Option<u16>,
        #[serde(skip_serializing_if = "Option::is_none")]
        target_device_id: Option<String>,
    },
    /// Peer joined the room.
    PeerJoined { device_id: String },
    /// Peer left the room.
    PeerLeft { device_id: String },
    /// Error from server.
    Error { message: String },
    /// Room joined successfully.
    Joined { room_id: String },
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
    /// Received an offer from a peer.
    OfferReceived {
        sdp: String,
        from_device_id: Option<String>,
    },
    /// Received an answer from a peer.
    AnswerReceived {
        sdp: String,
        from_device_id: Option<String>,
    },
    /// Received an ICE candidate from a peer.
    IceCandidateReceived {
        candidate: String,
        sdp_mid: Option<String>,
        sdp_mline_index: Option<u16>,
        from_device_id: Option<String>,
    },
    /// A peer joined the room.
    PeerJoined { device_id: String },
    /// A peer left the room.
    PeerLeft { device_id: String },
    /// Joined a room successfully.
    RoomJoined { room_id: String },
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

    /// Joins a room with the given device ID.
    fn join_room(
        &self,
        device_id: &str,
        room_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>>;

    /// Leaves the current room.
    fn leave_room(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>>;

    /// Sends an SDP offer.
    fn send_offer(
        &self,
        sdp: &str,
        target_device_id: Option<&str>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>>;

    /// Sends an SDP answer.
    fn send_answer(
        &self,
        sdp: &str,
        target_device_id: Option<&str>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>>;

    /// Sends an ICE candidate.
    fn send_ice_candidate(
        &self,
        candidate: &str,
        sdp_mid: Option<&str>,
        sdp_mline_index: Option<u16>,
        target_device_id: Option<&str>,
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
    /// The WebSocket URL of the signaling server.
    pub server_url: String,
    /// Initial backoff duration for reconnection.
    pub initial_backoff: Duration,
    /// Maximum backoff duration for reconnection.
    pub max_backoff: Duration,
    /// Multiplier for exponential backoff.
    pub backoff_multiplier: f64,
    /// Whether to automatically reconnect on disconnect.
    pub auto_reconnect: bool,
}

impl Default for SignalingConfig {
    fn default() -> Self {
        Self {
            server_url: "wss://remoshell-signaling.moukrea.workers.dev".to_string(),
            initial_backoff: Duration::from_millis(INITIAL_BACKOFF_MS),
            max_backoff: Duration::from_millis(MAX_BACKOFF_MS),
            backoff_multiplier: BACKOFF_MULTIPLIER,
            auto_reconnect: true,
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
    /// Current device ID (if joined).
    current_device_id: Option<String>,
    /// Sender for outgoing messages.
    message_tx: Option<mpsc::Sender<SignalingMessage>>,
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
            current_device_id: None,
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
    async fn set_state(&self, new_state: ConnectionState) {
        {
            let mut state = self.state.write().await;
            state.connection_state = new_state;
        }
        if let Err(e) = self
            .event_tx
            .send(SignalingEvent::StateChanged(new_state))
            .await
        {
            tracing::warn!(error = %e, state = ?new_state, "Failed to send StateChanged event - receiver may be dropped");
        }
    }

    /// Sends a message to the signaling server.
    async fn send_message(&self, message: SignalingMessage) -> Result<()> {
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
    async fn handle_message(&self, message: SignalingMessage) {
        match message {
            SignalingMessage::Joined { room_id } => {
                {
                    let mut state = self.state.write().await;
                    state.current_room = Some(room_id.clone());
                }
                if let Err(e) = self
                    .event_tx
                    .send(SignalingEvent::RoomJoined {
                        room_id: room_id.clone(),
                    })
                    .await
                {
                    tracing::warn!(error = %e, room_id = %room_id, "Failed to send RoomJoined event - receiver may be dropped");
                }
            }
            SignalingMessage::Offer {
                sdp,
                target_device_id,
            } => {
                if let Err(e) = self
                    .event_tx
                    .send(SignalingEvent::OfferReceived {
                        sdp,
                        from_device_id: target_device_id,
                    })
                    .await
                {
                    tracing::warn!(error = %e, "Failed to send OfferReceived event - receiver may be dropped");
                }
            }
            SignalingMessage::Answer {
                sdp,
                target_device_id,
            } => {
                if let Err(e) = self
                    .event_tx
                    .send(SignalingEvent::AnswerReceived {
                        sdp,
                        from_device_id: target_device_id,
                    })
                    .await
                {
                    tracing::warn!(error = %e, "Failed to send AnswerReceived event - receiver may be dropped");
                }
            }
            SignalingMessage::Ice {
                candidate,
                sdp_mid,
                sdp_mline_index,
                target_device_id,
            } => {
                if let Err(e) = self
                    .event_tx
                    .send(SignalingEvent::IceCandidateReceived {
                        candidate,
                        sdp_mid,
                        sdp_mline_index,
                        from_device_id: target_device_id,
                    })
                    .await
                {
                    tracing::warn!(error = %e, "Failed to send IceCandidateReceived event - receiver may be dropped");
                }
            }
            SignalingMessage::PeerJoined { device_id } => {
                if let Err(e) = self
                    .event_tx
                    .send(SignalingEvent::PeerJoined {
                        device_id: device_id.clone(),
                    })
                    .await
                {
                    tracing::warn!(error = %e, device_id = %device_id, "Failed to send PeerJoined event - receiver may be dropped");
                }
            }
            SignalingMessage::PeerLeft { device_id } => {
                if let Err(e) = self
                    .event_tx
                    .send(SignalingEvent::PeerLeft {
                        device_id: device_id.clone(),
                    })
                    .await
                {
                    tracing::warn!(error = %e, device_id = %device_id, "Failed to send PeerLeft event - receiver may be dropped");
                }
            }
            SignalingMessage::Error { message } => {
                if let Err(e) = self
                    .event_tx
                    .send(SignalingEvent::Error {
                        message: message.clone(),
                    })
                    .await
                {
                    tracing::warn!(error = %e, signaling_error = %message, "Failed to send Error event - receiver may be dropped");
                }
            }
            _ => {
                // Ignore other message types (Join, Leave are outgoing only)
            }
        }
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

            // Attempt to connect
            self.set_state(ConnectionState::Connecting).await;

            match self.connect_internal().await {
                Ok((message_tx, mut ws_rx)) => {
                    // Connection successful
                    {
                        let mut state = self.state.write().await;
                        state.message_tx = Some(message_tx);
                        state.current_backoff = self.config.initial_backoff;
                    }
                    self.set_state(ConnectionState::Connected).await;

                    // Re-join room if we were in one
                    {
                        let state = self.state.read().await;
                        if let (Some(room_id), Some(device_id)) =
                            (&state.current_room, &state.current_device_id)
                        {
                            let msg = SignalingMessage::Join {
                                device_id: device_id.clone(),
                                room_id: room_id.clone(),
                            };
                            drop(state);
                            let _ = self.send_message(msg).await;
                        }
                    }

                    // Process incoming messages
                    loop {
                        tokio::select! {
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
    async fn connect_internal(
        &self,
    ) -> Result<(
        mpsc::Sender<SignalingMessage>,
        mpsc::Receiver<Result<SignalingMessage>>,
    )> {
        // Validate the URL format
        let _url = Url::parse(&self.config.server_url)
            .map_err(|e| ProtocolError::HandshakeFailed(format!("invalid signaling URL: {}", e)))?;

        // Pass the string directly to connect_async (it implements IntoClientRequest for &str)
        let (ws_stream, _) = connect_async(&self.config.server_url)
            .await
            .map_err(|e| match e {
                WsError::Io(io_err) => ProtocolError::from(io_err),
                _ => ProtocolError::ConnectionClosed(format!("WebSocket connection failed: {}", e)),
            })?;

        let (mut ws_sink, mut ws_stream) = ws_stream.split();

        // Create channels for message passing
        let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<SignalingMessage>(256);
        let (incoming_tx, incoming_rx) = mpsc::channel::<Result<SignalingMessage>>(256);

        // Spawn task to handle outgoing messages
        tokio::spawn(async move {
            while let Some(msg) = outgoing_rx.recv().await {
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
        });

        // Spawn task to handle incoming messages
        tokio::spawn(async move {
            while let Some(result) = ws_stream.next().await {
                match result {
                    Ok(WsMessage::Text(text)) => {
                        match serde_json::from_str::<SignalingMessage>(&text) {
                            Ok(msg) => {
                                if incoming_tx.send(Ok(msg)).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                tracing::warn!("failed to parse signaling message: {}", e);
                            }
                        }
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
                        // Ignore ping/pong/binary messages
                    }
                }
            }
        });

        Ok((outgoing_tx, incoming_rx))
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
            state.connection_state = ConnectionState::Disconnected;
            Ok(())
        })
    }

    fn join_room(
        &self,
        device_id: &str,
        room_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        let device_id = device_id.to_string();
        let room_id = room_id.to_string();

        Box::pin(async move {
            {
                let mut state = self.state.write().await;
                state.current_device_id = Some(device_id.clone());
                state.current_room = Some(room_id.clone());
            }

            self.send_message(SignalingMessage::Join { device_id, room_id })
                .await
        })
    }

    fn leave_room(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            {
                let mut state = self.state.write().await;
                state.current_room = None;
            }

            self.send_message(SignalingMessage::Leave).await
        })
    }

    fn send_offer(
        &self,
        sdp: &str,
        target_device_id: Option<&str>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        let sdp = sdp.to_string();
        let target_device_id = target_device_id.map(|s| s.to_string());

        Box::pin(async move {
            self.send_message(SignalingMessage::Offer {
                sdp,
                target_device_id,
            })
            .await
        })
    }

    fn send_answer(
        &self,
        sdp: &str,
        target_device_id: Option<&str>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        let sdp = sdp.to_string();
        let target_device_id = target_device_id.map(|s| s.to_string());

        Box::pin(async move {
            self.send_message(SignalingMessage::Answer {
                sdp,
                target_device_id,
            })
            .await
        })
    }

    fn send_ice_candidate(
        &self,
        candidate: &str,
        sdp_mid: Option<&str>,
        sdp_mline_index: Option<u16>,
        target_device_id: Option<&str>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        let candidate = candidate.to_string();
        let sdp_mid = sdp_mid.map(|s| s.to_string());
        let target_device_id = target_device_id.map(|s| s.to_string());

        Box::pin(async move {
            self.send_message(SignalingMessage::Ice {
                candidate,
                sdp_mid,
                sdp_mline_index,
                target_device_id,
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
        // In production, consider a broadcast channel instead
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
        assert_eq!(
            config.server_url,
            "wss://remoshell-signaling.moukrea.workers.dev"
        );
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
    fn test_signaling_message_serialization() {
        let join = SignalingMessage::Join {
            device_id: "device-123".to_string(),
            room_id: "room-456".to_string(),
        };
        let json = serde_json::to_string(&join).unwrap();
        assert!(json.contains("\"type\":\"join\""));
        assert!(json.contains("\"device_id\":\"device-123\""));
        assert!(json.contains("\"room_id\":\"room-456\""));

        let offer = SignalingMessage::Offer {
            sdp: "v=0\r\n...".to_string(),
            target_device_id: None,
        };
        let json = serde_json::to_string(&offer).unwrap();
        assert!(json.contains("\"type\":\"offer\""));
        assert!(json.contains("\"sdp\":\"v=0\\r\\n...\""));

        let answer = SignalingMessage::Answer {
            sdp: "v=0\r\n...".to_string(),
            target_device_id: Some("target-789".to_string()),
        };
        let json = serde_json::to_string(&answer).unwrap();
        assert!(json.contains("\"type\":\"answer\""));
        assert!(json.contains("\"target_device_id\":\"target-789\""));

        let ice = SignalingMessage::Ice {
            candidate: "candidate:1 1 UDP 2130706431 ...".to_string(),
            sdp_mid: Some("0".to_string()),
            sdp_mline_index: Some(0),
            target_device_id: None,
        };
        let json = serde_json::to_string(&ice).unwrap();
        assert!(json.contains("\"type\":\"ice\""));
        assert!(json.contains("\"candidate\":"));
    }

    #[test]
    fn test_signaling_message_deserialization() {
        let json = r#"{"type":"join","device_id":"dev-1","room_id":"room-1"}"#;
        let msg: SignalingMessage = serde_json::from_str(json).unwrap();
        match msg {
            SignalingMessage::Join { device_id, room_id } => {
                assert_eq!(device_id, "dev-1");
                assert_eq!(room_id, "room-1");
            }
            _ => panic!("unexpected message type"),
        }

        let json = r#"{"type":"offer","sdp":"test-sdp"}"#;
        let msg: SignalingMessage = serde_json::from_str(json).unwrap();
        match msg {
            SignalingMessage::Offer {
                sdp,
                target_device_id,
            } => {
                assert_eq!(sdp, "test-sdp");
                assert!(target_device_id.is_none());
            }
            _ => panic!("unexpected message type"),
        }

        let json = r#"{"type":"error","message":"room not found"}"#;
        let msg: SignalingMessage = serde_json::from_str(json).unwrap();
        match msg {
            SignalingMessage::Error { message } => {
                assert_eq!(message, "room not found");
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
            .send_message(SignalingMessage::Join {
                device_id: "test".to_string(),
                room_id: "room".to_string(),
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
        assert!(events.is_some(), "First call to events() should return Some");
        let _events = events.unwrap();

        // Second call should return None (already taken)
        let events_again = client.events();
        assert!(
            events_again.is_none(),
            "Second call to events() should return None"
        );

        // We can verify events are received by changing state
        client.set_state(ConnectionState::Connecting).await;

        // Note: In a real test, we would receive the event from the receiver
    }

    #[tokio::test]
    async fn test_join_room_stores_state() {
        let config = SignalingConfig::new("wss://localhost:8080");
        let client = WebSocketSignalingClient::new(config);

        // Join room (will fail to send but should store state)
        let _ = client.join_room("device-1", "room-1").await;

        // Verify state is stored
        let state = client.state.read().await;
        assert_eq!(state.current_device_id, Some("device-1".to_string()));
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

    /// Integration test for the signaling flow.
    ///
    /// Note: This test requires a running signaling server to pass.
    /// It is marked as ignore by default.
    #[tokio::test]
    #[ignore = "requires running signaling server"]
    async fn test_signaling_flow_integration() {
        let config = SignalingConfig::new("ws://localhost:8080/ws")
            .with_auto_reconnect(false)
            .with_initial_backoff(Duration::from_millis(100));

        let client = Arc::new(WebSocketSignalingClient::new(config));
        let mut events = client.events().expect("events() should return receiver on first call");

        // Start the client
        client.clone().start();

        // Wait for connection
        tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(event) = events.recv().await {
                if let SignalingEvent::StateChanged(ConnectionState::Connected) = event {
                    break;
                }
            }
        })
        .await
        .expect("connection timeout");

        // Join a room
        client.join_room("test-device", "test-room").await.unwrap();

        // Wait for room joined event
        tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(event) = events.recv().await {
                if let SignalingEvent::RoomJoined { room_id } = event {
                    assert_eq!(room_id, "test-room");
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
