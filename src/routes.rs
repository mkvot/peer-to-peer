use std::{io::{Error, ErrorKind, Result}, net::TcpStream, sync::{Arc, Mutex}, thread};

use serde_json::Value;

use crate::{client::{forward_block, forward_inv}, crypto::calculate_hash, http::{Request, reply}, state::NodeState};

pub fn handle_ping(stream: TcpStream, request: Request) -> Result<()> {
    let addr = request.node_addr().unwrap_or("");
    println!("ping: {addr}");
    reply(stream, 200, "".to_string())
}

pub fn handle_addr(stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let peers = state.lock().unwrap().clone().peers;
    let peers_json = serde_json::to_string(&peers)
    .map_err(|e| Error::new(ErrorKind::Other, e))?;
    reply(stream, 200, peers_json)
}

pub fn handle_announce(stream: TcpStream, state: Arc<Mutex<NodeState>>, peer_json: String) -> Result<()> {
    println!("anno: {}", stream.peer_addr().unwrap());
    let json: Value = serde_json::from_str(&peer_json)?;
    let peer = json["address"].as_str()
        .ok_or(Error::new(ErrorKind::InvalidData, "missing address"))?;

    let peers_json = {
        let mut node = state.lock().unwrap();
        if peer != node.addr && !node.peers.contains(&peer.to_string()) {
            node.peers.push(peer.to_string());
        }
        serde_json::to_string(&node.peers)
            .map_err(|e| Error::new(ErrorKind::Other, e))?
    };
    reply(stream, 200, peers_json)
}

pub fn handle_not_found(stream: TcpStream) -> Result<()> {
    reply(stream, 404, "".to_string())
}

pub fn handle_get_blocks(stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let blocks: Vec<String> = state.lock().unwrap().blocks.keys().cloned().collect();
    let body = serde_json::to_string(&blocks)
        .map_err(|e| Error::new(ErrorKind::Other, e))?;
    reply(stream, 200, body)
}

pub fn handle_get_data(stream: TcpStream, state: Arc<Mutex<NodeState>>, hash: &str) -> Result<()> {
    let blocks = state.lock().unwrap().blocks.clone();
    match blocks.get(hash) {
        Some(content) => {
            let body = format!(r#"{{"hash": "{hash}", "content": {content}}}"#);
            reply(stream, 200, body)
        },
        None => reply(stream, 404, r#"{"error": "block not found"}"#.to_string()),
    }
}

pub fn handle_post_block(stream: TcpStream, state: Arc<Mutex<NodeState>>, body: String) -> Result<()> {
    let json: Value = serde_json::from_str(&body)
        .map_err(|e| Error::new(ErrorKind::Other, e))?;
    let hash = json["hash"].as_str()
        .ok_or(Error::new(ErrorKind::InvalidData, "missing hash"))?;
    let content = json["content"].as_str()
        .ok_or(Error::new(ErrorKind::InvalidData, "missing content"))?;

    let already_have = state.lock().unwrap().blocks.contains_key(hash);
    if already_have {
        return reply(stream, 200, r#"{"status": "already have it"}"#.to_string());
    }
    
    let calculated_hash = calculate_hash(content);
    if calculated_hash != hash {
        return reply(stream, 400, r#"{"error": "invalid hash"}"#.to_string());
    }

    state.lock().unwrap().blocks.insert(hash.to_string(), content.to_string());
    println!("Stored block {hash}");
    
    let peers = state.lock().unwrap().peers.clone();
    thread::spawn(move || {
        for peer in peers {
            let _ = forward_block(&peer, &body, &state);
        }
    });

    Ok(())
}

pub fn handle_get_blocks_from(stream: TcpStream, state: Arc<Mutex<NodeState>>, from_hash: &str) -> Result<()> {
    let blocks = state.lock().unwrap().blocks.clone();
    let keys: Vec<String> = blocks.keys()
        .skip_while(|k| k.as_str() != from_hash)
        .cloned()
        .collect();
    let body = serde_json::to_string(&keys)
        .map_err(|e| Error::new(ErrorKind::Other, e))?;
    reply(stream, 200, body)
}

pub fn handle_post_inv(stream: TcpStream, state: Arc<Mutex<NodeState>>, body: String) -> Result<()> {
    let json: Value = serde_json::from_str(&body)
        .map_err(|e| Error::new(ErrorKind::Other, e))?;
    let hash = json["hash"].as_str()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "missing hash"))?;
    let content = json["content"].as_str()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "missing content"))?;

    let already_have = state.lock().unwrap().transactions.contains_key(hash);
    if already_have {
        return reply(stream, 200, r#"{"status": "already have it"}"#.to_string());
    }

    let calculated_hash = calculate_hash(content);
    if calculated_hash != hash {
        return reply(stream, 400, r#"{"error": "invalid hash"}"#.to_string());
    }

    state.lock().unwrap().transactions.insert(hash.to_string(), content.to_string());
    println!("stored transaction {hash}");

    reply(stream, 200, r#"{"status": "ok"}"#.to_string())?;

    let peers = state.lock().unwrap().peers.clone();
    for peer in peers.iter() {
        if let Err(e) = forward_inv(peer, &body, &state) {
            println!("failed to forward transaction to {peer}: {e}");
        }
    }

    Ok(())
}

pub fn handle_status(stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let node = state.lock().unwrap();
    let body = serde_json::json!({
        "addr": node.addr,
        "peers": node.peers,
        "block_count": node.blocks.len(),
        "transaction_count": node.transactions.len(),
    }).to_string();
    reply(stream, 200, body)
}

pub fn handle_options(mut stream: TcpStream) -> Result<()> {
    let response = "HTTP/1.1 204 No Content\r\n\
        Access-Control-Allow-Origin: *\r\n\
        Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
        Access-Control-Allow-Headers: Content-Type, X-Node-Addr\r\n\
        Content-Length: 0\r\n\
        \r\n";
    stream.write_all(response.as_bytes())
}