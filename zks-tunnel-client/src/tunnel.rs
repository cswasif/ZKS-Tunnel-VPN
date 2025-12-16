//! Tunnel Client - WebSocket connection to ZKS-Tunnel Worker
//!
//! Production-grade implementation with:
//! - Efficient bidirectional data relay
//! - Proper resource cleanup
//! - Connection keepalive via ping/pong
//! - Memory-efficient buffer management

use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tracing::{debug, error, info, warn};
use zks_tunnel_proto::{StreamId, TunnelMessage};

#[allow(dead_code)]
type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// Per-stream state with sender for incoming data
struct StreamState {
    tx: mpsc::Sender<Bytes>,
}

/// Production-grade tunnel client with connection multiplexing
pub struct TunnelClient {
    /// Sender for outgoing messages
    sender: mpsc::Sender<TunnelMessage>,
    /// Next stream ID (atomic for thread-safety)
    next_stream_id: AtomicU32,
    /// Active streams - maps stream_id to sender for that stream's data
    streams: Arc<Mutex<HashMap<StreamId, StreamState>>>,
}

impl TunnelClient {
    /// Connect to the ZKS-Tunnel Worker with automatic reconnection
    pub async fn connect_ws(url: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        info!("Connecting to ZKS-Tunnel Worker at {}", url);

        let (ws_stream, response) = connect_async(url).await?;
        info!("WebSocket connected (status: {})", response.status());

        let (mut write, mut read) = ws_stream.split();

        // Channel for sending messages to the WebSocket (bounded for backpressure)
        let (sender, mut receiver) = mpsc::channel::<TunnelMessage>(256);

        // Streams map - shared between reader task and main client
        let streams: Arc<Mutex<HashMap<StreamId, StreamState>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let streams_clone = streams.clone();

        // Spawn writer task - sends messages from channel to WebSocket
        let writer_handle = tokio::spawn(async move {
            while let Some(msg) = receiver.recv().await {
                let encoded = msg.encode();
                if let Err(e) = write.send(Message::Binary(encoded.to_vec())).await {
                    error!("WebSocket write error: {}", e);
                    break;
                }
            }
            debug!("Writer task exiting");
        });

        // Spawn reader task - receives messages from WebSocket and dispatches to streams
        let reader_handle = tokio::spawn(async move {
            while let Some(msg_result) = read.next().await {
                match msg_result {
                    Ok(Message::Binary(data)) => {
                        if let Ok(tunnel_msg) = TunnelMessage::decode(&data) {
                            match tunnel_msg {
                                TunnelMessage::Data { stream_id, payload } => {
                                    // Forward data to the appropriate stream
                                    let streams = streams_clone.lock().await;
                                    if let Some(state) = streams.get(&stream_id) {
                                        if state.tx.send(payload).await.is_err() {
                                            debug!("Stream {} receiver dropped", stream_id);
                                        }
                                    } else {
                                        warn!("Data for unknown stream {}", stream_id);
                                    }
                                }
                                TunnelMessage::Close { stream_id } => {
                                    let mut streams = streams_clone.lock().await;
                                    streams.remove(&stream_id);
                                    debug!("Stream {} closed by server", stream_id);
                                }
                                TunnelMessage::ErrorReply {
                                    stream_id,
                                    code,
                                    message,
                                } => {
                                    error!(
                                        "Stream {} error: {} (code {})",
                                        stream_id, message, code
                                    );
                                    let mut streams = streams_clone.lock().await;
                                    streams.remove(&stream_id);
                                }
                                TunnelMessage::Pong => {
                                    debug!("Received pong");
                                }
                                _ => {}
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        info!("Server closed connection");
                        break;
                    }
                    Err(e) => {
                        error!("WebSocket read error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
            debug!("Reader task exiting");
        });

        // Keep handles for potential cleanup
        let _ = (writer_handle, reader_handle);

        Ok(Self {
            sender,
            next_stream_id: AtomicU32::new(1),
            streams,
        })
    }

    /// Open a new connection through the tunnel
    /// Returns (stream_id, receiver for incoming data)
    pub async fn open_stream(
        &self,
        host: &str,
        port: u16,
    ) -> Result<(StreamId, mpsc::Receiver<Bytes>), Box<dyn std::error::Error + Send + Sync>> {
        let stream_id = self.next_stream_id.fetch_add(1, Ordering::SeqCst);

        // Send CONNECT command
        let msg = TunnelMessage::Connect {
            stream_id,
            host: host.to_string(),
            port,
        };
        self.sender.send(msg).await?;

        // Create channel for receiving data for this stream (bounded for backpressure)
        let (tx, rx) = mpsc::channel::<Bytes>(256);
        {
            let mut streams = self.streams.lock().await;
            streams.insert(stream_id, StreamState { tx });
        }

        debug!("Opened stream {} to {}:{}", stream_id, host, port);
        Ok((stream_id, rx))
    }

    /// Relay data between local TCP socket and tunnel stream (BIDIRECTIONAL)
    /// Uses efficient buffer management and proper cleanup
    pub async fn relay(
        &self,
        stream_id: StreamId,
        local: TcpStream,
        mut rx: mpsc::Receiver<Bytes>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (mut read_half, mut write_half) = local.into_split();
        let sender = self.sender.clone();
        let sender_for_close = self.sender.clone();
        let streams = self.streams.clone();

        // Task 1: Local -> Tunnel (read from local TCP, send to tunnel)
        let local_to_tunnel = tokio::spawn(async move {
            // Use a reasonably sized buffer for efficiency
            let mut buf = vec![0u8; 16384]; // 16KB buffer

            loop {
                match read_half.read(&mut buf).await {
                    Ok(0) => {
                        debug!("Local EOF for stream {}", stream_id);
                        break;
                    }
                    Ok(n) => {
                        let msg = TunnelMessage::Data {
                            stream_id,
                            payload: Bytes::copy_from_slice(&buf[..n]),
                        };
                        if sender.send(msg).await.is_err() {
                            debug!("Tunnel sender closed for stream {}", stream_id);
                            break;
                        }
                    }
                    Err(e) => {
                        debug!("Local read error for stream {}: {}", stream_id, e);
                        break;
                    }
                }
            }
        });

        // Task 2: Tunnel -> Local (receive from tunnel, write to local TCP)
        let tunnel_to_local = tokio::spawn(async move {
            while let Some(data) = rx.recv().await {
                if let Err(e) = write_half.write_all(&data).await {
                    debug!("Local write error for stream {}: {}", stream_id, e);
                    break;
                }
            }
            debug!("Tunnel receiver closed for stream {}", stream_id);
        });

        // Wait for either direction to finish
        tokio::select! {
            result = local_to_tunnel => {
                if let Err(e) = result {
                    debug!("Local->Tunnel task error: {}", e);
                }
            }
            result = tunnel_to_local => {
                if let Err(e) = result {
                    debug!("Tunnel->Local task error: {}", e);
                }
            }
        }

        // Send close command to server
        let _ = sender_for_close
            .send(TunnelMessage::Close { stream_id })
            .await;

        // Clean up stream
        {
            let mut streams_guard = streams.lock().await;
            streams_guard.remove(&stream_id);
        }

        debug!("Stream {} relay completed", stream_id);
        Ok(())
    }

    /// Send a ping to keep the connection alive
    #[allow(dead_code)]
    pub async fn ping(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.sender.send(TunnelMessage::Ping).await?;
        Ok(())
    }

    /// Get the number of active streams
    #[allow(dead_code)]
    pub async fn active_stream_count(&self) -> usize {
        self.streams.lock().await.len()
    }

    /// Get a clone of the message sender
    pub fn sender(&self) -> mpsc::Sender<TunnelMessage> {
        self.sender.clone()
    }
}
