mod client;
mod http;
mod routes;
mod server;
use std::{env, fs, io::Result, sync::{Arc, Mutex}, thread};

fn main() -> Result<()> {
    let port = if let Some(port) = env::args().nth(1) {
        port
    } else {
        println!("Usage: ./app <port> [peers.json]");
        return Ok(());
    };

    let peers: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    if let Some(path) = env::args().nth(2) {
        let json = fs::read_to_string(path).unwrap();
        let peer_info: Vec<String> = serde_json::from_str(&json).expect("failed to parse json");
        let mut guard = peers.lock().unwrap();
        *guard = peer_info;

        println!("Got peers from file:");
        for peer in guard.iter() {
            println!("{peer}")
        }
    }

    let client_state = peers.clone();

    let state = peers.clone();

    thread::spawn(move || {
        client::start(client_state).unwrap();
    });

    server::start(port, state)?;
    Ok(())
}
