mod client;
mod http;
mod routes;
mod server;
use std::{env, fs, io::Result, sync::{Arc, Mutex}, thread};

fn main() -> Result<()> {
    let peers: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    if let Some(path) = env::args().nth(1) {
        let json = fs::read_to_string(path).unwrap();
        let peer_info: Vec<String> = serde_json::from_str(&json).expect("failed to parse json");
        let mut guard = peers.lock().unwrap();
        *guard = peer_info;

        println!("Got peers from file:");
        for peer in guard.iter() {
            println!("{peer}")
        }
    }

    let client_peers = peers.clone();
    let server_peers = peers.clone();

    // let msg = env::args().nth(2);
    let msg = Some("peers".to_string());
    let port = env::args().nth(3).unwrap_or("8080".to_string());

    thread::spawn(move || {
        client::start(msg, client_peers).unwrap();
    });

    server::start(port, server_peers)?;
    Ok(())
}
