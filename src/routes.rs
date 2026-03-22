use std::{io::{Error, ErrorKind, Result}, net::TcpStream, sync::{Arc, Mutex}};

use serde_json::Value;

use crate::http::reply;

pub fn handle_ping(stream: TcpStream) -> Result<()> {
    reply(stream, r#"{"status": "ok"}"#.to_string())
}

pub fn handle_addr(stream: TcpStream, state: Arc<Mutex<Vec<String>>>) -> Result<()> {
    let peers = state.lock().unwrap().clone();
    let peers_json = serde_json::to_string(&peers)?;
    reply(stream, peers_json)
}

pub fn handle_announce(stream: TcpStream, state: Arc<Mutex<Vec<String>>>, peer_json: String) -> Result<()> {
    let json: Value = serde_json::from_str(&peer_json)?;
    let peer = json["address"].as_str().ok_or(Error::new(ErrorKind::InvalidData, "missing address"))?;
    state.lock().unwrap().push(peer.to_string());

    let peers = state.lock().unwrap().clone();
    let peers_json = serde_json::to_string(&peers)?;
    reply(stream, peers_json)
}

pub fn handle_not_found(stream: TcpStream) -> Result<()> {
    reply(stream, "".to_string())
}