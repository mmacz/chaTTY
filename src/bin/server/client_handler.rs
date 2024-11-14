use tokio::net::TcpStream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, Mutex};
use log::info;

use std::sync::Arc;
use std::collections::HashMap;

use uuid::Uuid;

pub type ClientList = Arc<Mutex<HashMap<Uuid, broadcast::Sender<String>>>>;

pub async fn handle_client(
    socket: TcpStream,
    tx: broadcast::Sender<String>,
    clients: ClientList,
) {
    let (reader, mut writer) = socket.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    let client_id = Uuid::new_v4();
    {
        let mut clients = clients.lock().await;
        clients.insert(client_id, tx.clone());
    }

    let tx_clone = tx.clone();
    let client_task = tokio::spawn(async move {
        while let Ok(bytes) = reader.read_line(&mut line).await {
            if bytes == 0 {
                info!("Client disconnected");
                break;
            }
            let msg = line.clone();
            if tx_clone.send(msg.clone()).is_err() {
                info!("Failed to broadcast message");
            }
            line.clear();
        }
    });

    let mut rx = tx.subscribe();
    let writer_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if writer.write_all(msg.as_bytes()).await.is_err() {
                info!("Client write error; disconnecting");
                break;
            }
        }
    });

    tokio::select! {
        _ = client_task => info!("Client read task finished"),
        _ = writer_task => info!("Client write task finished"),
    }

    let mut clients = clients.lock().await;
    clients.remove(&client_id);
}

