use log::{Level, LevelFilter, Metadata, Record, SetLoggerError};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};
use std::sync::Arc;
use std::collections::HashMap;
use std::env;

mod client_handler;
use client_handler::{ClientList, handle_client};

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    static LOGGER: SimpleLogger = SimpleLogger;
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(LevelFilter::Info))
        .expect("Failed to initialize logger!");

    let port: u16 = match env::var("CHATTY_PORT") {
        Ok(p) => p.parse().unwrap(),
        _ => 8080,
    };

    log::info!("Starting server on port: {}", port);

    match TcpListener::bind(format!("127.0.0.1:{}", port)).await {
        Ok(listener) => {
            log::info!("Server started!");
            let (broadcast_tx, _) = broadcast::channel::<String>(1);
            let clients: ClientList = Arc::new(Mutex::new(HashMap::new()));

            loop {
                let (socket, addr) = listener.accept().await?;
                log::info!("Client connected: {}", addr);

                let tx = broadcast_tx.clone();
                let clients = Arc::clone(&clients);

                tokio::spawn(async move {
                    handle_client(socket, tx, clients).await;
                });
            }
        }
        Err(e) => {
            log::error!("Could not start a chaTTY server! {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}
