mod client;
mod http;
mod routes;
mod server;
mod state;
mod crypto;

use std::{env, fs, io::Result, sync::{Arc, Mutex}, thread};
use crate::state::NodeState;

fn main() -> Result<()> {
    let port = env::args().nth(1).unwrap_or_else(|| {
        println!("Usage: ./app <port> [peers.json] [bind-ip]");
        println!("Optional 3rd arg: your LAN IP, e.g. 192.168.1.42");
        std::process::exit(1);
    });
    let ip = env::args().nth(3).unwrap_or_else(|| "127.0.0.1".to_string());

    let bind_addr = format!("0.0.0.0:{port}");
    let announce_addr = format!("{ip}:{port}");
    let state: Arc<Mutex<NodeState>> = Arc::new(Mutex::new(NodeState::new(announce_addr, bind_addr)));

    if let Some(path) = env::args().nth(2) {
        let json = fs::read_to_string(path).unwrap();
        let peer_info: Vec<String> = serde_json::from_str(&json).expect("failed to parse json");
        let mut state = state.lock().unwrap();
        state.peers = peer_info;
        for peer in state.peers.iter() {
            println!("  {peer}");
        }
    }

    let client_state = state.clone();
    thread::spawn(move || {
        client::start(client_state).unwrap();
    });

    let server_state = state.clone();
    server::start(server_state)?;
    Ok(())
}