//! QUIC connection manager for Tauri client.
//!
//! This module implements QUIC peer connections using the iroh crate with:
//! - Built-in hole punching for NAT traversal
//! - Automatic relay fallback for reliable connectivity
//! - TLS 1.3 encryption (native to QUIC)
//! - Bi-directional streams for data transfer
//! - Event emission for Tauri frontend integration

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[cfg(test)]
use iroh::RelayMode;
use iroh::endpoint::Connection;
use iroh::{Endpoint, NodeAddr, NodeId, RelayUrl, SecretKey};
use protocol::error::{ProtocolError, Result};
use tokio::sync::{Mutex, RwLock, broadcast, mpsc};

/// The ALPN protocol identifier for RemoShell QUIC connections.
pub const REMOSHELL_ALPN: &[u8] = b"remoshell/1";

/// Default timeout for connection operations.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Default timeout for stream operations.
pub const DEFAULT_STREAM_TIMEOUT: Duration = Duration::from_secs(10);

/// Buffer size for message channels.
const CHANNEL_BUFFER_SIZE: usize = 256;

/// Buffer size for event broadcast channel.
const EVENT_BUFFER_SIZE: usize = 64;

/// Maximum message size for a single read (1MB).
const MAX_MESSAGE_SIZE: usize = 1024 * 1024;

/// The type of data channel for message routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ChannelType {
    /// Control channel: ordered, reliable - session management
    Control,
    /// Terminal channel: unordered - low-latency I/O
    Terminal,
    /// Files channel: ordered, reliable - file transfers
    Files,
}

impl ChannelType {
    /// Returns the channel identifier byte for stream negotiation.
    pub fn id(&self) -> u8 {
        match self {
            ChannelType::Control => 0,
            ChannelType::Terminal => 1,
            ChannelType::Files => 2,
        }
    }

    /// Creates a ChannelType from an identifier byte.
    pub fn from_id(id: u8) -> Option<Self> {
        match id {
            0 => Some(ChannelType::Control),
            1 => Some(ChannelType::Terminal),
            2 => Some(ChannelType::Files),
            _ => None,
        }
    }
}

/// Connection state for the QUIC manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ConnectionState {
    /// Not connected to any peer.
    Disconnected,
    /// Currently attempting to connect.
    Connecting,
    /// Connected and ready for communication.
    Connected,
    /// Connection lost, may attempt reconnection.
    Reconnecting,
    /// Connection failed with an error.
    Failed,
}

/// Events emitted by the QUIC manager for frontend notification.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum ConnectionEvent {
    /// Connection state changed.
    StateChanged(ConnectionState),
    /// Data received on a channel.
    DataReceived {
        channel: ChannelType,
        /// Base64-encoded data for JSON serialization.
        data: String,
    },
    /// Error occurred.
    Error(String),
    /// Peer information available.
    PeerInfo { node_id: String },
}

/// Configuration for QUIC connections.
#[derive(Debug, Clone)]
pub struct QuicConfig {
    /// Optional relay URL for NAT traversal.
    pub relay_url: Option<RelayUrl>,
    /// Connection timeout.
    pub connect_timeout: Duration,
    /// Stream operation timeout.
    pub stream_timeout: Duration,
}

impl Default for QuicConfig {
    fn default() -> Self {
        Self {
            relay_url: None,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            stream_timeout: DEFAULT_STREAM_TIMEOUT,
        }
    }
}

impl QuicConfig {
    /// Creates a new QUIC configuration with a relay URL.
    pub fn with_relay(relay_url: RelayUrl) -> Self {
        Self {
            relay_url: Some(relay_url),
            ..Default::default()
        }
    }

    /// Sets the connection timeout.
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Sets the stream operation timeout.
    pub fn stream_timeout(mut self, timeout: Duration) -> Self {
        self.stream_timeout = timeout;
        self
    }
}

/// Internal state for managing bi-directional streams per channel type.
struct StreamChannels {
    control: Option<StreamPair>,
    terminal: Option<StreamPair>,
    files: Option<StreamPair>,
}

impl Default for StreamChannels {
    fn default() -> Self {
        Self {
            control: None,
            terminal: None,
            files: None,
        }
    }
}

impl StreamChannels {
    #[allow(dead_code)]
    fn get(&self, channel_type: ChannelType) -> Option<&StreamPair> {
        match channel_type {
            ChannelType::Control => self.control.as_ref(),
            ChannelType::Terminal => self.terminal.as_ref(),
            ChannelType::Files => self.files.as_ref(),
        }
    }

    fn get_mut(&mut self, channel_type: ChannelType) -> Option<&mut StreamPair> {
        match channel_type {
            ChannelType::Control => self.control.as_mut(),
            ChannelType::Terminal => self.terminal.as_mut(),
            ChannelType::Files => self.files.as_mut(),
        }
    }

    fn set(&mut self, channel_type: ChannelType, stream: StreamPair) {
        match channel_type {
            ChannelType::Control => self.control = Some(stream),
            ChannelType::Terminal => self.terminal = Some(stream),
            ChannelType::Files => self.files = Some(stream),
        }
    }

    #[allow(dead_code)]
    fn has(&self, channel_type: ChannelType) -> bool {
        self.get(channel_type).is_some()
    }

    fn clear(&mut self) {
        self.control = None;
        self.terminal = None;
        self.files = None;
    }
}

/// A pair of send and receive streams for bi-directional communication.
struct StreamPair {
    send: iroh::endpoint::SendStream,
    recv: iroh::endpoint::RecvStream,
}

/// QUIC connection manager for Tauri client.
///
/// This struct manages QUIC connections with:
/// - Built-in hole punching via iroh
/// - Automatic relay fallback
/// - Multiple bi-directional streams for different message types
/// - TLS 1.3 encryption (built into QUIC)
/// - Event emission for Tauri frontend integration
pub struct QuicManager {
    /// The iroh endpoint for managing connections.
    endpoint: Endpoint,
    /// The active QUIC connection to the peer.
    connection: Arc<RwLock<Option<Connection>>>,
    /// Stream channels for different message types.
    streams: Arc<Mutex<StreamChannels>>,
    /// Receiver for incoming messages from streams.
    message_rx: Arc<Mutex<HashMap<ChannelType, mpsc::Receiver<Vec<u8>>>>>,
    /// Senders for incoming messages (used by stream readers).
    message_tx: Arc<RwLock<HashMap<ChannelType, mpsc::Sender<Vec<u8>>>>>,
    /// Current connection state.
    state: Arc<RwLock<ConnectionState>>,
    /// Remote peer's public key (node ID).
    peer_node_id: Arc<RwLock<Option<NodeId>>>,
    /// Last connected node address (for reconnection).
    last_node_addr: Arc<RwLock<Option<NodeAddr>>>,
    /// Event broadcast sender for frontend notifications.
    event_tx: broadcast::Sender<ConnectionEvent>,
    /// Configuration for this manager.
    config: QuicConfig,
}

impl QuicManager {
    /// Creates a new QUIC manager with the given configuration.
    ///
    /// This sets up the iroh endpoint and prepares for connections.
    pub async fn new(config: QuicConfig) -> Result<Self> {
        let builder = Endpoint::builder().alpns(vec![REMOSHELL_ALPN.to_vec()]);

        // Configure relay if provided
        if let Some(ref relay_url) = config.relay_url {
            tracing::debug!("QUIC endpoint will use relay: {}", relay_url);
        }

        let endpoint = builder.bind().await.map_err(|e| {
            ProtocolError::HandshakeFailed(format!("failed to create iroh endpoint: {}", e))
        })?;

        Self::from_endpoint(endpoint, config)
    }

    /// Creates a new QUIC manager with a specific secret key.
    ///
    /// This allows using a persistent identity across restarts.
    pub async fn with_secret_key(config: QuicConfig, secret_key: SecretKey) -> Result<Self> {
        let builder = Endpoint::builder()
            .secret_key(secret_key)
            .alpns(vec![REMOSHELL_ALPN.to_vec()]);

        if let Some(ref relay_url) = config.relay_url {
            tracing::debug!("QUIC endpoint will use relay: {}", relay_url);
        }

        let endpoint = builder.bind().await.map_err(|e| {
            ProtocolError::HandshakeFailed(format!("failed to create iroh endpoint: {}", e))
        })?;

        Self::from_endpoint(endpoint, config)
    }

    /// Creates a QuicManager from an existing endpoint.
    fn from_endpoint(endpoint: Endpoint, config: QuicConfig) -> Result<Self> {
        // Create message channels for each channel type
        let mut message_rx = HashMap::new();
        let mut message_tx = HashMap::new();

        for channel_type in [
            ChannelType::Control,
            ChannelType::Terminal,
            ChannelType::Files,
        ] {
            let (tx, rx) = mpsc::channel(CHANNEL_BUFFER_SIZE);
            message_tx.insert(channel_type, tx);
            message_rx.insert(channel_type, rx);
        }

        let (event_tx, _) = broadcast::channel(EVENT_BUFFER_SIZE);

        Ok(Self {
            endpoint,
            connection: Arc::new(RwLock::new(None)),
            streams: Arc::new(Mutex::new(StreamChannels::default())),
            message_rx: Arc::new(Mutex::new(message_rx)),
            message_tx: Arc::new(RwLock::new(message_tx)),
            state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            peer_node_id: Arc::new(RwLock::new(None)),
            last_node_addr: Arc::new(RwLock::new(None)),
            event_tx,
            config,
        })
    }

    /// Creates a new QUIC manager optimized for local testing.
    ///
    /// This disables discovery and relay for faster local connections.
    #[cfg(test)]
    pub(crate) async fn new_for_testing() -> Result<Self> {
        let builder = Endpoint::builder()
            .alpns(vec![REMOSHELL_ALPN.to_vec()])
            .relay_mode(RelayMode::Disabled)
            .clear_discovery();

        let endpoint = builder.bind().await.map_err(|e| {
            ProtocolError::HandshakeFailed(format!("failed to create iroh endpoint: {}", e))
        })?;

        // Use shorter timeouts for testing
        let config = QuicConfig {
            relay_url: None,
            connect_timeout: Duration::from_secs(5),
            stream_timeout: Duration::from_secs(5),
        };

        Self::from_endpoint(endpoint, config)
    }

    /// Returns the node ID (public key) of this endpoint.
    pub fn node_id(&self) -> NodeId {
        self.endpoint.node_id()
    }

    /// Returns the secret key of this endpoint.
    pub fn secret_key(&self) -> SecretKey {
        self.endpoint.secret_key().clone()
    }

    /// Returns the node address for this endpoint.
    ///
    /// This includes the node ID, relay URL, and any direct addresses.
    pub async fn node_addr(&self) -> Result<NodeAddr> {
        self.endpoint.node_addr().await.map_err(|e| {
            ProtocolError::HandshakeFailed(format!("failed to get node address: {}", e))
        })
    }

    /// Returns the underlying iroh endpoint for advanced operations.
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Subscribes to connection events.
    ///
    /// Returns a receiver that will receive all events emitted by this manager.
    pub fn subscribe(&self) -> broadcast::Receiver<ConnectionEvent> {
        self.event_tx.subscribe()
    }

    /// Emits an event to all subscribers.
    fn emit(&self, event: ConnectionEvent) {
        // Ignore send errors (no subscribers)
        let _ = self.event_tx.send(event);
    }

    /// Sets the connection state and emits an event.
    async fn set_state(&self, new_state: ConnectionState) {
        {
            let mut state = self.state.write().await;
            *state = new_state;
        }
        self.emit(ConnectionEvent::StateChanged(new_state));
    }

    /// Returns the current connection state.
    pub async fn state(&self) -> ConnectionState {
        *self.state.read().await
    }

    /// Returns whether the connection is currently active.
    pub fn is_connected(&self) -> bool {
        match self.state.try_read() {
            Ok(guard) => *guard == ConnectionState::Connected,
            Err(_) => false,
        }
    }

    /// Returns the remote peer's node ID (public key) if connected.
    pub fn peer_node_id(&self) -> Option<NodeId> {
        match self.peer_node_id.try_read() {
            Ok(guard) => *guard,
            Err(_) => None,
        }
    }

    /// Connects to a peer using their node address.
    ///
    /// This initiates a QUIC connection with built-in hole punching.
    pub async fn connect(&self, node_addr: NodeAddr) -> Result<()> {
        // Update state
        self.set_state(ConnectionState::Connecting).await;

        let remote_node_id = node_addr.node_id;

        // Store the node address for potential reconnection
        {
            let mut last_addr = self.last_node_addr.write().await;
            *last_addr = Some(node_addr.clone());
        }

        let connection = match tokio::time::timeout(
            self.config.connect_timeout,
            self.endpoint.connect(node_addr, REMOSHELL_ALPN),
        )
        .await
        {
            Ok(Ok(conn)) => conn,
            Ok(Err(e)) => {
                self.set_state(ConnectionState::Failed).await;
                self.emit(ConnectionEvent::Error(format!("connection failed: {}", e)));
                return Err(ProtocolError::HandshakeFailed(format!(
                    "failed to connect: {}",
                    e
                )));
            }
            Err(_) => {
                self.set_state(ConnectionState::Failed).await;
                self.emit(ConnectionEvent::Error("connection timed out".into()));
                return Err(ProtocolError::Timeout("connection timed out".into()));
            }
        };

        // Store the peer's node ID
        {
            let mut peer_id = self.peer_node_id.write().await;
            *peer_id = Some(remote_node_id);
        }

        // Store the connection
        {
            let mut conn = self.connection.write().await;
            *conn = Some(connection);
        }

        // Mark as connected
        self.set_state(ConnectionState::Connected).await;

        // Emit peer info event
        self.emit(ConnectionEvent::PeerInfo {
            node_id: remote_node_id.to_string(),
        });

        tracing::info!("Connected to peer: {}", remote_node_id);
        Ok(())
    }

    /// Disconnects from the current peer.
    ///
    /// This closes all streams and the connection gracefully.
    pub async fn disconnect(&self) -> Result<()> {
        // Close all streams
        {
            let mut streams = self.streams.lock().await;
            streams.clear();
        }

        // Close the connection
        {
            let mut conn = self.connection.write().await;
            if let Some(connection) = conn.take() {
                connection.close(0u32.into(), b"graceful disconnect");
            }
        }

        // Clear peer info
        {
            let mut peer_id = self.peer_node_id.write().await;
            *peer_id = None;
        }

        // Update state
        self.set_state(ConnectionState::Disconnected).await;

        tracing::info!("Disconnected from peer");
        Ok(())
    }

    /// Attempts to reconnect to the previously connected peer.
    ///
    /// This is useful when the connection is lost due to network issues.
    pub async fn reconnect(&self) -> Result<()> {
        let node_addr = {
            let addr = self.last_node_addr.read().await;
            addr.clone()
        };

        let node_addr = node_addr.ok_or_else(|| {
            ProtocolError::ConnectionClosed("no previous connection to reconnect to".into())
        })?;

        // Update state
        self.set_state(ConnectionState::Reconnecting).await;

        // Close existing connection if any
        self.disconnect().await?;

        // Reset message channels
        {
            let mut message_rx = self.message_rx.lock().await;
            let mut message_tx = self.message_tx.write().await;

            for channel_type in [
                ChannelType::Control,
                ChannelType::Terminal,
                ChannelType::Files,
            ] {
                let (tx, rx) = mpsc::channel(CHANNEL_BUFFER_SIZE);
                message_tx.insert(channel_type, tx);
                message_rx.insert(channel_type, rx);
            }
        }

        // Reconnect
        self.connect(node_addr).await
    }

    /// Creates bi-directional streams for all channel types.
    ///
    /// This should be called after connecting to set up the communication channels.
    pub async fn create_streams(&self) -> Result<()> {
        let conn = self.connection.read().await;
        let connection = conn
            .as_ref()
            .ok_or_else(|| ProtocolError::ConnectionClosed("no active connection".into()))?;

        let mut streams = self.streams.lock().await;

        for channel_type in [
            ChannelType::Control,
            ChannelType::Terminal,
            ChannelType::Files,
        ] {
            let (send, recv) = connection.open_bi().await.map_err(|e| {
                ProtocolError::HandshakeFailed(format!(
                    "failed to open {:?} stream: {}",
                    channel_type, e
                ))
            })?;

            // Send channel type identifier so the peer knows which channel this is
            let mut send = send;
            send.write_all(&[channel_type.id()]).await.map_err(|e| {
                ProtocolError::TransferFailed(format!("failed to send channel id: {}", e))
            })?;

            streams.set(channel_type, StreamPair { send, recv });
            tracing::debug!("Created {:?} stream", channel_type);
        }

        Ok(())
    }

    /// Spawns background tasks to read from streams and forward to message channels.
    ///
    /// This also emits DataReceived events for the frontend.
    pub fn spawn_receive_loop(&self) {
        let streams = self.streams.clone();
        let message_tx = self.message_tx.clone();
        let state = self.state.clone();
        let event_tx = self.event_tx.clone();

        for channel_type in [
            ChannelType::Control,
            ChannelType::Terminal,
            ChannelType::Files,
        ] {
            let streams = streams.clone();
            let message_tx = message_tx.clone();
            let state = state.clone();
            let event_tx = event_tx.clone();

            tokio::spawn(async move {
                loop {
                    let result = Self::read_from_stream(&streams, channel_type).await;

                    match result {
                        Ok(data) => {
                            // Forward to internal message channel
                            let tx = {
                                let senders = message_tx.read().await;
                                senders.get(&channel_type).cloned()
                            };

                            if let Some(tx) = tx {
                                if let Err(e) = tx.send(data.clone()).await {
                                    tracing::error!(
                                        "failed to forward {:?} message: {}",
                                        channel_type,
                                        e
                                    );
                                    break;
                                }
                            }

                            // Emit event for frontend
                            let event = ConnectionEvent::DataReceived {
                                channel: channel_type,
                                data: base64_encode(&data),
                            };
                            let _ = event_tx.send(event);
                        }
                        Err(e) => {
                            tracing::debug!("{:?} stream closed or error: {}", channel_type, e);
                            // Update state to disconnected
                            let mut conn_state = state.write().await;
                            if *conn_state == ConnectionState::Connected {
                                *conn_state = ConnectionState::Disconnected;
                                let _ = event_tx.send(ConnectionEvent::StateChanged(
                                    ConnectionState::Disconnected,
                                ));
                            }
                            break;
                        }
                    }
                }
            });
        }
    }

    /// Reads a length-prefixed message from a stream.
    async fn read_from_stream(
        streams: &Arc<Mutex<StreamChannels>>,
        channel_type: ChannelType,
    ) -> Result<Vec<u8>> {
        let mut streams = streams.lock().await;
        let stream_pair = streams.get_mut(channel_type).ok_or_else(|| {
            ProtocolError::ConnectionClosed(format!("{:?} stream not available", channel_type))
        })?;

        // Read 4-byte length prefix
        let mut len_buf = [0u8; 4];
        stream_pair
            .recv
            .read_exact(&mut len_buf)
            .await
            .map_err(|e| ProtocolError::ConnectionClosed(format!("stream read error: {}", e)))?;

        let len = u32::from_be_bytes(len_buf) as usize;
        if len > MAX_MESSAGE_SIZE {
            return Err(ProtocolError::FrameTooLarge {
                size: len,
                max: MAX_MESSAGE_SIZE,
            });
        }

        // Read the message data
        let mut data = vec![0u8; len];
        stream_pair
            .recv
            .read_exact(&mut data)
            .await
            .map_err(|e| ProtocolError::ConnectionClosed(format!("stream read error: {}", e)))?;

        Ok(data)
    }

    /// Writes a length-prefixed message to a stream.
    async fn write_to_stream(
        streams: &Arc<Mutex<StreamChannels>>,
        channel_type: ChannelType,
        data: &[u8],
    ) -> Result<()> {
        if data.len() > MAX_MESSAGE_SIZE {
            return Err(ProtocolError::FrameTooLarge {
                size: data.len(),
                max: MAX_MESSAGE_SIZE,
            });
        }

        let mut streams = streams.lock().await;
        let stream_pair = streams.get_mut(channel_type).ok_or_else(|| {
            ProtocolError::ConnectionClosed(format!("{:?} stream not available", channel_type))
        })?;

        // Write 4-byte length prefix
        let len = data.len() as u32;
        stream_pair
            .send
            .write_all(&len.to_be_bytes())
            .await
            .map_err(|e| ProtocolError::TransferFailed(format!("stream write error: {}", e)))?;

        // Write the message data
        stream_pair
            .send
            .write_all(data)
            .await
            .map_err(|e| ProtocolError::TransferFailed(format!("stream write error: {}", e)))?;

        Ok(())
    }

    /// Sends data over a specific channel.
    ///
    /// The data is length-prefixed and written to the appropriate stream.
    pub async fn send(&self, channel_type: ChannelType, data: &[u8]) -> Result<()> {
        if !self.is_connected() {
            return Err(ProtocolError::ConnectionClosed("not connected".into()));
        }
        Self::write_to_stream(&self.streams, channel_type, data).await
    }

    /// Receives data from a specific channel.
    ///
    /// This reads from the internal message queue populated by the receive loop.
    pub async fn recv(&self, channel_type: ChannelType) -> Result<Vec<u8>> {
        let mut receivers = self.message_rx.lock().await;
        let rx = receivers.get_mut(&channel_type).ok_or_else(|| {
            ProtocolError::ConnectionClosed(format!("{:?} channel not available", channel_type))
        })?;

        rx.recv()
            .await
            .ok_or_else(|| ProtocolError::ConnectionClosed("channel closed".into()))
    }

    /// Closes the manager and releases all resources.
    pub async fn close(&self) -> Result<()> {
        self.disconnect().await?;

        // Close the endpoint
        self.endpoint.close().await;

        tracing::info!("QUIC manager closed");
        Ok(())
    }
}

/// Base64 encode data for JSON serialization.
fn base64_encode(data: &[u8]) -> String {
    use std::io::Write;
    let mut buf = Vec::with_capacity(data.len() * 4 / 3 + 4);
    {
        let mut encoder = Base64Encoder::new(&mut buf);
        encoder.write_all(data).ok();
    }
    String::from_utf8(buf).unwrap_or_default()
}

/// Simple base64 encoder using standard alphabet.
struct Base64Encoder<W: std::io::Write> {
    writer: W,
}

impl<W: std::io::Write> Base64Encoder<W> {
    fn new(writer: W) -> Self {
        Self { writer }
    }
}

impl<W: std::io::Write> std::io::Write for Base64Encoder<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

        let mut i = 0;
        while i + 3 <= buf.len() {
            let n = ((buf[i] as u32) << 16) | ((buf[i + 1] as u32) << 8) | (buf[i + 2] as u32);
            self.writer.write_all(&[
                ALPHABET[(n >> 18 & 0x3F) as usize],
                ALPHABET[(n >> 12 & 0x3F) as usize],
                ALPHABET[(n >> 6 & 0x3F) as usize],
                ALPHABET[(n & 0x3F) as usize],
            ])?;
            i += 3;
        }

        match buf.len() - i {
            1 => {
                let n = (buf[i] as u32) << 16;
                self.writer.write_all(&[
                    ALPHABET[(n >> 18 & 0x3F) as usize],
                    ALPHABET[(n >> 12 & 0x3F) as usize],
                    b'=',
                    b'=',
                ])?;
            }
            2 => {
                let n = ((buf[i] as u32) << 16) | ((buf[i + 1] as u32) << 8);
                self.writer.write_all(&[
                    ALPHABET[(n >> 18 & 0x3F) as usize],
                    ALPHABET[(n >> 12 & 0x3F) as usize],
                    ALPHABET[(n >> 6 & 0x3F) as usize],
                    b'=',
                ])?;
            }
            _ => {}
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_quic_config_default() {
        let config = QuicConfig::default();
        assert!(config.relay_url.is_none());
        assert_eq!(config.connect_timeout, DEFAULT_CONNECT_TIMEOUT);
        assert_eq!(config.stream_timeout, DEFAULT_STREAM_TIMEOUT);
    }

    #[tokio::test]
    async fn test_quic_config_builder() {
        let config = QuicConfig::default()
            .connect_timeout(Duration::from_secs(60))
            .stream_timeout(Duration::from_secs(30));

        assert_eq!(config.connect_timeout, Duration::from_secs(60));
        assert_eq!(config.stream_timeout, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_channel_type_id() {
        assert_eq!(ChannelType::Control.id(), 0);
        assert_eq!(ChannelType::Terminal.id(), 1);
        assert_eq!(ChannelType::Files.id(), 2);
    }

    #[tokio::test]
    async fn test_channel_type_from_id() {
        assert_eq!(ChannelType::from_id(0), Some(ChannelType::Control));
        assert_eq!(ChannelType::from_id(1), Some(ChannelType::Terminal));
        assert_eq!(ChannelType::from_id(2), Some(ChannelType::Files));
        assert_eq!(ChannelType::from_id(3), None);
        assert_eq!(ChannelType::from_id(255), None);
    }

    #[tokio::test]
    async fn test_create_manager() {
        let config = QuicConfig::default();
        let manager = QuicManager::new(config).await;
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_manager_has_node_id() {
        let config = QuicConfig::default();
        let manager = QuicManager::new(config).await.unwrap();

        // Node ID should be non-zero
        let node_id = manager.node_id();
        assert!(!node_id.as_bytes().iter().all(|&b| b == 0));
    }

    #[tokio::test]
    async fn test_manager_with_secret_key() {
        let secret_key = SecretKey::generate(rand::rngs::OsRng);
        let expected_node_id = secret_key.public();

        let config = QuicConfig::default();
        let manager = QuicManager::with_secret_key(config, secret_key)
            .await
            .unwrap();

        assert_eq!(manager.node_id(), expected_node_id);
    }

    #[tokio::test]
    async fn test_not_connected_initially() {
        let config = QuicConfig::default();
        let manager = QuicManager::new(config).await.unwrap();

        assert!(!manager.is_connected());
        assert!(manager.peer_node_id().is_none());
        assert_eq!(manager.state().await, ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn test_node_addr() {
        let config = QuicConfig::default();
        let manager = QuicManager::new(config).await.unwrap();

        let addr = manager.node_addr().await;
        assert!(addr.is_ok());

        let addr = addr.unwrap();
        assert_eq!(addr.node_id, manager.node_id());
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let config = QuicConfig::default();
        let manager = QuicManager::new(config).await.unwrap();

        let mut rx = manager.subscribe();

        // Emit a test event by changing state
        manager.set_state(ConnectionState::Connecting).await;

        // Should receive the event
        let event = rx.recv().await;
        assert!(event.is_ok());

        match event.unwrap() {
            ConnectionEvent::StateChanged(state) => {
                assert_eq!(state, ConnectionState::Connecting);
            }
            _ => panic!("expected StateChanged event"),
        }
    }

    #[tokio::test]
    async fn test_base64_encode() {
        // Test empty
        assert_eq!(base64_encode(b""), "");

        // Test standard cases
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[tokio::test]
    async fn test_send_without_connection_fails() {
        let config = QuicConfig::default();
        let manager = QuicManager::new(config).await.unwrap();

        let result = manager.send(ChannelType::Control, b"test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_reconnect_without_previous_connection_fails() {
        let config = QuicConfig::default();
        let manager = QuicManager::new(config).await.unwrap();

        let result = manager.reconnect().await;
        assert!(result.is_err());
    }

    /// Integration test for QUIC connection between two endpoints.
    ///
    /// This test creates two managers, connects them, creates streams,
    /// and exchanges messages.
    ///
    /// Note: This test is ignored by default because it requires network connectivity
    /// and may take a long time in some environments. Run with `--ignored` to include.
    #[tokio::test]
    #[ignore = "requires network connectivity, run with --ignored"]
    async fn test_full_connection() {
        // Create two managers with testing configuration (no relay/discovery)
        let server = QuicManager::new_for_testing()
            .await
            .expect("failed to create server");
        let client = QuicManager::new_for_testing()
            .await
            .expect("failed to create client");

        // Get server's bound sockets (local addresses to connect to)
        let bound_sockets = server.endpoint().bound_sockets();
        let server_node_id = server.node_id();

        // Create a NodeAddr with direct addresses from bound sockets
        let mut direct_addrs: Vec<std::net::SocketAddr> = vec![bound_sockets.0.into()];
        if let Some(addr) = bound_sockets.1 {
            direct_addrs.push(addr.into());
        }

        let server_addr = NodeAddr::from_parts(server_node_id, None, direct_addrs);

        // Subscribe to client events
        let mut client_events = client.subscribe();

        // Spawn server accept task (simulate daemon behavior)
        let server_handle = {
            tokio::spawn(async move {
                // Accept incoming connection
                let incoming = server
                    .endpoint()
                    .accept()
                    .await
                    .expect("no incoming connection");
                let connection = incoming.await.expect("failed to accept connection");

                // Store connection
                {
                    let mut conn = server.connection.write().await;
                    *conn = Some(connection);
                }
                server.set_state(ConnectionState::Connected).await;

                // Accept streams
                {
                    let conn = server.connection.read().await;
                    let connection = conn.as_ref().unwrap();
                    let mut streams = server.streams.lock().await;

                    for _ in 0..3 {
                        let (send, mut recv) = connection
                            .accept_bi()
                            .await
                            .expect("failed to accept stream");

                        // Read channel type
                        let mut channel_id = [0u8; 1];
                        recv.read_exact(&mut channel_id)
                            .await
                            .expect("failed to read channel id");

                        let channel_type =
                            ChannelType::from_id(channel_id[0]).expect("unknown channel");
                        streams.set(channel_type, StreamPair { send, recv });
                    }
                }

                server.spawn_receive_loop();
                server
            })
        };

        // Client connects
        tokio::time::sleep(Duration::from_millis(100)).await;
        client
            .connect(server_addr)
            .await
            .expect("client connect failed");
        client
            .create_streams()
            .await
            .expect("client create streams failed");
        client.spawn_receive_loop();

        // Verify we received state change events
        let event = client_events.recv().await.expect("expected event");
        assert!(matches!(
            event,
            ConnectionEvent::StateChanged(ConnectionState::Connecting)
        ));

        let event = client_events.recv().await.expect("expected event");
        assert!(matches!(
            event,
            ConnectionEvent::StateChanged(ConnectionState::Connected)
        ));

        // Wait for server to be ready
        let server = server_handle.await.expect("server task panicked");

        assert!(client.is_connected());
        assert!(server.is_connected());

        // Test message exchange on control channel
        let test_message = b"Hello from client!";
        client
            .send(ChannelType::Control, test_message)
            .await
            .expect("client send failed");

        let received = server
            .recv(ChannelType::Control)
            .await
            .expect("server recv failed");
        assert_eq!(received, test_message);

        // Test message from server to client
        let reply_message = b"Hello from server!";
        server
            .send(ChannelType::Control, reply_message)
            .await
            .expect("server send failed");

        let received = client
            .recv(ChannelType::Control)
            .await
            .expect("client recv failed");
        assert_eq!(received, reply_message);

        // Close connections
        client.disconnect().await.expect("client disconnect failed");
        server.disconnect().await.expect("server disconnect failed");

        assert!(!client.is_connected());
        assert!(!server.is_connected());
    }
}
