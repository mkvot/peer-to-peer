use crate::http::parse_request;
use crate::routes::{handle_addr, handle_not_found, handle_ping};
use std::sync::{Arc, Mutex};
use std::{
    io::{Read, Result},
    net::{TcpListener, TcpStream},
};

fn handle_client(mut stream: TcpStream, state: Arc<Mutex<Vec<String>>>) -> Result<()> {
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf)?;

    // println!(
    //     "Received {} bytes from {}:",
    //     n,
    //     stream.local_addr().unwrap()
    // );
    // println!("{}", String::from_utf8_lossy(&buf[..n]));

    let request = parse_request(&buf[..n]);

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/ping") => handle_ping(stream),
        ("GET", "/addr") => handle_addr(stream, state),
        ("POST", "/peers/announce") => handle_addr(stream, state),
        _ => handle_not_found(stream),
    }
}

pub fn start(port: String, peers: Arc<Mutex<Vec<String>>>) -> Result<()> {
    let ip = "127.0.0.1";
    let addr = format!("{ip}:{port}");
    let listener = TcpListener::bind(addr)?;

    for stream in listener.incoming() {
        let peers_ref = peers.clone();
        handle_client(stream?, peers_ref)?;
    }
    Ok(())
}
