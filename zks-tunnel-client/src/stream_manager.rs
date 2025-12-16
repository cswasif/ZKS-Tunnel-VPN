//! Stream Manager - Manages multiplexed streams
//!
//! Each stream corresponds to one TCP connection being proxied.

use bytes::Bytes;
use std::collections::HashMap;
use tokio::sync::mpsc;
use zks_tunnel_proto::StreamId;

#[allow(dead_code)]
pub struct StreamManager {
    streams: HashMap<StreamId, StreamState>,
}

#[allow(dead_code)]
struct StreamState {
    tx: mpsc::Sender<Bytes>,
    rx: mpsc::Receiver<Bytes>,
}

#[allow(dead_code)]
impl StreamManager {
    pub fn new() -> Self {
        Self {
            streams: HashMap::new(),
        }
    }

    pub fn create_stream(&mut self, id: StreamId) -> mpsc::Receiver<Bytes> {
        let (tx, rx) = mpsc::channel(100);
        let (_out_tx, out_rx) = mpsc::channel::<Bytes>(100);

        self.streams.insert(id, StreamState { tx, rx: out_rx });
        rx
    }

    pub fn remove_stream(&mut self, id: StreamId) {
        self.streams.remove(&id);
    }

    pub async fn send_to_stream(&self, id: StreamId, data: Bytes) -> bool {
        if let Some(stream) = self.streams.get(&id) {
            stream.tx.send(data).await.is_ok()
        } else {
            false
        }
    }
}
