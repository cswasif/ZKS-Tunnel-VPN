//! TunnelSession - Durable Object for persistent WebSocket connections
//!
//! Production-grade implementation with:
//! - Full bidirectional TCP relay
//! - Proper error handling and logging
//! - Memory-efficient buffer management
//! - Hibernation support for zero-cost idle connections

use worker::*;
use worker::wasm_bindgen::JsCast;
use zks_tunnel_proto::{TunnelMessage, StreamId};
use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;

/// Stream information including write half of socket
struct StreamInfo {
    socket: tokio::io::WriteHalf<Socket>,
}

#[durable_object]
pub struct TunnelSession {
    state: State,
    #[allow(dead_code)]
    env: Env,
    /// Active stream tracking - maps stream_id -> socket
    active_streams: Rc<RefCell<HashMap<StreamId, StreamInfo>>>,
    /// Connection counter for metrics
    connection_count: Rc<RefCell<u32>>,
}

impl DurableObject for TunnelSession {
    fn new(state: State, env: Env) -> Self {
        console_log!("[TunnelSession] Initializing new session");
        Self {
            state,
            env,
            active_streams: Rc::new(RefCell::new(HashMap::new())),
            connection_count: Rc::new(RefCell::new(0)),
        }
    }

    async fn fetch(&self, req: Request) -> Result<Response> {
        let upgrade = req.headers().get("Upgrade")?;

        if upgrade.as_deref() != Some("websocket") {
            return Response::error("Expected WebSocket upgrade", 426);
        }

        // Create WebSocket pair
        let pair = WebSocketPair::new()?;
        let server = pair.server;
        let client = pair.client;

        // Accept with hibernation for cost efficiency (zero CPU when idle)
        self.state.accept_web_socket(&server);

        *self.connection_count.borrow_mut() += 1;
        console_log!("[TunnelSession] Connection #{} established", self.connection_count.borrow());

        Response::from_websocket(client)
    }

    async fn websocket_message(
        &self,
        ws: WebSocket,
        message: WebSocketIncomingMessage,
    ) -> Result<()> {
        match message {
            WebSocketIncomingMessage::Binary(data) => {
                self.handle_binary_message(&ws, &data).await?;
            }
            WebSocketIncomingMessage::String(text) => {
                // Handle text messages (could be JSON commands in future)
                console_log!("[TunnelSession] Text message received: {} bytes", text.len());
            }
        }
        Ok(())
    }

    async fn websocket_close(
        &self,
        _ws: WebSocket,
        code: usize,
        reason: String,
        was_clean: bool,
    ) -> Result<()> {
        console_log!(
            "[TunnelSession] Connection closed: code={}, reason={}, clean={}",
            code, reason, was_clean
        );
        
        // Clean up all streams
        let stream_count = self.active_streams.borrow().len();
        self.active_streams.borrow_mut().clear();
        
        console_log!("[TunnelSession] Cleaned up {} streams", stream_count);
        Ok(())
    }

    async fn websocket_error(&self, _ws: WebSocket, error: Error) -> Result<()> {
        console_error!("[TunnelSession] WebSocket error: {:?}", error);
        Ok(())
    }
}

impl TunnelSession {
    /// Handle binary protocol messages with zero-copy parsing
    async fn handle_binary_message(&self, ws: &WebSocket, data: &[u8]) -> Result<()> {
        let msg = match TunnelMessage::decode(data) {
            Ok(m) => m,
            Err(e) => {
                console_error!("[TunnelSession] Protocol decode error: {:?}", e);
                return Ok(()); // Don't propagate - invalid messages are dropped
            }
        };

        match msg {
            TunnelMessage::Connect { stream_id, host, port } => {
                // Validate host to prevent SSRF
                if !Self::is_valid_host(&host) {
                    console_warn!("[TunnelSession] Rejected invalid host: {}", host);
                    Self::send_error(ws, stream_id, 400, "Invalid host");
                    return Ok(());
                }
                
                console_log!("[TunnelSession] CONNECT stream={} to {}:{}", stream_id, host, port);
                self.handle_connect(ws, stream_id, &host, port).await?;
            }
            TunnelMessage::Data { stream_id, payload } => {
                self.handle_data(ws, stream_id, &payload).await?;
            }
            TunnelMessage::Close { stream_id } => {
                self.handle_close(stream_id).await?;
            }
            TunnelMessage::Ping => {
                // Fast path for keepalive
                let pong = TunnelMessage::Pong.encode();
                let _ = ws.send_with_bytes(&pong);
            }
            TunnelMessage::Pong => {
                // Client responded to our ping - connection is alive
            }
            TunnelMessage::ErrorReply { .. } => {
                // Unexpected - client shouldn't send errors
                console_warn!("[TunnelSession] Received unexpected ErrorReply from client");
            }
            TunnelMessage::DnsQuery { request_id, query } => {
                console_log!("[TunnelSession] DNS query request_id={} len={}", request_id, query.len());
                self.handle_dns_query(ws, request_id, &query).await?;
            }
            TunnelMessage::DnsResponse { .. } => {
                // Unexpected - worker sends responses, not client
                console_warn!("[TunnelSession] Received unexpected DnsResponse from client");
            }
            TunnelMessage::UdpDatagram { request_id, host, port, payload } => {
                console_log!("[TunnelSession] UDP datagram request_id={} to {}:{} len={}", 
                    request_id, host, port, payload.len());
                // Note: Workers don't have raw UDP socket support
                // For DNS (port 53), we redirect to DoH
                if port == 53 {
                    self.handle_dns_query(ws, request_id, &payload).await?;
                } else {
                    // Other UDP traffic - send error as Workers can't relay raw UDP
                    let error_msg = TunnelMessage::ErrorReply {
                        stream_id: request_id,
                        code: 501,
                        message: "UDP not supported (except DNS via DoH)".to_string(),
                    };
                    let _ = ws.send_with_bytes(&error_msg.encode());
                }
            }
        }

        Ok(())
    }

    /// Validate hostname to prevent SSRF attacks
    fn is_valid_host(host: &str) -> bool {
        // Block internal/private networks
        let blocked_prefixes = ["127.", "10.", "192.168.", "172.16.", "172.17.", 
                                "172.18.", "172.19.", "172.20.", "172.21.", "172.22.",
                                "172.23.", "172.24.", "172.25.", "172.26.", "172.27.",
                                "172.28.", "172.29.", "172.30.", "172.31.", "169.254.",
                                "0.", "localhost", "::1", "fc", "fd", "fe80"];
        
        let host_lower = host.to_lowercase();
        
        for prefix in blocked_prefixes {
            if host_lower.starts_with(prefix) {
                return false;
            }
        }
        
        // Block empty or too long hosts
        if host.is_empty() || host.len() > 253 {
            return false;
        }
        
        true
    }

    /// Send error message to client
    fn send_error(ws: &WebSocket, stream_id: StreamId, code: u16, message: &str) {
        let error_msg = TunnelMessage::ErrorReply {
            stream_id,
            code,
            message: message.to_string(),
        };
        let _ = ws.send_with_bytes(&error_msg.encode());
    }

    /// Handle CONNECT command - establish outbound TCP connection and spawn reader task
    async fn handle_connect(
        &self,
        ws: &WebSocket,
        stream_id: StreamId,
        host: &str,
        port: u16,
    ) -> Result<()> {
        // Check for duplicate stream ID
        if self.active_streams.borrow().contains_key(&stream_id) {
            console_warn!("[TunnelSession] Duplicate stream ID: {}", stream_id);
            Self::send_error(ws, stream_id, 409, "Stream ID already in use");
            return Ok(());
        }

        let address = format!("{}:{}", host, port);
        console_log!("[TunnelSession] Connecting to {}", address);

        // Use Socket::builder().connect() for outbound TCP
        match Socket::builder().connect(host, port) {
            Ok(socket) => {
                console_log!("[TunnelSession] Connected to {}", address);
                
                // Split socket into read and write halves
                let (mut read_half, write_half) = tokio::io::split(socket);
                
                // Spawn a task to read from socket using tokio AsyncReadExt
                let ws_for_reader = ws.clone();
                let active_streams_for_reader = self.active_streams.clone();
                
                spawn_local(async move {
                    use tokio::io::AsyncReadExt;
                    let mut buffer = vec![0u8; 16384];
                    
                    loop {
                        // Read from socket (Server -> Client)
                        match read_half.read(&mut buffer).await {
                            Ok(0) => {
                                // Socket closed by remote
                                console_log!("[TunnelSession] Socket {} closed by remote", stream_id);
                                
                                // Notify client
                                let close_msg = TunnelMessage::Close { stream_id };
                                let _ = ws_for_reader.send_with_bytes(&close_msg.encode());
                                
                                // Clean up
                                active_streams_for_reader.borrow_mut().remove(&stream_id);
                                break;
                            }
                            Ok(n) => {
                                // Send DATA message to client
                                let msg = TunnelMessage::Data {
                                    stream_id,
                                    payload: bytes::Bytes::copy_from_slice(&buffer[..n]),
                                };
                                if ws_for_reader.send_with_bytes(&msg.encode()).is_err() {
                                    console_error!("[TunnelSession] Failed to send data for stream {}", stream_id);
                                    break;
                                }
                            }
                            Err(e) => {
                                console_error!("[TunnelSession] Socket read error on stream {}: {:?}", stream_id, e);
                                Self::send_error(&ws_for_reader, stream_id, 500, "Socket read error");
                                active_streams_for_reader.borrow_mut().remove(&stream_id);
                                break;
                            }
                        }
                    }
                    console_log!("[TunnelSession] Reader task exiting for stream {}", stream_id);
                });
                
                // Store write half for Client -> Server direction (handled in handle_data)
                self.active_streams.borrow_mut().insert(stream_id, StreamInfo {
                    socket: write_half,
                });
                
                console_log!("[TunnelSession] Stream {} ready for bidirectional data transfer", stream_id);
            }
            Err(e) => {
                console_error!("[TunnelSession] Connect failed to {}: {:?}", address, e);
                Self::send_error(ws, stream_id, 502, &format!("Connection failed: {:?}", e));
            }
        }

        Ok(())
    }

    /// Handle DATA command - forward data to TCP socket (Client -> Server)
    async fn handle_data(&self, ws: &WebSocket, stream_id: StreamId, payload: &[u8]) -> Result<()> {
        use tokio::io::AsyncWriteExt;
        
        // Get mutable access to streams
        let mut streams = self.active_streams.borrow_mut();
        
        if let Some(stream_info) = streams.get_mut(&stream_id) {
            // Write data to socket
            match stream_info.socket.write_all(payload).await {
                Ok(()) => {
                    console_log!("[TunnelSession] Wrote {} bytes to stream {}", payload.len(), stream_id);
                }
                Err(e) => {
                    console_error!("[TunnelSession] Socket write error on stream {}: {:?}", stream_id, e);
                    streams.remove(&stream_id);
                    Self::send_error(ws, stream_id, 500, &format!("Write error: {:?}", e));
                }
            }
        } else {
            console_warn!("[TunnelSession] DATA for unknown stream {}", stream_id);
            Self::send_error(ws, stream_id, 404, "Stream not found");
        }
        
        Ok(())
    }

    /// Handle CLOSE command - close TCP socket
    async fn handle_close(&self, stream_id: StreamId) -> Result<()> {
        if let Some(info) = self.active_streams.borrow_mut().remove(&stream_id) {
            console_log!("[TunnelSession] CLOSE stream={}", stream_id);
            
            // Socket will be dropped automatically, closing the connection
            drop(info);
            
            console_log!("[TunnelSession] Stream {} closed gracefully", stream_id);
        } else {
            console_log!("[TunnelSession] CLOSE stream={} (not found)", stream_id);
        }
        Ok(())
    }

    /// Handle DNS query via DoH (DNS-over-HTTPS)
    /// Uses Cloudflare's 1.1.1.1 DoH service
    async fn handle_dns_query(&self, ws: &WebSocket, request_id: u32, query: &[u8]) -> Result<()> {
        // Use resolve_dns_via_doh which uses web_sys fetch directly
        let response = self.resolve_dns_via_doh(query).await;
        
        match response {
            Ok(dns_response) => {
                let msg = TunnelMessage::DnsResponse {
                    request_id,
                    response: bytes::Bytes::from(dns_response),
                };
                let _ = ws.send_with_bytes(&msg.encode());
                console_log!("[TunnelSession] DNS response sent for request_id={}", request_id);
            }
            Err(e) => {
                console_error!("[TunnelSession] DoH resolution failed: {:?}", e);
                // Send error reply
                let error_msg = TunnelMessage::ErrorReply {
                    stream_id: request_id,
                    code: 503,
                    message: format!("DNS resolution failed: {:?}", e),
                };
                let _ = ws.send_with_bytes(&error_msg.encode());
            }
        }
        
        Ok(())
    }

    /// Resolve DNS query via DoH using native fetch
    async fn resolve_dns_via_doh(&self, query: &[u8]) -> Result<Vec<u8>> {
        use worker::wasm_bindgen::JsValue;
        use worker::js_sys::{ArrayBuffer, Uint8Array};
        
        // Cloudflare DoH endpoint
        let url = "https://1.1.1.1/dns-query";
        
        // Create the request using web_sys
        let opts = web_sys::RequestInit::new();
        opts.set_method("POST");
        
        // Set body as ArrayBuffer
        let body_array = Uint8Array::new_with_length(query.len() as u32);
        body_array.copy_from(query);
        opts.set_body(&body_array.buffer());
        
        // Create headers
        let headers = web_sys::Headers::new().map_err(|_| Error::from("Headers creation failed"))?;
        headers.set("Content-Type", "application/dns-message").map_err(|_| Error::from("Set header failed"))?;
        headers.set("Accept", "application/dns-message").map_err(|_| Error::from("Set header failed"))?;
        opts.set_headers(&headers);
        
        // Create request
        let request = web_sys::Request::new_with_str_and_init(url, &opts)
            .map_err(|_| Error::from("Request creation failed"))?;
        
        // Use worker's Fetch
        let global = worker::js_sys::global();
        let fetch_fn = worker::js_sys::Reflect::get(&global, &JsValue::from_str("fetch"))
            .map_err(|_| Error::from("fetch not found"))?;
        
        // Call fetch
        let fetch_fn = fetch_fn.dyn_into::<worker::js_sys::Function>()
            .map_err(|_| Error::from("fetch is not a function"))?;
        
        let promise = fetch_fn.call1(&JsValue::NULL, &request)
            .map_err(|_| Error::from("fetch call failed"))?;
        
        let promise = worker::js_sys::Promise::from(promise);
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        
        let response = future.await.map_err(|e| Error::from(format!("fetch failed: {:?}", e)))?;
        let response: web_sys::Response = response.dyn_into()
            .map_err(|_| Error::from("response cast failed"))?;
        
        if !response.ok() {
            return Err(Error::from(format!("DoH returned status {}", response.status())));
        }
        
        // Get response body as ArrayBuffer
        let body_promise = response.array_buffer()
            .map_err(|_| Error::from("array_buffer() failed"))?;
        
        let body_future = wasm_bindgen_futures::JsFuture::from(body_promise);
        let body = body_future.await.map_err(|_| Error::from("body await failed"))?;
        
        let array_buffer: ArrayBuffer = body.dyn_into()
            .map_err(|_| Error::from("body cast failed"))?;
        
        let uint8_array = Uint8Array::new(&array_buffer);
        let mut vec = vec![0u8; uint8_array.length() as usize];
        uint8_array.copy_to(&mut vec);
        
        Ok(vec)
    }
}
