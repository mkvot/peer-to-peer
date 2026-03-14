mod client;
mod http;
mod routes;
mod server;
use std::{env, io::Result, sync::{Arc, Mutex}, thread};

fn main() -> Result<()> {
    println!("Started application");

    let peers: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    peers.lock().unwrap().push(String::from("127.0.0.1:8080"));
    let client_peers = peers.clone();
    let server_peers = peers.clone();

    let msg = env::args().nth(2);
    let port = env::args().nth(2).unwrap_or("8080".to_string());

    thread::spawn(move || {
        client::start(msg, client_peers).unwrap();
    });

    server::start(port, server_peers)?;
    Ok(())
}
