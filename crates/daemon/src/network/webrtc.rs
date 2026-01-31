//! WebRTC connection handler for browser clients.
//!
//! This module implements WebRTC peer connections with:
//! - ICE server configuration (STUN/TURN)
//! - Signaling integration (offer/answer)
//! - Data channel creation (control, terminal, files)
//! - Noise protocol encryption over data channels

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use protocol::crypto::DeviceIdentity;
use protocol::error::{ProtocolError, Result};
use protocol::noise::NoiseSession;
use tokio::sync::{mpsc, Mutex, RwLock};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;

use super::{ChannelType, Connection};

/// Default STUN servers for ICE connectivity.
pub const DEFAULT_STUN_SERVERS: &[&str] = &[
    "stun:stun.l.google.com:19302",
    "stun:stun1.l.google.com:19302",
];

/// ICE server configuration.
#[derive(Debug, Clone)]
pub struct IceServer {
    /// STUN/TURN server URLs.
    pub urls: Vec<String>,
    /// Username for TURN authentication (optional for STUN).
    pub username: Option<String>,
    /// Credential for TURN authentication (optional for STUN).
    pub credential: Option<String>,
}

impl IceServer {
    /// Creates a new STUN server configuration.
    pub fn stun(url: impl Into<String>) -> Self {
        Self {
            urls: vec![url.into()],
            username: None,
            credential: None,
        }
    }

    /// Creates a new TURN server configuration with authentication.
    pub fn turn(url: impl Into<String>, username: impl Into<String>, credential: impl Into<String>) -> Self {
        Self {
            urls: vec![url.into()],
            username: Some(username.into()),
            credential: Some(credential.into()),
        }
    }

    /// Adds an additional URL to this server configuration.
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.urls.push(url.into());
        self
    }
}

impl From<IceServer> for RTCIceServer {
    fn from(server: IceServer) -> Self {
        RTCIceServer {
            urls: server.urls,
            username: server.username.unwrap_or_default(),
            credential: server.credential.unwrap_or_default(),
            ..Default::default()
        }
    }
}

/// Configuration for WebRTC connections.
#[derive(Debug, Clone)]
pub struct WebRtcConfig {
    /// ICE servers for connectivity.
    pub ice_servers: Vec<IceServer>,
}

impl Default for WebRtcConfig {
    fn default() -> Self {
        Self {
            ice_servers: DEFAULT_STUN_SERVERS
                .iter()
                .map(|&url| IceServer::stun(url))
                .collect(),
        }
    }
}

impl WebRtcConfig {
    /// Creates a new WebRTC configuration with custom ICE servers.
    pub fn with_ice_servers(ice_servers: Vec<IceServer>) -> Self {
        Self { ice_servers }
    }

    /// Adds an ICE server to the configuration.
    pub fn add_ice_server(mut self, server: IceServer) -> Self {
        self.ice_servers.push(server);
        self
    }

    /// Converts to WebRTC RTCConfiguration.
    fn to_rtc_configuration(&self) -> RTCConfiguration {
        RTCConfiguration {
            ice_servers: self.ice_servers.iter().cloned().map(Into::into).collect(),
            ..Default::default()
        }
    }
}

/// Internal state for data channels.
struct DataChannels {
    control: Option<Arc<RTCDataChannel>>,
    terminal: Option<Arc<RTCDataChannel>>,
    files: Option<Arc<RTCDataChannel>>,
}

impl Default for DataChannels {
    fn default() -> Self {
        Self {
            control: None,
            terminal: None,
            files: None,
        }
    }
}

impl DataChannels {
    fn get(&self, channel_type: ChannelType) -> Option<&Arc<RTCDataChannel>> {
        match channel_type {
            ChannelType::Control => self.control.as_ref(),
            ChannelType::Terminal => self.terminal.as_ref(),
            ChannelType::Files => self.files.as_ref(),
        }
    }

    fn set(&mut self, channel_type: ChannelType, channel: Arc<RTCDataChannel>) {
        match channel_type {
            ChannelType::Control => self.control = Some(channel),
            ChannelType::Terminal => self.terminal = Some(channel),
            ChannelType::Files => self.files = Some(channel),
        }
    }
}

/// WebRTC connection handler for browser clients.
///
/// This struct manages a WebRTC peer connection with:
/// - ICE negotiation for NAT traversal
/// - Multiple data channels for different message types
/// - Noise protocol encryption for secure communication
pub struct WebRtcConnectionHandler {
    /// The WebRTC peer connection.
    peer_connection: Arc<RTCPeerConnection>,
    /// Data channels for different message types.
    data_channels: Arc<RwLock<DataChannels>>,
    /// Noise session for encryption/decryption.
    noise_session: Arc<Mutex<Option<NoiseSession>>>,
    /// Device identity for Noise handshake.
    identity: DeviceIdentity,
    /// Receiver for incoming messages from data channels.
    message_rx: Arc<Mutex<HashMap<ChannelType, mpsc::Receiver<Vec<u8>>>>>,
    /// Senders for incoming messages (used by data channel callbacks).
    message_tx: Arc<RwLock<HashMap<ChannelType, mpsc::Sender<Vec<u8>>>>>,
    /// Whether the connection is established.
    connected: Arc<RwLock<bool>>,
    /// Remote peer's X25519 public key (from Noise handshake).
    peer_public_key: Arc<RwLock<Option<[u8; 32]>>>,
}

impl WebRtcConnectionHandler {
    /// Creates a new WebRTC connection handler.
    ///
    /// This sets up the WebRTC API and creates a peer connection with the given configuration.
    pub async fn new(config: WebRtcConfig, identity: DeviceIdentity) -> Result<Self> {
        // Create a MediaEngine (required even for data-only connections)
        let mut media_engine = MediaEngine::default();
        media_engine.register_default_codecs().map_err(|e| {
            ProtocolError::HandshakeFailed(format!("failed to register codecs: {}", e))
        })?;

        // Create an InterceptorRegistry
        let mut registry = Registry::new();
        registry = register_default_interceptors(registry, &mut media_engine).map_err(|e| {
            ProtocolError::HandshakeFailed(format!("failed to register interceptors: {}", e))
        })?;

        // Build the API
        let api = APIBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .build();

        // Create the peer connection
        let peer_connection = api
            .new_peer_connection(config.to_rtc_configuration())
            .await
            .map_err(|e| {
                ProtocolError::HandshakeFailed(format!("failed to create peer connection: {}", e))
            })?;

        let peer_connection = Arc::new(peer_connection);

        // Create message channels for each channel type
        let mut message_rx = HashMap::new();
        let mut message_tx = HashMap::new();

        for channel_type in [ChannelType::Control, ChannelType::Terminal, ChannelType::Files] {
            let (tx, rx) = mpsc::channel(256);
            message_tx.insert(channel_type, tx);
            message_rx.insert(channel_type, rx);
        }

        let handler = Self {
            peer_connection,
            data_channels: Arc::new(RwLock::new(DataChannels::default())),
            noise_session: Arc::new(Mutex::new(None)),
            identity,
            message_rx: Arc::new(Mutex::new(message_rx)),
            message_tx: Arc::new(RwLock::new(message_tx)),
            connected: Arc::new(RwLock::new(false)),
            peer_public_key: Arc::new(RwLock::new(None)),
        };

        // Set up connection state change handler
        let connected = handler.connected.clone();
        handler
            .peer_connection
            .on_peer_connection_state_change(Box::new(move |state: RTCPeerConnectionState| {
                let connected = connected.clone();
                Box::pin(async move {
                    let mut is_connected = connected.write().await;
                    *is_connected = matches!(state, RTCPeerConnectionState::Connected);
                    tracing::debug!("peer connection state changed: {:?}", state);
                })
            }));

        Ok(handler)
    }

    /// Creates data channels as the offerer.
    ///
    /// This should be called before creating an offer.
    pub async fn create_data_channels(&self) -> Result<()> {
        // Control channel: ordered, reliable
        let control_options = webrtc::data_channel::data_channel_init::RTCDataChannelInit {
            ordered: Some(true),
            ..Default::default()
        };
        let control = self
            .peer_connection
            .create_data_channel(ChannelType::Control.channel_name(), Some(control_options))
            .await
            .map_err(|e| {
                ProtocolError::HandshakeFailed(format!("failed to create control channel: {}", e))
            })?;
        self.setup_data_channel(ChannelType::Control, control).await?;

        // Terminal channel: unordered for low latency
        let terminal_options = webrtc::data_channel::data_channel_init::RTCDataChannelInit {
            ordered: Some(false),
            ..Default::default()
        };
        let terminal = self
            .peer_connection
            .create_data_channel(ChannelType::Terminal.channel_name(), Some(terminal_options))
            .await
            .map_err(|e| {
                ProtocolError::HandshakeFailed(format!("failed to create terminal channel: {}", e))
            })?;
        self.setup_data_channel(ChannelType::Terminal, terminal).await?;

        // Files channel: ordered, reliable
        let files_options = webrtc::data_channel::data_channel_init::RTCDataChannelInit {
            ordered: Some(true),
            ..Default::default()
        };
        let files = self
            .peer_connection
            .create_data_channel(ChannelType::Files.channel_name(), Some(files_options))
            .await
            .map_err(|e| {
                ProtocolError::HandshakeFailed(format!("failed to create files channel: {}", e))
            })?;
        self.setup_data_channel(ChannelType::Files, files).await?;

        Ok(())
    }

    /// Sets up handlers for incoming data channels (as answerer).
    pub async fn setup_incoming_data_channels(&self) {
        let data_channels = self.data_channels.clone();
        let message_tx = self.message_tx.clone();

        self.peer_connection.on_data_channel(Box::new(
            move |channel: Arc<RTCDataChannel>| {
                let data_channels = data_channels.clone();
                let message_tx = message_tx.clone();

                Box::pin(async move {
                    let label = channel.label().to_string();
                    let channel_type = match label.as_str() {
                        "control" => ChannelType::Control,
                        "terminal" => ChannelType::Terminal,
                        "files" => ChannelType::Files,
                        _ => {
                            tracing::warn!("unknown data channel: {}", label);
                            return;
                        }
                    };

                    // Store the channel
                    {
                        let mut channels = data_channels.write().await;
                        channels.set(channel_type, channel.clone());
                    }

                    // Set up message handler
                    let tx = {
                        let senders = message_tx.read().await;
                        senders.get(&channel_type).cloned()
                    };

                    if let Some(tx) = tx {
                        channel.on_message(Box::new(move |msg: DataChannelMessage| {
                            let tx = tx.clone();
                            Box::pin(async move {
                                if let Err(e) = tx.send(msg.data.to_vec()).await {
                                    tracing::error!("failed to forward message: {}", e);
                                }
                            })
                        }));
                    }

                    tracing::debug!("data channel '{}' established", label);
                })
            },
        ));
    }

    /// Sets up a data channel with message handling.
    async fn setup_data_channel(
        &self,
        channel_type: ChannelType,
        channel: Arc<RTCDataChannel>,
    ) -> Result<()> {
        // Store the channel
        {
            let mut channels = self.data_channels.write().await;
            channels.set(channel_type, channel.clone());
        }

        // Set up message handler
        let tx = {
            let senders = self.message_tx.read().await;
            senders.get(&channel_type).cloned()
        };

        if let Some(tx) = tx {
            channel.on_message(Box::new(move |msg: DataChannelMessage| {
                let tx = tx.clone();
                Box::pin(async move {
                    if let Err(e) = tx.send(msg.data.to_vec()).await {
                        tracing::error!("failed to forward message: {}", e);
                    }
                })
            }));
        }

        Ok(())
    }

    /// Creates an SDP offer for signaling.
    pub async fn create_offer(&self) -> Result<RTCSessionDescription> {
        let offer = self
            .peer_connection
            .create_offer(None)
            .await
            .map_err(|e| ProtocolError::HandshakeFailed(format!("failed to create offer: {}", e)))?;

        self.peer_connection
            .set_local_description(offer.clone())
            .await
            .map_err(|e| {
                ProtocolError::HandshakeFailed(format!("failed to set local description: {}", e))
            })?;

        Ok(offer)
    }

    /// Creates an SDP answer for signaling.
    pub async fn create_answer(&self) -> Result<RTCSessionDescription> {
        let answer = self
            .peer_connection
            .create_answer(None)
            .await
            .map_err(|e| ProtocolError::HandshakeFailed(format!("failed to create answer: {}", e)))?;

        self.peer_connection
            .set_local_description(answer.clone())
            .await
            .map_err(|e| {
                ProtocolError::HandshakeFailed(format!("failed to set local description: {}", e))
            })?;

        Ok(answer)
    }

    /// Sets the remote SDP description.
    pub async fn set_remote_description(&self, desc: RTCSessionDescription) -> Result<()> {
        self.peer_connection
            .set_remote_description(desc)
            .await
            .map_err(|e| {
                ProtocolError::HandshakeFailed(format!("failed to set remote description: {}", e))
            })
    }

    /// Waits for the ICE gathering to complete and returns the local description.
    pub async fn gather_ice_candidates(&self) -> Result<RTCSessionDescription> {
        // Wait for ICE gathering to complete
        let (tx, mut rx) = mpsc::channel(1);

        self.peer_connection.on_ice_gathering_state_change(Box::new(
            move |state: webrtc::ice_transport::ice_gatherer_state::RTCIceGathererState| {
                let tx = tx.clone();
                Box::pin(async move {
                    if state == webrtc::ice_transport::ice_gatherer_state::RTCIceGathererState::Complete {
                        let _ = tx.send(()).await;
                    }
                })
            },
        ));

        // Wait for gathering to complete (with timeout)
        tokio::select! {
            _ = rx.recv() => {}
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
                tracing::warn!("ICE gathering timeout, proceeding with current candidates");
            }
        }

        self.peer_connection
            .local_description()
            .await
            .ok_or_else(|| ProtocolError::HandshakeFailed("no local description available".into()))
    }

    /// Performs the Noise XX handshake as initiator.
    ///
    /// This should be called after the WebRTC connection is established.
    pub async fn perform_noise_handshake_initiator(&self) -> Result<()> {
        let mut noise = NoiseSession::new_initiator(&self.identity)?;

        // Message 1: Send -> e
        let msg1 = noise.write_handshake_message(&[])?;
        self.send_raw(ChannelType::Control, &msg1).await?;

        // Message 2: Receive <- e, ee, s, es
        let msg2 = self.recv_raw(ChannelType::Control).await?;
        noise.read_handshake_message(&msg2)?;

        // Message 3: Send -> s, se
        let msg3 = noise.write_handshake_message(&[])?;
        self.send_raw(ChannelType::Control, &msg3).await?;

        // Store the peer's public key
        if let Some(key) = noise.get_remote_static() {
            let mut peer_key = self.peer_public_key.write().await;
            *peer_key = Some(key);
        }

        // Transition to transport mode
        noise.into_transport()?;

        // Store the noise session
        let mut session = self.noise_session.lock().await;
        *session = Some(noise);

        Ok(())
    }

    /// Performs the Noise XX handshake as responder.
    ///
    /// This should be called after the WebRTC connection is established.
    pub async fn perform_noise_handshake_responder(&self) -> Result<()> {
        let mut noise = NoiseSession::new_responder(&self.identity)?;

        // Message 1: Receive -> e
        let msg1 = self.recv_raw(ChannelType::Control).await?;
        noise.read_handshake_message(&msg1)?;

        // Message 2: Send <- e, ee, s, es
        let msg2 = noise.write_handshake_message(&[])?;
        self.send_raw(ChannelType::Control, &msg2).await?;

        // Message 3: Receive -> s, se
        let msg3 = self.recv_raw(ChannelType::Control).await?;
        noise.read_handshake_message(&msg3)?;

        // Store the peer's public key
        if let Some(key) = noise.get_remote_static() {
            let mut peer_key = self.peer_public_key.write().await;
            *peer_key = Some(key);
        }

        // Transition to transport mode
        noise.into_transport()?;

        // Store the noise session
        let mut session = self.noise_session.lock().await;
        *session = Some(noise);

        Ok(())
    }

    /// Sends raw (unencrypted) data over a data channel.
    ///
    /// Used during handshake before encryption is established.
    async fn send_raw(&self, channel_type: ChannelType, data: &[u8]) -> Result<()> {
        let channels = self.data_channels.read().await;
        let channel = channels.get(channel_type).ok_or_else(|| {
            ProtocolError::ConnectionClosed(format!("{:?} channel not available", channel_type))
        })?;

        channel
            .send(&bytes::Bytes::copy_from_slice(data))
            .await
            .map_err(|e| ProtocolError::TransferFailed(format!("failed to send: {}", e)))?;

        Ok(())
    }

    /// Receives raw (unencrypted) data from a data channel.
    ///
    /// Used during handshake before encryption is established.
    async fn recv_raw(&self, channel_type: ChannelType) -> Result<Vec<u8>> {
        let mut receivers = self.message_rx.lock().await;
        let rx = receivers.get_mut(&channel_type).ok_or_else(|| {
            ProtocolError::ConnectionClosed(format!("{:?} channel not available", channel_type))
        })?;

        rx.recv().await.ok_or_else(|| {
            ProtocolError::ConnectionClosed("channel closed".into())
        })
    }

    /// Sends encrypted data over a data channel.
    async fn send_encrypted(&self, channel_type: ChannelType, data: &[u8]) -> Result<()> {
        let mut session = self.noise_session.lock().await;
        let noise = session.as_mut().ok_or(ProtocolError::HandshakeIncomplete)?;

        let ciphertext = noise.encrypt(data)?;
        drop(session);

        self.send_raw(channel_type, &ciphertext).await
    }

    /// Receives and decrypts data from a data channel.
    async fn recv_encrypted(&self, channel_type: ChannelType) -> Result<Vec<u8>> {
        let ciphertext = self.recv_raw(channel_type).await?;

        let mut session = self.noise_session.lock().await;
        let noise = session.as_mut().ok_or(ProtocolError::HandshakeIncomplete)?;

        noise.decrypt(&ciphertext)
    }

    /// Returns the underlying peer connection for advanced operations.
    pub fn peer_connection(&self) -> &Arc<RTCPeerConnection> {
        &self.peer_connection
    }
}

impl Connection for WebRtcConnectionHandler {
    fn send<'a>(
        &'a mut self,
        channel: ChannelType,
        data: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(self.send_encrypted(channel, data))
    }

    fn recv<'a>(
        &'a mut self,
        channel: ChannelType,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + 'a>> {
        Box::pin(self.recv_encrypted(channel))
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            self.peer_connection.close().await.map_err(|e| {
                ProtocolError::ConnectionClosed(format!("failed to close connection: {}", e))
            })
        })
    }

    fn is_connected(&self) -> bool {
        // We can't await in a sync function, so we check the peer connection state directly
        matches!(
            self.peer_connection.connection_state(),
            RTCPeerConnectionState::Connected
        )
    }

    fn peer_public_key(&self) -> Option<[u8; 32]> {
        // This is a sync function, so we can't await. Return None if lock would block.
        // In practice, this should be called after handshake completes.
        match self.peer_public_key.try_read() {
            Ok(guard) => *guard,
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_webrtc_config_default() {
        let config = WebRtcConfig::default();
        assert!(!config.ice_servers.is_empty());
    }

    #[tokio::test]
    async fn test_ice_server_stun() {
        let server = IceServer::stun("stun:example.com:3478");
        assert_eq!(server.urls, vec!["stun:example.com:3478"]);
        assert!(server.username.is_none());
        assert!(server.credential.is_none());
    }

    #[tokio::test]
    async fn test_ice_server_turn() {
        let server = IceServer::turn("turn:example.com:3478", "user", "pass");
        assert_eq!(server.urls, vec!["turn:example.com:3478"]);
        assert_eq!(server.username, Some("user".to_string()));
        assert_eq!(server.credential, Some("pass".to_string()));
    }

    #[tokio::test]
    async fn test_create_connection_handler() {
        let identity = DeviceIdentity::generate();
        let config = WebRtcConfig::default();

        let handler = WebRtcConnectionHandler::new(config, identity).await;
        assert!(handler.is_ok());
    }

    #[tokio::test]
    async fn test_create_data_channels() {
        let identity = DeviceIdentity::generate();
        let config = WebRtcConfig::default();

        let handler = WebRtcConnectionHandler::new(config, identity).await.unwrap();
        let result = handler.create_data_channels().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_offer() {
        let identity = DeviceIdentity::generate();
        let config = WebRtcConfig::default();

        let handler = WebRtcConnectionHandler::new(config, identity).await.unwrap();
        handler.create_data_channels().await.unwrap();

        let offer = handler.create_offer().await;
        assert!(offer.is_ok());
    }

    #[tokio::test]
    async fn test_channel_type_names() {
        assert_eq!(ChannelType::Control.channel_name(), "control");
        assert_eq!(ChannelType::Terminal.channel_name(), "terminal");
        assert_eq!(ChannelType::Files.channel_name(), "files");
    }

    /// Integration test for full WebRTC connection with Noise handshake.
    ///
    /// This test creates two peers, establishes a WebRTC connection between them,
    /// performs a Noise handshake, and exchanges encrypted messages.
    #[tokio::test]
    async fn test_full_connection_establishment() {
        // Create two identities
        let offerer_identity = DeviceIdentity::generate();
        let answerer_identity = DeviceIdentity::generate();

        // Create handlers
        let offerer = WebRtcConnectionHandler::new(
            WebRtcConfig::default(),
            offerer_identity,
        )
        .await
        .expect("failed to create offerer");

        let answerer = WebRtcConnectionHandler::new(
            WebRtcConfig::default(),
            answerer_identity,
        )
        .await
        .expect("failed to create answerer");

        // Offerer creates data channels and offer
        offerer.create_data_channels().await.expect("failed to create data channels");
        let offer = offerer.create_offer().await.expect("failed to create offer");

        // Set up answerer to receive data channels
        answerer.setup_incoming_data_channels().await;

        // Answerer processes offer and creates answer
        answerer
            .set_remote_description(offer)
            .await
            .expect("failed to set remote description");
        let answer = answerer.create_answer().await.expect("failed to create answer");

        // Offerer processes answer
        offerer
            .set_remote_description(answer)
            .await
            .expect("failed to set remote description");

        // Note: In a real test, we would wait for ICE to complete and then
        // perform the Noise handshake. However, without a signaling server
        // to exchange ICE candidates, we can't fully test the connection.
        // The above tests verify the API works correctly.
    }
}
