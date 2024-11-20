use tokio::sync::{Mutex};
use tokio_tungstenite::{WebSocketStream, MaybeTlsStream};
use tokio::net::TcpStream;
use futures::{SinkExt, StreamExt};
use tokio::task;
use serde_json;
use std::sync::Arc;
use std::error::Error;
use tokio_tungstenite::tungstenite::protocol::Message;

async fn send_chat_message(
    ws_stream: &Arc<Mutex<WebSocketStream<MaybeTlsStream<TcpStream>>>>,
    message: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let message_json = serde_json::json!({ "message": message }).to_string();

    // Lock the WebSocket stream for sending the message
    let mut ws_stream = ws_stream.lock().await; // Correctly awaiting the lock

    // Send the message over the WebSocket stream
    ws_stream.send(Message::Text(message_json)).await?;

    Ok(())
}

async fn handle_incoming_messages(
    ws_stream: Arc<Mutex<WebSocketStream<MaybeTlsStream<TcpStream>>>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Lock the WebSocket stream to start reading
    let mut ws_stream = ws_stream.lock().await; // Correctly awaiting the lock

    while let Some(message) = ws_stream.next().await {
        match message {
            Ok(Message::Text(text)) => {
                println!("Received message: {}", text);
            }
            Ok(Message::Close(close)) => {
                println!("Connection closed: {:?}", close);
                break;
            }
            Err(e) => {
                eprintln!("Error receiving message: {}", e);
                break;
            }
            _ => {}
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    // Connect to WebSocket server
    let (ws_stream, _) = tokio_tungstenite::connect_async("ws://127.0.0.1:8080/ws")
        .await
        .expect("Failed to connect");

    // Wrap the WebSocket stream in an Arc<Mutex<_>> for shared access
    let ws_stream = Arc::new(Mutex::new(ws_stream));

    // Spawn a task to handle incoming messages, pass cloned ws_stream
    let incoming_task = tokio::spawn({
        let ws_stream_clone = Arc::clone(&ws_stream); // Clone Arc to use inside async block
        async move {
            handle_incoming_messages(ws_stream_clone).await
        }
    });

    // Send a chat message
    send_chat_message(&ws_stream, "Hello, WebSocket!").await?;

    // Wait for the incoming message handler to complete
    incoming_task.await??; // Unwrap the Result from the spawned task

    Ok(())
}

