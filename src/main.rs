mod client;
mod http;
mod routes;
mod server;
mod state;

use std::{env, fs, io::Result, sync::{Arc, Mutex}, thread};
use crate::state::NodeState;

fn main() -> Result<()> {
    let port = if let Some(port) = env::args().nth(1) {
        port
    } else {
        println!("Usage: ./app <port> [peers.json]");
        return Ok(());
    };

    let state: Arc<Mutex<NodeState>> = Arc::new(Mutex::new(NodeState::new()));

    if let Some(path) = env::args().nth(2) {
        let json = fs::read_to_string(path).unwrap();
        let peer_info: Vec<String> = serde_json::from_str(&json).expect("failed to parse json");
        let mut guard = state.lock().unwrap();
        guard.peers = peer_info;
        println!("Loaded peers from file:");
        for peer in guard.peers.iter() {
            println!("  {peer}");
        }
    }

    let my_addr = format!("127.0.0.1:{port}");

    let client_state = state.clone();
    thread::spawn(move || {
        client::start(client_state, &my_addr).unwrap();
    });

    server::start(port, state)?;
    Ok(())
}