use crate::http::{parse_request};
use crate::routes::{handle_addr, handle_announce, handle_get_blocks, handle_get_blocks_from, handle_get_data, handle_not_found, handle_options, handle_ping, handle_post_block, handle_post_inv, handle_status};
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

    let request = parse_request(&buf[..n]);

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/ping") => handle_ping(stream, request),
        ("GET", "/addr") => handle_addr(stream, state),
        ("POST", "/peers/announce") => handle_announce(stream, state, request.body),
        ("GET", "/getblocks") => handle_get_blocks(stream, state),
        ("GET", path) if path.starts_with("/getdata/") => {
            let hash = path.trim_start_matches("/getdata/");
            handle_get_data(stream, state, hash)
        },
        ("POST", "/block") => handle_post_block(stream, state, request.body),
        ("GET", path) if path.starts_with("/getblocks/") => {
            let hash = path.trim_start_matches("/getblocks/");
            handle_get_blocks_from(stream, state, hash)
        },
        ("POST", "/inv") => handle_post_inv(stream, state, request.body),
        ("GET", "/status") => handle_status(stream, state),
        ("OPTIONS", _) => handle_options(stream),
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
