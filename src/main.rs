mod client;
mod http;
mod routes;
mod server;
mod state;
mod crypto;

use std::{env, fs, io::Result, sync::{Arc, Mutex}, thread};
use crate::state::NodeState;

fn main() -> Result<()> {
    let port = if let Some(port) = env::args().nth(1) {
        port
    } else {
        println!("Usage: ./app <port> [peers.json]");
        return Ok(());
    };

    let addr = format!("127.0.0.1:{port}");
    let state: Arc<Mutex<NodeState>> = Arc::new(Mutex::new(NodeState::new(addr)));

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