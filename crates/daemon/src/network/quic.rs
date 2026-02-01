//! QUIC connection handler for native Tauri clients.
//!
//! This module implements QUIC peer connections using the iroh crate with:
//! - Built-in hole punching for NAT traversal
//! - Automatic relay fallback for reliable connectivity
//! - TLS 1.3 encryption (native to QUIC)
//! - Bi-directional streams for data transfer

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use iroh::endpoint::Connection;
#[cfg(test)]
use iroh::RelayMode;
use iroh::{Endpoint, NodeAddr, NodeId, RelayUrl, SecretKey};
use protocol::error::{ProtocolError, Result};
use tokio::sync::{mpsc, Mutex, RwLock};

use super::{ChannelType, Connection as ConnectionTrait};

/// The ALPN protocol identifier for RemoShell QUIC connections.
pub const REMOSHELL_ALPN: &[u8] = b"remoshell/1";

/// Default timeout for connection operations.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Default timeout for stream operations.
pub const DEFAULT_STREAM_TIMEOUT: Duration = Duration::from_secs(10);

/// Buffer size for message channels.
const CHANNEL_BUFFER_SIZE: usize = 256;

/// Maximum message size for a single read (1MB).
const MAX_MESSAGE_SIZE: usize = 1024 * 1024;

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
#[derive(Default)]
struct StreamChannels {
    control: Option<StreamPair>,
    terminal: Option<StreamPair>,
    files: Option<StreamPair>,
}

impl StreamChannels {
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

    fn has(&self, channel_type: ChannelType) -> bool {
        self.get(channel_type).is_some()
    }
}

/// A pair of send and receive streams for bi-directional communication.
struct StreamPair {
    send: iroh::endpoint::SendStream,
    recv: iroh::endpoint::RecvStream,
}

/// QUIC connection handler for native Tauri clients.
///
/// This struct manages a QUIC connection with:
/// - Built-in hole punching via iroh
/// - Automatic relay fallback
/// - Multiple bi-directional streams for different message types
/// - TLS 1.3 encryption (built into QUIC)
pub struct QuicConnectionHandler {
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
    /// Whether the connection is established.
    connected: Arc<RwLock<bool>>,
    /// Remote peer's public key (node ID).
    peer_node_id: Arc<RwLock<Option<NodeId>>>,
    /// Configuration for this handler.
    config: QuicConfig,
}

impl QuicConnectionHandler {
    /// Creates a new QUIC connection handler with the given configuration.
    ///
    /// This sets up the iroh endpoint and prepares for connections.
    pub async fn new(config: QuicConfig) -> Result<Self> {
        let builder = Endpoint::builder().alpns(vec![REMOSHELL_ALPN.to_vec()]);

        // Configure relay if provided
        if let Some(ref relay_url) = config.relay_url {
            // The relay URL is used during connection, not endpoint creation
            tracing::debug!("QUIC endpoint will use relay: {}", relay_url);
        }

        let endpoint = builder.bind().await.map_err(|e| {
            ProtocolError::HandshakeFailed(format!("failed to create iroh endpoint: {}", e))
        })?;

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

        Ok(Self {
            endpoint,
            connection: Arc::new(RwLock::new(None)),
            streams: Arc::new(Mutex::new(StreamChannels::default())),
            message_rx: Arc::new(Mutex::new(message_rx)),
            message_tx: Arc::new(RwLock::new(message_tx)),
            connected: Arc::new(RwLock::new(false)),
            peer_node_id: Arc::new(RwLock::new(None)),
            config,
        })
    }

    /// Creates a new QUIC connection handler with a specific secret key.
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

        Ok(Self {
            endpoint,
            connection: Arc::new(RwLock::new(None)),
            streams: Arc::new(Mutex::new(StreamChannels::default())),
            message_rx: Arc::new(Mutex::new(message_rx)),
            message_tx: Arc::new(RwLock::new(message_tx)),
            connected: Arc::new(RwLock::new(false)),
            peer_node_id: Arc::new(RwLock::new(None)),
            config,
        })
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

    /// Accepts an incoming connection from a peer.
    ///
    /// This method blocks until a connection is received or the endpoint is closed.
    pub async fn accept(&self) -> Result<()> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or_else(|| ProtocolError::ConnectionClosed("endpoint closed".into()))?;

        let connection = incoming.await.map_err(|e| {
            ProtocolError::HandshakeFailed(format!("failed to accept connection: {}", e))
        })?;

        // Store the peer's node ID
        let remote_node_id = connection.remote_node_id().map_err(|e| {
            ProtocolError::HandshakeFailed(format!("failed to get remote node id: {}", e))
        })?;
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
        {
            let mut connected = self.connected.write().await;
            *connected = true;
        }

        tracing::info!("Accepted QUIC connection from peer: {}", remote_node_id);
        Ok(())
    }

    /// Connects to a peer using their node address.
    ///
    /// This initiates a QUIC connection with built-in hole punching.
    pub async fn connect(&self, node_addr: NodeAddr) -> Result<()> {
        let remote_node_id = node_addr.node_id;

        let connection = tokio::time::timeout(
            self.config.connect_timeout,
            self.endpoint.connect(node_addr, REMOSHELL_ALPN),
        )
        .await
        .map_err(|_| ProtocolError::Timeout("connection timed out".into()))?
        .map_err(|e| ProtocolError::HandshakeFailed(format!("failed to connect: {}", e)))?;

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
        {
            let mut connected = self.connected.write().await;
            *connected = true;
        }

        tracing::info!("Connected to peer: {}", remote_node_id);
        Ok(())
    }

    /// Creates bi-directional streams for all channel types.
    ///
    /// This should be called by the connection initiator after connecting.
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
            let channel_id = match channel_type {
                ChannelType::Control => 0u8,
                ChannelType::Terminal => 1u8,
                ChannelType::Files => 2u8,
            };
            send.write_all(&[channel_id]).await.map_err(|e| {
                ProtocolError::TransferFailed(format!("failed to send channel id: {}", e))
            })?;

            streams.set(channel_type, StreamPair { send, recv });
            tracing::debug!("Created {:?} stream", channel_type);
        }

        Ok(())
    }

    /// Accepts bi-directional streams for all channel types.
    ///
    /// This should be called by the connection acceptor after accepting.
    pub async fn accept_streams(&self) -> Result<()> {
        let conn = self.connection.read().await;
        let connection = conn
            .as_ref()
            .ok_or_else(|| ProtocolError::ConnectionClosed("no active connection".into()))?;

        let mut streams = self.streams.lock().await;
        let mut accepted_count = 0;

        while accepted_count < 3 {
            let (send, mut recv) =
                tokio::time::timeout(self.config.stream_timeout, connection.accept_bi())
                    .await
                    .map_err(|_| ProtocolError::Timeout("stream acceptance timed out".into()))?
                    .map_err(|e| {
                        ProtocolError::HandshakeFailed(format!("failed to accept stream: {}", e))
                    })?;

            // Read channel type identifier
            let mut channel_id = [0u8; 1];
            recv.read_exact(&mut channel_id).await.map_err(|e| {
                ProtocolError::TransferFailed(format!("failed to read channel id: {}", e))
            })?;

            let channel_type = match channel_id[0] {
                0 => ChannelType::Control,
                1 => ChannelType::Terminal,
                2 => ChannelType::Files,
                _ => {
                    tracing::warn!("unknown channel id: {}", channel_id[0]);
                    continue;
                }
            };

            if streams.has(channel_type) {
                tracing::warn!("duplicate {:?} stream received", channel_type);
                continue;
            }

            streams.set(channel_type, StreamPair { send, recv });
            tracing::debug!("Accepted {:?} stream", channel_type);
            accepted_count += 1;
        }

        Ok(())
    }

    /// Spawns background tasks to read from streams and forward to message channels.
    pub fn spawn_stream_readers(&self) {
        let streams = self.streams.clone();
        let message_tx = self.message_tx.clone();
        let connected = self.connected.clone();

        for channel_type in [
            ChannelType::Control,
            ChannelType::Terminal,
            ChannelType::Files,
        ] {
            let streams = streams.clone();
            let message_tx = message_tx.clone();
            let connected = connected.clone();

            tokio::spawn(async move {
                loop {
                    let result = Self::read_from_stream(&streams, channel_type).await;

                    match result {
                        Ok(data) => {
                            let tx = {
                                let senders = message_tx.read().await;
                                senders.get(&channel_type).cloned()
                            };

                            if let Some(tx) = tx {
                                if let Err(e) = tx.send(data).await {
                                    tracing::error!(
                                        "failed to forward {:?} message: {}",
                                        channel_type,
                                        e
                                    );
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            tracing::debug!("{:?} stream closed or error: {}", channel_type, e);
                            let mut conn = connected.write().await;
                            *conn = false;
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
    pub async fn send(&self, channel_type: ChannelType, data: &[u8]) -> Result<()> {
        Self::write_to_stream(&self.streams, channel_type, data).await
    }

    /// Receives data from a specific channel.
    pub async fn recv(&self, channel_type: ChannelType) -> Result<Vec<u8>> {
        let mut receivers = self.message_rx.lock().await;
        let rx = receivers.get_mut(&channel_type).ok_or_else(|| {
            ProtocolError::ConnectionClosed(format!("{:?} channel not available", channel_type))
        })?;

        rx.recv()
            .await
            .ok_or_else(|| ProtocolError::ConnectionClosed("channel closed".into()))
    }

    /// Closes the connection gracefully.
    pub async fn close(&self) -> Result<()> {
        // Close all streams
        {
            let mut streams = self.streams.lock().await;
            *streams = StreamChannels::default();
        }

        // Close the connection
        {
            let mut conn = self.connection.write().await;
            if let Some(connection) = conn.take() {
                connection.close(0u32.into(), b"graceful shutdown");
            }
        }

        // Mark as disconnected
        {
            let mut connected = self.connected.write().await;
            *connected = false;
        }

        tracing::info!("QUIC connection closed");
        Ok(())
    }

    /// Attempts to reconnect to a previously connected peer.
    ///
    /// This is useful when the connection is lost due to network issues.
    pub async fn reconnect(&self, node_addr: NodeAddr) -> Result<()> {
        // Close existing connection if any
        self.close().await?;

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

    /// Returns whether the connection is currently active.
    pub fn is_connected(&self) -> bool {
        match self.connected.try_read() {
            Ok(guard) => *guard,
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

    /// Returns the underlying iroh endpoint for advanced operations.
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Creates a new QUIC connection handler optimized for local testing.
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

        // Use shorter timeouts for testing
        let config = QuicConfig {
            relay_url: None,
            connect_timeout: Duration::from_secs(5),
            stream_timeout: Duration::from_secs(5),
        };

        Ok(Self {
            endpoint,
            connection: Arc::new(RwLock::new(None)),
            streams: Arc::new(Mutex::new(StreamChannels::default())),
            message_rx: Arc::new(Mutex::new(message_rx)),
            message_tx: Arc::new(RwLock::new(message_tx)),
            connected: Arc::new(RwLock::new(false)),
            peer_node_id: Arc::new(RwLock::new(None)),
            config,
        })
    }
}

impl ConnectionTrait for QuicConnectionHandler {
    fn send<'a>(
        &'a mut self,
        channel: ChannelType,
        data: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move { Self::write_to_stream(&self.streams, channel, data).await })
    }

    fn recv<'a>(
        &'a mut self,
        channel: ChannelType,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + 'a>> {
        Box::pin(async move {
            let mut receivers = self.message_rx.lock().await;
            let rx = receivers.get_mut(&channel).ok_or_else(|| {
                ProtocolError::ConnectionClosed(format!("{:?} channel not available", channel))
            })?;

            rx.recv()
                .await
                .ok_or_else(|| ProtocolError::ConnectionClosed("channel closed".into()))
        })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move { self.close().await })
    }

    fn is_connected(&self) -> bool {
        self.is_connected()
    }

    fn peer_public_key(&self) -> Option<[u8; 32]> {
        // iroh uses 32-byte public keys (same as X25519)
        self.peer_node_id().map(|id| *id.as_bytes())
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
    async fn test_create_handler() {
        let config = QuicConfig::default();
        let handler = QuicConnectionHandler::new(config).await;
        assert!(handler.is_ok());
    }

    #[tokio::test]
    async fn test_handler_has_node_id() {
        let config = QuicConfig::default();
        let handler = QuicConnectionHandler::new(config).await.unwrap();

        // Node ID should be non-zero
        let node_id = handler.node_id();
        assert!(!node_id.as_bytes().iter().all(|&b| b == 0));
    }

    #[tokio::test]
    async fn test_handler_with_secret_key() {
        let secret_key = SecretKey::generate(rand::rngs::OsRng);
        let expected_node_id = secret_key.public();

        let config = QuicConfig::default();
        let handler = QuicConnectionHandler::with_secret_key(config, secret_key)
            .await
            .unwrap();

        assert_eq!(handler.node_id(), expected_node_id);
    }

    #[tokio::test]
    async fn test_not_connected_initially() {
        let config = QuicConfig::default();
        let handler = QuicConnectionHandler::new(config).await.unwrap();

        assert!(!handler.is_connected());
        assert!(handler.peer_node_id().is_none());
    }

    #[tokio::test]
    async fn test_node_addr() {
        let config = QuicConfig::default();
        let handler = QuicConnectionHandler::new(config).await.unwrap();

        let addr = handler.node_addr().await;
        assert!(addr.is_ok());

        let addr = addr.unwrap();
        assert_eq!(addr.node_id, handler.node_id());
    }

    /// Integration test for QUIC connection between two endpoints.
    ///
    /// This test creates two handlers, connects them, creates streams,
    /// and exchanges messages.
    ///
    /// Note: This test is ignored by default because it requires network connectivity
    /// and may take a long time in some environments. Run with `--ignored` to include.
    #[tokio::test]
    #[ignore = "requires network connectivity, run with --ignored"]
    async fn test_full_connection() {
        // Create two handlers with testing configuration (no relay/discovery)
        let server = QuicConnectionHandler::new_for_testing()
            .await
            .expect("failed to create server");
        let client = QuicConnectionHandler::new_for_testing()
            .await
            .expect("failed to create client");

        // Get server's bound sockets (local addresses to connect to)
        let bound_sockets = server.endpoint().bound_sockets();
        let server_node_id = server.node_id();

        // Create a NodeAddr with direct addresses from bound sockets
        // bound_sockets returns (SocketAddrV4, Option<SocketAddrV6>)
        let mut direct_addrs: Vec<std::net::SocketAddr> = vec![bound_sockets.0];
        if let Some(addr) = bound_sockets.1 {
            direct_addrs.push(addr);
        }

        let server_addr = NodeAddr::from_parts(server_node_id, None, direct_addrs);

        // Spawn server accept task
        let server_handle = {
            let server = server;
            tokio::spawn(async move {
                server.accept().await.expect("server accept failed");
                server
                    .accept_streams()
                    .await
                    .expect("server accept streams failed");
                server.spawn_stream_readers();
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
        client.spawn_stream_readers();

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
        client.close().await.expect("client close failed");
        server.close().await.expect("server close failed");

        assert!(!client.is_connected());
        assert!(!server.is_connected());
    }

    /// Note: This test is ignored by default because it requires network connectivity.
    /// Run with `--ignored` to include.
    #[tokio::test]
    #[ignore = "requires network connectivity, run with --ignored"]
    async fn test_multiple_channel_streams() {
        // Create two handlers with testing configuration (no relay/discovery)
        let server = QuicConnectionHandler::new_for_testing()
            .await
            .expect("failed to create server");
        let client = QuicConnectionHandler::new_for_testing()
            .await
            .expect("failed to create client");

        // Get server's bound sockets (local addresses)
        let bound_sockets = server.endpoint().bound_sockets();
        let server_node_id = server.node_id();

        // bound_sockets returns (SocketAddrV4, Option<SocketAddrV6>)
        let mut direct_addrs: Vec<std::net::SocketAddr> = vec![bound_sockets.0];
        if let Some(addr) = bound_sockets.1 {
            direct_addrs.push(addr);
        }

        let server_addr = NodeAddr::from_parts(server_node_id, None, direct_addrs);

        // Spawn server
        let server_handle = {
            let server = server;
            tokio::spawn(async move {
                server.accept().await.expect("server accept failed");
                server
                    .accept_streams()
                    .await
                    .expect("server accept streams failed");
                server.spawn_stream_readers();
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
        client.spawn_stream_readers();

        let server = server_handle.await.expect("server task panicked");

        // Test all three channels
        for (channel, msg) in [
            (ChannelType::Control, b"control message".as_slice()),
            (ChannelType::Terminal, b"terminal data".as_slice()),
            (ChannelType::Files, b"file chunk".as_slice()),
        ] {
            client.send(channel, msg).await.expect("send failed");
            let received = server.recv(channel).await.expect("recv failed");
            assert_eq!(received, msg);
        }

        client.close().await.ok();
        server.close().await.ok();
    }
}
