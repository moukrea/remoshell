//! Session output multiplexer.
//!
//! This module provides broadcasting of PTY output to multiple attached clients.
//! It handles slow clients gracefully by dropping messages when their buffers are full,
//! tracks backpressure, and maintains activity timestamps.

use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::mpsc;
use tokio::sync::RwLock;

/// Unique identifier for a client.
pub type ClientId = String;

/// Default channel capacity for client output.
const DEFAULT_CHANNEL_CAPACITY: usize = 256;

/// Buffer size for reading from PTY.
const READ_BUFFER_SIZE: usize = 4096;

/// Statistics about a client's message handling.
#[derive(Debug, Clone, Default)]
pub struct ClientStats {
    /// Total messages sent successfully.
    pub messages_sent: u64,
    /// Messages dropped due to slow client.
    pub messages_dropped: u64,
    /// Whether the client is currently experiencing backpressure.
    pub is_backpressured: bool,
}

/// A handle representing a connected client that receives output.
///
/// Each client has a bounded channel for output data. When the channel is full,
/// messages are dropped rather than blocking other clients.
pub struct ClientHandle {
    /// Unique client identifier.
    id: ClientId,
    /// Sender for output data to this client.
    tx: mpsc::Sender<Vec<u8>>,
    /// Statistics about message handling.
    stats: ClientStats,
    /// Whether the client is currently backpressured.
    backpressured: AtomicBool,
}

impl ClientHandle {
    /// Creates a new client handle.
    ///
    /// Returns the handle and a receiver for output data.
    pub fn new(id: ClientId) -> (Self, mpsc::Receiver<Vec<u8>>) {
        Self::with_capacity(id, DEFAULT_CHANNEL_CAPACITY)
    }

    /// Creates a new client handle with a specific channel capacity.
    pub fn with_capacity(id: ClientId, capacity: usize) -> (Self, mpsc::Receiver<Vec<u8>>) {
        let (tx, rx) = mpsc::channel(capacity);
        let handle = ClientHandle {
            id,
            tx,
            stats: ClientStats::default(),
            backpressured: AtomicBool::new(false),
        };
        (handle, rx)
    }

    /// Returns the client ID.
    pub fn id(&self) -> &ClientId {
        &self.id
    }

    /// Returns a clone of the current statistics.
    pub fn stats(&self) -> ClientStats {
        let mut stats = self.stats.clone();
        stats.is_backpressured = self.backpressured.load(Ordering::Relaxed);
        stats
    }

    /// Returns whether the client is currently backpressured.
    pub fn is_backpressured(&self) -> bool {
        self.backpressured.load(Ordering::Relaxed)
    }

    /// Attempts to send data to the client.
    ///
    /// Uses try_send to avoid blocking. If the channel is full, the message
    /// is dropped and the backpressure flag is set.
    ///
    /// Returns true if the message was sent, false if dropped.
    fn try_send(&mut self, data: Vec<u8>) -> bool {
        match self.tx.try_send(data) {
            Ok(()) => {
                self.stats.messages_sent += 1;
                if self.backpressured.load(Ordering::Relaxed) {
                    self.backpressured.store(false, Ordering::Relaxed);
                    tracing::debug!(
                        client_id = %self.id,
                        "Client recovered from backpressure"
                    );
                }
                true
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.stats.messages_dropped += 1;
                if !self.backpressured.load(Ordering::Relaxed) {
                    self.backpressured.store(true, Ordering::Relaxed);
                    tracing::warn!(
                        client_id = %self.id,
                        dropped = self.stats.messages_dropped,
                        "Client is backpressured, dropping messages"
                    );
                }
                false
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                // Client disconnected
                tracing::debug!(client_id = %self.id, "Client channel closed");
                false
            }
        }
    }

    /// Checks if the client channel is closed.
    fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }
}

/// Broadcasts PTY output to multiple connected clients.
///
/// The broadcaster reads from a PTY using spawn_blocking and fans out
/// the output to all registered clients. Slow clients are handled by
/// dropping messages (try_send) rather than blocking.
pub struct SessionOutputBroadcaster {
    /// Map of client ID to client handle.
    clients: Arc<RwLock<HashMap<ClientId, ClientHandle>>>,
    /// Flag indicating if the broadcaster is running.
    running: Arc<AtomicBool>,
    /// Last activity timestamp (Unix epoch milliseconds).
    last_activity: Arc<AtomicU64>,
}

impl SessionOutputBroadcaster {
    /// Creates a new session output broadcaster.
    pub fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(AtomicBool::new(false)),
            last_activity: Arc::new(AtomicU64::new(Self::now_millis())),
        }
    }

    /// Returns the current Unix timestamp in milliseconds.
    fn now_millis() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    /// Registers a new client to receive output.
    ///
    /// Returns a receiver for the client's output data.
    pub async fn add_client(&self, client_id: ClientId) -> mpsc::Receiver<Vec<u8>> {
        self.add_client_with_capacity(client_id, DEFAULT_CHANNEL_CAPACITY)
            .await
    }

    /// Registers a new client with a specific channel capacity.
    pub async fn add_client_with_capacity(
        &self,
        client_id: ClientId,
        capacity: usize,
    ) -> mpsc::Receiver<Vec<u8>> {
        let (handle, rx) = ClientHandle::with_capacity(client_id.clone(), capacity);
        let mut clients = self.clients.write().await;
        clients.insert(client_id.clone(), handle);
        tracing::debug!(client_id = %client_id, "Added client to broadcaster");
        rx
    }

    /// Removes a client from the broadcaster.
    ///
    /// Returns the client's statistics if the client existed.
    pub async fn remove_client(&self, client_id: &ClientId) -> Option<ClientStats> {
        let mut clients = self.clients.write().await;
        clients.remove(client_id).map(|h| h.stats())
    }

    /// Returns the number of connected clients.
    pub async fn client_count(&self) -> usize {
        self.clients.read().await.len()
    }

    /// Returns statistics for a specific client.
    pub async fn client_stats(&self, client_id: &ClientId) -> Option<ClientStats> {
        self.clients.read().await.get(client_id).map(|h| h.stats())
    }

    /// Returns whether a specific client is backpressured.
    pub async fn is_client_backpressured(&self, client_id: &ClientId) -> bool {
        self.clients
            .read()
            .await
            .get(client_id)
            .map(|h| h.is_backpressured())
            .unwrap_or(false)
    }

    /// Returns the last activity timestamp in Unix milliseconds.
    pub fn last_activity(&self) -> u64 {
        self.last_activity.load(Ordering::Relaxed)
    }

    /// Returns whether the broadcaster is currently running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Stops the broadcaster.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Broadcasts data to all connected clients.
    ///
    /// This is the core fan-out method. It sends data to all clients,
    /// dropping messages for slow clients and removing disconnected ones.
    ///
    /// Returns the number of clients that successfully received the data.
    pub async fn broadcast(&self, data: Vec<u8>) -> usize {
        // Update activity timestamp
        self.last_activity
            .store(Self::now_millis(), Ordering::Relaxed);

        let mut clients = self.clients.write().await;
        let mut disconnected = Vec::new();
        let mut success_count = 0;

        for (client_id, handle) in clients.iter_mut() {
            if handle.is_closed() {
                disconnected.push(client_id.clone());
                continue;
            }

            if handle.try_send(data.clone()) {
                success_count += 1;
            }
        }

        // Remove disconnected clients
        for client_id in disconnected {
            clients.remove(&client_id);
            tracing::debug!(client_id = %client_id, "Removed disconnected client");
        }

        success_count
    }

    /// Starts the PTY reader loop using spawn_blocking.
    ///
    /// This method reads from the provided PTY reader and broadcasts
    /// output to all connected clients. The loop runs until the
    /// broadcaster is stopped or the PTY is closed.
    ///
    /// # Arguments
    /// * `reader` - A PTY reader that implements Read + Send + 'static.
    pub fn start_reader_loop<R>(&self, reader: R)
    where
        R: Read + Send + 'static,
    {
        if self.running.swap(true, Ordering::SeqCst) {
            tracing::warn!("Reader loop already running");
            return;
        }

        let clients = Arc::clone(&self.clients);
        let running = Arc::clone(&self.running);
        let last_activity = Arc::clone(&self.last_activity);

        // Wrap reader for spawn_blocking
        let reader = Arc::new(std::sync::Mutex::new(reader));

        tokio::spawn(async move {
            loop {
                if !running.load(Ordering::SeqCst) {
                    tracing::debug!("Reader loop stopping: not running");
                    break;
                }

                let reader_clone = Arc::clone(&reader);

                // Use spawn_blocking to read from PTY
                let result = tokio::task::spawn_blocking(move || {
                    let mut buffer = vec![0u8; READ_BUFFER_SIZE];
                    let mut reader = reader_clone.lock().unwrap();
                    match reader.read(&mut buffer) {
                        Ok(0) => Ok(None), // EOF
                        Ok(n) => {
                            buffer.truncate(n);
                            Ok(Some(buffer))
                        }
                        Err(e) => Err(e),
                    }
                })
                .await;

                match result {
                    Ok(Ok(Some(data))) => {
                        // Update activity timestamp
                        last_activity.store(Self::now_millis(), Ordering::Relaxed);

                        // Fan out to all clients
                        let mut clients_guard = clients.write().await;
                        let mut disconnected = Vec::new();

                        for (client_id, handle) in clients_guard.iter_mut() {
                            if handle.is_closed() {
                                disconnected.push(client_id.clone());
                                continue;
                            }
                            handle.try_send(data.clone());
                        }

                        // Remove disconnected clients
                        for client_id in disconnected {
                            clients_guard.remove(&client_id);
                            tracing::debug!(client_id = %client_id, "Removed disconnected client");
                        }
                    }
                    Ok(Ok(None)) => {
                        // EOF
                        tracing::info!("PTY EOF - reader loop ending");
                        running.store(false, Ordering::SeqCst);
                        break;
                    }
                    Ok(Err(e)) => {
                        if running.load(Ordering::SeqCst) {
                            tracing::error!(error = %e, "Error reading from PTY");
                        }
                        running.store(false, Ordering::SeqCst);
                        break;
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Read task panicked");
                        running.store(false, Ordering::SeqCst);
                        break;
                    }
                }
            }

            tracing::debug!("Reader loop ended");
        });
    }

    /// Returns a list of all connected client IDs.
    pub async fn client_ids(&self) -> Vec<ClientId> {
        self.clients.read().await.keys().cloned().collect()
    }

    /// Returns statistics for all clients.
    pub async fn all_client_stats(&self) -> HashMap<ClientId, ClientStats> {
        self.clients
            .read()
            .await
            .iter()
            .map(|(id, h)| (id.clone(), h.stats()))
            .collect()
    }
}

impl Default for SessionOutputBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::time::Duration;
    use tokio::time::timeout;

    #[test]
    fn test_client_handle_creation() {
        let (handle, _rx) = ClientHandle::new("client-1".to_string());
        assert_eq!(handle.id(), "client-1");
        assert!(!handle.is_backpressured());

        let stats = handle.stats();
        assert_eq!(stats.messages_sent, 0);
        assert_eq!(stats.messages_dropped, 0);
        assert!(!stats.is_backpressured);
    }

    #[test]
    fn test_client_handle_with_capacity() {
        let (handle, _rx) = ClientHandle::with_capacity("client-2".to_string(), 10);
        assert_eq!(handle.id(), "client-2");
    }

    #[tokio::test]
    async fn test_broadcaster_creation() {
        let broadcaster = SessionOutputBroadcaster::new();
        assert!(!broadcaster.is_running());
        assert_eq!(broadcaster.client_count().await, 0);

        // last_activity should be set to current time
        let now = SessionOutputBroadcaster::now_millis();
        let activity = broadcaster.last_activity();
        assert!(activity <= now);
        assert!(now - activity < 1000); // Within 1 second
    }

    #[tokio::test]
    async fn test_add_and_remove_client() {
        let broadcaster = SessionOutputBroadcaster::new();

        // Add a client
        let _rx = broadcaster.add_client("client-1".to_string()).await;
        assert_eq!(broadcaster.client_count().await, 1);

        // Add another client
        let _rx2 = broadcaster.add_client("client-2".to_string()).await;
        assert_eq!(broadcaster.client_count().await, 2);

        // Remove a client
        let stats = broadcaster.remove_client(&"client-1".to_string()).await;
        assert!(stats.is_some());
        assert_eq!(broadcaster.client_count().await, 1);

        // Remove non-existent client
        let stats = broadcaster.remove_client(&"nonexistent".to_string()).await;
        assert!(stats.is_none());
    }

    #[tokio::test]
    async fn test_broadcast_to_multiple_clients() {
        let broadcaster = SessionOutputBroadcaster::new();

        // Add multiple clients
        let mut rx1 = broadcaster.add_client("client-1".to_string()).await;
        let mut rx2 = broadcaster.add_client("client-2".to_string()).await;
        let mut rx3 = broadcaster.add_client("client-3".to_string()).await;

        // Broadcast data
        let data = b"Hello, clients!".to_vec();
        let count = broadcaster.broadcast(data.clone()).await;
        assert_eq!(count, 3);

        // All clients should receive the data
        let received1 = timeout(Duration::from_millis(100), rx1.recv())
            .await
            .expect("timeout")
            .expect("no data");
        let received2 = timeout(Duration::from_millis(100), rx2.recv())
            .await
            .expect("timeout")
            .expect("no data");
        let received3 = timeout(Duration::from_millis(100), rx3.recv())
            .await
            .expect("timeout")
            .expect("no data");

        assert_eq!(received1, data);
        assert_eq!(received2, data);
        assert_eq!(received3, data);

        // Check stats
        let stats1 = broadcaster
            .client_stats(&"client-1".to_string())
            .await
            .unwrap();
        assert_eq!(stats1.messages_sent, 1);
        assert_eq!(stats1.messages_dropped, 0);
    }

    #[tokio::test]
    async fn test_slow_client_handling() {
        let broadcaster = SessionOutputBroadcaster::new();

        // Add a client with very small capacity
        let mut rx_fast = broadcaster.add_client("fast-client".to_string()).await;
        let _rx_slow = broadcaster
            .add_client_with_capacity("slow-client".to_string(), 2)
            .await;

        // Fast client consumes immediately, slow client doesn't
        // Broadcast more messages than the slow client can buffer
        for i in 0..10 {
            broadcaster
                .broadcast(format!("message-{}", i).into_bytes())
                .await;

            // Fast client always consumes
            let _ = rx_fast.recv().await;
        }

        // Check slow client stats - should have dropped some messages
        let stats = broadcaster
            .client_stats(&"slow-client".to_string())
            .await
            .unwrap();
        assert!(
            stats.messages_dropped > 0,
            "Slow client should have dropped messages"
        );

        // Fast client should have received all
        let fast_stats = broadcaster
            .client_stats(&"fast-client".to_string())
            .await
            .unwrap();
        assert_eq!(fast_stats.messages_sent, 10);
        assert_eq!(fast_stats.messages_dropped, 0);
    }

    #[tokio::test]
    async fn test_backpressure_signaling() {
        let broadcaster = SessionOutputBroadcaster::new();

        // Add a client with tiny capacity
        let _rx = broadcaster
            .add_client_with_capacity("client".to_string(), 1)
            .await;

        // Should not be backpressured initially
        assert!(
            !broadcaster
                .is_client_backpressured(&"client".to_string())
                .await
        );

        // Fill the buffer
        broadcaster.broadcast(b"msg1".to_vec()).await;
        broadcaster.broadcast(b"msg2".to_vec()).await;
        broadcaster.broadcast(b"msg3".to_vec()).await;

        // Should now be backpressured
        assert!(
            broadcaster
                .is_client_backpressured(&"client".to_string())
                .await
        );
    }

    #[tokio::test]
    async fn test_last_activity_updates() {
        let broadcaster = SessionOutputBroadcaster::new();
        let initial_activity = broadcaster.last_activity();

        // Wait a bit
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Broadcast updates activity
        broadcaster.broadcast(b"data".to_vec()).await;
        let new_activity = broadcaster.last_activity();

        assert!(new_activity >= initial_activity);
    }

    #[tokio::test]
    async fn test_reader_loop_with_cursor() {
        let broadcaster = SessionOutputBroadcaster::new();

        // Add a client
        let mut rx = broadcaster.add_client("client".to_string()).await;

        // Create a simple reader with some data
        let data = b"Line 1\nLine 2\nLine 3";
        let reader = Cursor::new(data.to_vec());

        // Start the reader loop
        broadcaster.start_reader_loop(reader);

        // Should be running
        assert!(broadcaster.is_running());

        // Receive the data
        let received = timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("timeout")
            .expect("no data");

        assert!(!received.is_empty());

        // Wait for the loop to end (EOF)
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Should have stopped after EOF
        assert!(!broadcaster.is_running());
    }

    #[tokio::test]
    async fn test_stop_broadcaster() {
        let broadcaster = SessionOutputBroadcaster::new();

        // Create a reader that blocks (using a pipe would be complex, so use a cursor)
        // With a Cursor, it will EOF immediately, but we can test stop behavior

        assert!(!broadcaster.is_running());

        // Manually set running to true
        broadcaster.running.store(true, Ordering::SeqCst);
        assert!(broadcaster.is_running());

        // Stop it
        broadcaster.stop();
        assert!(!broadcaster.is_running());
    }

    #[tokio::test]
    async fn test_client_ids() {
        let broadcaster = SessionOutputBroadcaster::new();

        let _rx1 = broadcaster.add_client("alpha".to_string()).await;
        let _rx2 = broadcaster.add_client("beta".to_string()).await;
        let _rx3 = broadcaster.add_client("gamma".to_string()).await;

        let ids = broadcaster.client_ids().await;
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&"alpha".to_string()));
        assert!(ids.contains(&"beta".to_string()));
        assert!(ids.contains(&"gamma".to_string()));
    }

    #[tokio::test]
    async fn test_all_client_stats() {
        let broadcaster = SessionOutputBroadcaster::new();

        let _rx1 = broadcaster.add_client("client-1".to_string()).await;
        let _rx2 = broadcaster.add_client("client-2".to_string()).await;

        // Send some data
        broadcaster.broadcast(b"test".to_vec()).await;

        let all_stats = broadcaster.all_client_stats().await;
        assert_eq!(all_stats.len(), 2);
        assert!(all_stats.contains_key("client-1"));
        assert!(all_stats.contains_key("client-2"));

        for stats in all_stats.values() {
            assert_eq!(stats.messages_sent, 1);
        }
    }

    #[tokio::test]
    async fn test_disconnected_client_removal() {
        let broadcaster = SessionOutputBroadcaster::new();

        // Add clients
        let rx1 = broadcaster.add_client("stays".to_string()).await;
        let rx2 = broadcaster.add_client("drops".to_string()).await;

        assert_eq!(broadcaster.client_count().await, 2);

        // Drop one receiver
        drop(rx2);

        // Keep the other one
        let _rx1 = rx1;

        // Broadcast should remove the disconnected client
        broadcaster.broadcast(b"test".to_vec()).await;

        // Wait a moment for the cleanup
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Only one client should remain
        assert_eq!(broadcaster.client_count().await, 1);
        let ids = broadcaster.client_ids().await;
        assert!(ids.contains(&"stays".to_string()));
        assert!(!ids.contains(&"drops".to_string()));
    }

    #[tokio::test]
    async fn test_output_ordering_preserved() {
        let broadcaster = SessionOutputBroadcaster::new();

        let mut rx = broadcaster.add_client("client".to_string()).await;

        // Send multiple messages in order
        for i in 0..10 {
            broadcaster
                .broadcast(format!("msg-{}", i).into_bytes())
                .await;
        }

        // Receive and verify order
        for i in 0..10 {
            let received = timeout(Duration::from_millis(100), rx.recv())
                .await
                .expect("timeout")
                .expect("no data");
            let expected = format!("msg-{}", i).into_bytes();
            assert_eq!(
                received, expected,
                "Message order not preserved at index {}",
                i
            );
        }
    }

    /// Integration test: multiple clients receive the same output concurrently.
    #[tokio::test]
    async fn test_multi_client_broadcast_integration() {
        let broadcaster = SessionOutputBroadcaster::new();

        // Set up multiple clients with different capacities
        let mut rx1 = broadcaster.add_client("client-1".to_string()).await;
        let mut rx2 = broadcaster
            .add_client_with_capacity("client-2".to_string(), 50)
            .await;
        let mut rx3 = broadcaster
            .add_client_with_capacity("client-3".to_string(), 100)
            .await;

        // Simulate PTY output by broadcasting multiple chunks
        let messages: Vec<Vec<u8>> = (0..20)
            .map(|i| format!("output-chunk-{:03}", i).into_bytes())
            .collect();

        // Broadcast all messages
        for msg in &messages {
            broadcaster.broadcast(msg.clone()).await;
        }

        // All clients should receive all messages in order
        for (i, expected) in messages.iter().enumerate() {
            let r1 = timeout(Duration::from_millis(100), rx1.recv())
                .await
                .expect("timeout rx1")
                .expect("no data rx1");
            let r2 = timeout(Duration::from_millis(100), rx2.recv())
                .await
                .expect("timeout rx2")
                .expect("no data rx2");
            let r3 = timeout(Duration::from_millis(100), rx3.recv())
                .await
                .expect("timeout rx3")
                .expect("no data rx3");

            assert_eq!(&r1, expected, "Client 1 mismatch at {}", i);
            assert_eq!(&r2, expected, "Client 2 mismatch at {}", i);
            assert_eq!(&r3, expected, "Client 3 mismatch at {}", i);
        }

        // All stats should show no drops
        let stats1 = broadcaster
            .client_stats(&"client-1".to_string())
            .await
            .unwrap();
        let stats2 = broadcaster
            .client_stats(&"client-2".to_string())
            .await
            .unwrap();
        let stats3 = broadcaster
            .client_stats(&"client-3".to_string())
            .await
            .unwrap();

        assert_eq!(stats1.messages_sent, 20);
        assert_eq!(stats1.messages_dropped, 0);
        assert_eq!(stats2.messages_sent, 20);
        assert_eq!(stats2.messages_dropped, 0);
        assert_eq!(stats3.messages_sent, 20);
        assert_eq!(stats3.messages_dropped, 0);
    }
}
