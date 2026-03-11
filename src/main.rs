mod client;
mod http;
mod routes;
mod server;
use std::{env, io::Result};

fn main() -> Result<()> {
    let mode = env::args().nth(1).unwrap_or("".to_string());
    println!("Started application");

    let mut peers: Vec<String> = Vec::new();
    peers.push(String::from("127.0.0.1:8080"));

    if mode == "c" {
        let msg = env::args().nth(2);
        client::start(msg)?
    } else {
        let port = env::args().nth(2).unwrap_or("8080".to_string());
        server::start(port)?
    }
    Ok(())
}
