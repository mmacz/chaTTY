use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::io::{self, Write};
use tokio::io::AsyncBufReadExt;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
};
use reqwest;

#[derive(Debug, Serialize)]
struct AuthRequest {
    username: String,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    status: String,
    message: String,
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")] // To deserialize based on the "type" field
enum ChatEvent {
    Message {
        id: u64,
        timestamp: u64,
        user: String,
        content: String,
    },
    UserJoined {
        id: u64,
        timestamp: u64,
        user: String,
    },
    UserLeft {
        id: u64,
        timestamp: u64,
        user: String,
    },
}

#[derive(Debug, Serialize)]
struct OutgoingMessage {
    message: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Welcome to chaTTY!");

    print!("Provide server address and port: ");
    io::stdout().flush()?;
    let mut server_address = String::new();
    io::stdin().read_line(&mut server_address)?;
    server_address = server_address.trim().to_string();
    
    print!("Enter username: ");
    io::stdout().flush()?;
    let mut username = String::new();
    io::stdin().read_line(&mut username)?;
    username = username.trim().to_string();

    let token = authenticate(&username, &server_address).await?;
    println!("Authentication successful!");

    let ws_url = format!("ws://{}/ws", server_address);
    let mut request = ws_url.into_client_request()?;
    request.headers_mut().insert(
        "Authorization",
        token.parse().unwrap()
    );

    let (ws_stream, _) = connect_async(request).await?;
    println!("WebSocket connected!");

    let (mut write, mut read) = ws_stream.split();

    let receive_task = tokio::spawn(async move {
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    match serde_json::from_str::<ChatEvent>(&text) {
                        Ok(ChatEvent::Message { user, content, timestamp, .. }) => {
                            println!(
                                "[{}] {}: {}",
                                chrono::DateTime::from_timestamp(timestamp as i64, 0)
                                    .unwrap_or_default()
                                    .format("%H:%M:%S"),
                                user,
                                content
                            );
                        }
                        Ok(ChatEvent::UserJoined { user, timestamp, .. }) => {
                            println!(
                                "[{}] *** {} has joined the chat ***",
                                chrono::DateTime::from_timestamp(timestamp as i64, 0)
                                    .unwrap_or_default()
                                    .format("%H:%M:%S"),
                                user
                            );
                        }
                        Ok(ChatEvent::UserLeft { user, timestamp, .. }) => {
                            println!(
                                "[{}] *** {} has left the chat ***",
                                chrono::DateTime::from_timestamp(timestamp as i64, 0)
                                    .unwrap_or_default()
                                    .format("%H:%M:%S"),
                                user
                            );
                        }
                        Err(_) => {
                            eprintln!("Received an unknown message format.");
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    println!("Connection closed by server");
                    break;
                }
                Err(e) => {
                    eprintln!("Error receiving message: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();

    println!("\nStart chatting (type '/quit' to exit):");

    while let Some(line) = stdin.next_line().await? {
        if line.trim() == "/quit" {
            println!("Goodbye.");
            if let Err(e) = write.send(Message::Close(None)).await {
                eprintln!("Error sending close frame: {}", e);
            }
            break; // Exit input loop
        }

        let message = OutgoingMessage {
            message: line.trim().to_string(),
        };

        if let Ok(json) = serde_json::to_string(&message) {
            if let Err(e) = write.send(Message::Text(json)).await {
                eprintln!("Error sending message: {}", e);
                break;
            }
        }
    }

    receive_task.abort();

    Ok(())
}

async fn authenticate(username: &str, server_address: &str) -> Result<String, Box<dyn Error>> {
    let client = reqwest::Client::new();
    let auth_request = AuthRequest {
        username: username.to_string(),
    };

    let response = client
        .post(&format!("http://{}/auth", server_address))
        .json(&auth_request)
        .send()
        .await?;

    let api_response: ApiResponse = response.json().await?;
    
    match api_response.token {
        Some(token) => Ok(token),
        None => Err("Authentication failed".into()),
    }
}


