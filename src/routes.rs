use std::{io::{Error, ErrorKind, Result}, net::TcpStream, sync::{Arc, Mutex}};

use serde_json::Value;

use crate::{http::reply, state::NodeState};

pub fn handle_ping(stream: TcpStream) -> Result<()> {
    reply(stream, 200, r#"{"status": "ok"}"#.to_string())
}

pub fn handle_addr(stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let peers = state.lock().unwrap().clone().peers;
    let peers_json = serde_json::to_string(&peers)?;
    reply(stream, 200, peers_json)
}

pub fn handle_announce(stream: TcpStream, state: Arc<Mutex<NodeState>>, peer_json: String) -> Result<()> {
    let json: Value = serde_json::from_str(&peer_json)?;
    let peer = json["address"].as_str().ok_or(Error::new(ErrorKind::InvalidData, "missing address"))?;
    state.lock().unwrap().peers.push(peer.to_string());

    let peers = state.lock().unwrap().clone().peers;
    let peers_json = serde_json::to_string(&peers)?;
    reply(stream, 200,peers_json)
}

pub fn handle_not_found(stream: TcpStream) -> Result<()> {
    reply(stream, 404, "".to_string())
}