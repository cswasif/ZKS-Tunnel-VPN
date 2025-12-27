use futures::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, Mutex};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use zks_tunnel_client::swarm_controller::{SwarmController, SwarmControllerConfig};

/// Mock Relay Server for testing
struct MockRelay {
    addr: SocketAddr,
    tx: broadcast::Sender<Message>,
}

impl MockRelay {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, _) = broadcast::channel(100);
        let tx_clone = tx.clone();

        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let tx = tx_clone.clone();
                tokio::spawn(async move {
                    let mut ws_stream = accept_async(stream).await.unwrap();
                    let (mut write, mut read) = ws_stream.split();
                    let mut rx = tx.subscribe();

                    loop {
                        tokio::select! {
                            msg = read.next() => {
                                match msg {
                                    Some(Ok(Message::Text(text))) => {
                                        // Simple echo/broadcast logic for testing
                                        if text.contains("join") {
                                            // Send Welcome/Joined response
                                            let response = r#"{"type":"joined","your_id":"test-peer-1"}"#;
                                            write.send(Message::Text(response.to_string())).await.unwrap();
                                        }
                                    }
                                    Some(Ok(Message::Close(_))) => break,
                                    _ => {}
                                }
                            }
                            Ok(msg) = rx.recv() => {
                                write.send(msg).await.unwrap();
                            }
                        }
                    }
                });
            }
        });

        Self { addr, tx }
    }
}

#[tokio::test]
async fn test_reproduce_insufficient_data_error() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("debug")
        .try_init();

    // 1. Start Mock Relay
    println!("TEST: Starting Mock Relay");
    let relay = MockRelay::start().await;
    println!("TEST: Mock Relay started at {}", relay.addr);
    let relay_url = format!("ws://{}", relay.addr);

    // 2. Configure Swarm Controller to connect to mock relay
    let config = SwarmControllerConfig {
        relay_url: relay_url.clone(),
        room_id: "test-room".to_string(),
        ..Default::default()
    };

    let mut controller = SwarmController::new(config);

    // Spawn controller in background
    tokio::spawn(async move {
        if let Err(e) = controller.start().await {
            eprintln!("Controller error: {}", e);
        }
    });

    // 3. Start Controller (this should connect)
    // We expect this to fail or log the error if the bug is present
    // For reproduction, we might need to inject a specific malformed message

    // Simulate the condition: Send a partial/malformed binary message
    // The "Insufficient data" error usually comes from trying to read more bytes than available
    // during a binary read (e.g. reading 8 bytes for u64 but getting fewer)

    // Wait for connection...
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Inject malformed binary packet (e.g. just 2 bytes, but parser expects header)
    println!("TEST: Sending malformed binary packet");
    relay.tx.send(Message::Binary(vec![0x01, 0x02])).unwrap();
    println!("TEST: Packet sent");

    // Give it time to process and crash/error
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Assertions would go here - for now we just want to see the logs
}
