use std::{io::Result, net::TcpStream, sync::{Arc, Mutex}};

use crate::http::reply;

pub fn handle_ping(stream: TcpStream) -> Result<()> {
    reply(stream, r#"{"status": "ok"}"#.to_string())
}

pub fn handle_addr(stream: TcpStream, state: Arc<Mutex<Vec<String>>>) -> Result<()> {
    let peers = state.lock().unwrap().clone();
    let peers_json = serde_json::to_string(&peers)?;
    reply(stream, peers_json)
}

pub fn handle_announce(stream: TcpStream, state: Arc<Mutex<Vec<String>>>, peer: String) -> Result<()> {
    let mut peers = state.lock().unwrap().clone();
    let peers_json = serde_json::to_string(&peers)?;
    reply(stream, peers_json);
    peers.push(peer);
    Ok(())
}

pub fn handle_not_found(stream: TcpStream) -> Result<()> {
    reply(stream, "".to_string())
}