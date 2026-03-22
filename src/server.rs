use crate::http::{parse_request};
use crate::routes::{handle_addr, handle_not_found, handle_ping, handle_announce};
use crate::state::NodeState;
use std::sync::{Arc, Mutex};
use std::thread;
use std::{
    io::{Read, Result},
    net::{TcpListener, TcpStream},
};

fn handle_client(mut stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf)?;

    println!(
        "Received from {}:",
        stream.peer_addr().unwrap()
    );

    let request = parse_request(&buf[..n]);

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/ping") => handle_ping(stream),
        ("GET", "/addr") => handle_addr(stream, state),
        ("POST", "/peers/announce") => handle_announce(stream, state, request.body),
        _ => handle_not_found(stream),
    }
}

pub fn start(state: Arc<Mutex<NodeState>>) -> Result<()> {
    let addr = state.lock().unwrap().addr.clone();
    let listener = TcpListener::bind(addr)?;

    for stream in listener.incoming() {
        let node = state.clone();
        thread::spawn(move || {
            if let Err(e) = handle_client(stream.unwrap(), node) {
                println!("Error handling client: {}", e);
            }
        });
    }
    
    Ok(())
}
