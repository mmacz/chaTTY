use std::net::{TcpListener, Ipv4Addr};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(author = "mmacz", version="1.0.0", about = None, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = { Ipv4Addr::new(127, 0, 0, 1) } )]
    ip: Ipv4Addr,

    #[arg(short, long, default_value_t = 8080)]
    port: u16,

    #[arg(short, long, default_value_t = ("chaTTY".to_string()))]
    name: String
}

fn main() {
    let args = Args::parse();
    println!("Hello server: {:?}", args);
}

