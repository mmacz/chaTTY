use axum::serve;
use axum::{
    extract::{
        State,
        WebSocketUpgrade,
        ws::{WebSocket, Message},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use log::{Level, LevelFilter, Metadata, Record};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, Mutex};
use futures::{
    sink::SinkExt,
    stream::StreamExt,
};

use std::env;

struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            println!("{} - {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

#[derive(Debug, Clone, Serialize)]
struct StoredMessage {
    id: u64,
    timestamp: u64,
    user: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct AuthRequest {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    message: String,
}

#[derive(Debug, Serialize)]
struct ApiResponse {
    status: String,
    message: String,
    token: Option<String>,
    messages: Option<Vec<StoredMessage>>,
}

#[derive(Clone)]
struct AppState {
    broadcast_tx: broadcast::Sender<StoredMessage>,
    authenticated_users: Arc<Mutex<HashMap<String, String>>>,
    message_history: Arc<Mutex<VecDeque<StoredMessage>>>,
}

#[derive(Debug)]
enum AppError {
    AuthError(String),
    ChatError(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::AuthError(msg) => (StatusCode::UNAUTHORIZED, msg),
            AppError::ChatError(msg) => (StatusCode::BAD_REQUEST, msg),
        };

        let body = Json(ApiResponse {
            status: "error".to_string(),
            message: error_message,
            token: None,
            messages: None,
        });

        (status, body).into_response()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    static LOGGER: SimpleLogger = SimpleLogger;
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(LevelFilter::Info))
        .expect("Failed to initialize logger!");

    let port: u16 = match env::var("CHATTY_PORT").unwrap_or("8080".to_string()).parse() {
        Ok(p) => p,
        _ => {
            log::info!("Invalid port selected: {}, defaulting to: 8080...", env::var("CHATTY_PORT")?);
            8080
        },
    };
    let (broadcast_tx, _) = broadcast::channel::<StoredMessage>(100);

    let state = AppState {
        broadcast_tx: broadcast_tx.clone(),
        authenticated_users: Arc::new(Mutex::new(HashMap::new())),
        message_history: Arc::new(Mutex::new(VecDeque::with_capacity(100))),
    };

    let app = Router::new()
        .route("/auth", post(handle_auth))
        .route("/chat", post(handle_chat))
        .route("/messages", get(get_messages))
        .route("/status", get(handle_status))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;
    log::info!("Server running on http://127.0.0.1:{}", port);

    serve(listener, app.into_make_service()).await?;

    Ok(())
}

async fn handle_auth(
    State(state): State<AppState>,
    Json(auth): Json<AuthRequest>,
) -> Result<Json<ApiResponse>, AppError> {
    if auth.username.is_empty() || auth.password.is_empty() {
        return Err(AppError::AuthError(
            "Invalid username or password".to_string(),
        ));
    }

    let mut users = state.authenticated_users.lock().await;

    let token = format!("token_{}", auth.username);
    users.insert(auth.username.clone(), token.clone());

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        message: "Authentication successful".to_string(),
        token: Some(token),
        messages: None,
    }))
}

async fn handle_chat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(chat): Json<ChatMessage>,
) -> Result<Json<ApiResponse>, AppError> {
    let auth_header = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::AuthError("Missing authorization header".to_string()))?;

    let users = state.authenticated_users.lock().await;
    let username = users
        .iter()
        .find(|(_, token)| **token == auth_header)
        .map(|(username, _)| username.clone())
        .ok_or_else(|| AppError::AuthError("Invalid token".to_string()))?;

    log::info!("Received message from {}: {}", username, chat.message);

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let message = StoredMessage {
        id: timestamp,
        timestamp,
        user: username,
        content: chat.message,
    };

    // TODO:
    // Store message in history
    // replace with db?
    {
        let mut history = state.message_history.lock().await;
        if history.len() >= 100 {
            history.pop_front();
        }
        history.push_back(message.clone());
    }

    if let Err(e) = state.broadcast_tx.send(message.clone()) {
        log::warn!("No active subscribers: {}", e);
    }

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        message: "Message sent".to_string(),
        token: None,
        messages: Some(vec![message]),
    }))
}

async fn get_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse>, AppError> {
    let auth_header = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::AuthError("Missing authorization header".to_string()))?;

    let users = state.authenticated_users.lock().await;
    if !users.values().any(|token| token == auth_header) {
        return Err(AppError::AuthError("Invalid token".to_string()));
    }

    let history = state.message_history.lock().await;
    let messages: Vec<StoredMessage> = history.iter().cloned().collect();

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        message: "Messages retrieved".to_string(),
        token: None,
        messages: Some(messages),
    }))
}

async fn handle_status() -> Json<ApiResponse> {
    Json(ApiResponse {
        status: "success".to_string(),
        message: "Server is running".to_string(),
        token: None,
        messages: None,
    })
}

async fn ws_handler(
    ws: WebSocketUpgrade, 
    headers: HeaderMap,
    State(state): State<AppState>
) -> Result<Response, AppError> {
    let auth_header = headers
        .get("authorization")  // Changed to use standard authorization header
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::AuthError("Missing authorization header".to_string()))?;

    let users = state.authenticated_users.clone();
    let users = users.lock().await;
    let username = users
        .iter()
        .find(|(_, token)| *token == auth_header)
        .map(|(username, _)| username.clone())
        .ok_or_else(|| AppError::AuthError("Invalid token".to_string()))?;

    Ok(ws.on_upgrade(move |socket| handle_websocket(socket, state, username)))
}

async fn handle_websocket(
    socket: WebSocket, 
    state: AppState,
    username: String,
) {
    log::info!("WebSocket connection established for user: {}", username);

    let (mut sender, mut receiver) = socket.split();

    let mut broadcast_rx = state.broadcast_tx.subscribe();
    let send_task = tokio::spawn(async move {
        while let Ok(msg) = broadcast_rx.recv().await {
            if let Ok(json_msg) = serde_json::to_string(&msg) {
                if sender.send(Message::Text(json_msg)).await.is_err() {
                    break;
                }
            }
        }
    });

    let state_clone = state.clone();
    let username_clone = username.clone();

    let receive_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    if let Ok(chat_message) = serde_json::from_str::<ChatMessage>(&text) {
                        let timestamp = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs();

                        let message = StoredMessage {
                            id: timestamp,
                            timestamp,
                            user: username_clone.clone(),
                            content: chat_message.message,
                        };

                        {
                            let mut history = state_clone.message_history.lock().await;
                            if history.len() >= 100 {
                                history.pop_front();
                            }
                            history.push_back(message.clone());
                        }

                        if let Err(e) = state_clone.broadcast_tx.send(message) {
                            log::error!("Failed to broadcast message: {}", e);
                            break;
                        }
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = send_task => log::info!("Send task completed for user: {}", username),
        _ = receive_task => log::info!("Receive task completed for user: {}", username),
    }

    log::info!("WebSocket connection closed for user: {}", username);
}

async fn authenticate_websocket_user(
    state: &AppState, 
    token: Option<String>
) -> Result<String, AppError> {
    let token = token.ok_or_else(|| AppError::AuthError("No token provided".to_string()))?;

    let users = state.authenticated_users.lock().await;
    
    users.iter()
        .find(|(_, stored_token)| stored_token.to_string() == token)
        .map(|(username, _)| username.clone())
        .ok_or_else(|| AppError::AuthError("Invalid token".to_string()))
}

async fn process_websocket_message(
    state: &AppState, 
    username: String, 
    message_text: String
) -> Result<(), AppError> {
    let chat_message = ChatMessage { message: message_text };
    
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let message = StoredMessage {
        id: timestamp,
        timestamp,
        user: username,
        content: chat_message.message,
    };

    {
        let mut history = state.message_history.lock().await;
        if history.len() >= 100 {
            history.pop_front();
        }
        history.push_back(message.clone());
    }

    state.broadcast_tx.send(message.clone())
        .map_err(|_| AppError::ChatError("Failed to broadcast message".to_string()))?;

    Ok(())
}
