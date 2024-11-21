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
    password: String,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    status: String,
    message: String,
    token: Option<String>,
    messages: Option<Vec<ChatMessage>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ChatMessage {
    id: u64,
    timestamp: u64,
    user: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct OutgoingMessage {
    message: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Welcome to chaTTY!");

    // Get server address
    print!("Provide server address and port: ");
    let mut server_address = String::new();
    io::stdin().read_line(&mut server_address)?;
    server_address = server_address.trim().to_string();
    
    // Get username and password
    print!("Enter username: ");
    io::stdout().flush()?;
    let mut username = String::new();
    io::stdin().read_line(&mut username)?;
    username = username.trim().to_string();

    print!("Enter password: ");
    io::stdout().flush()?;
    let mut password = String::new();
    io::stdin().read_line(&mut password)?;
    password = password.trim().to_string();

    // Authenticate
    let token = authenticate(&username, &password, &server_address).await?;
    println!("Authentication successful!");

    // Create WebSocket request with authentication
    let ws_url = format!("ws://{}/ws", server_address);
    let mut request = ws_url.into_client_request()?;
    
    // Set the authorization header
    request.headers_mut().insert(
        "Authorization",
        token.parse().unwrap()
    );

    // Connect to WebSocket with the authenticated request
    let (ws_stream, _) = connect_async(request).await?;
    println!("WebSocket connected!");

    let (mut write, mut read) = ws_stream.split();

    // Spawn a task to handle incoming messages
    let receive_task = tokio::spawn(async move {
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(chat_msg) = serde_json::from_str::<ChatMessage>(&text) {
                        println!("[{}] {}: {}", 
                            chat_msg.user, 
                            chrono::NaiveDateTime::from_timestamp_opt(chat_msg.timestamp as i64, 0)
                                .unwrap_or_default()
                                .format("%H:%M:%S"),
                            chat_msg.content
                        );
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

    // Handle user input
    let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();

    println!("\nStart chatting (type '/quit' to exit):");

    while let Some(line) = stdin.next_line().await? {
        if line.trim() == "/quit" {
            println!("Goodbye!");
            break;
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

    // Clean up
    write.send(Message::Close(None)).await?;
    receive_task.abort();

    Ok(())
}

async fn authenticate(username: &str, password: &str, server_address: &str) -> Result<String, Box<dyn Error>> {
    let client = reqwest::Client::new();
    let auth_request = AuthRequest {
        username: username.to_string(),
        password: password.to_string(),
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
