use std::{
    io::{Error, ErrorKind, Result, Write},
    net::TcpStream,
    sync::{Arc, Mutex},
    thread,
};

use serde_json::Value;

use crate::{
    client::{forward_block, forward_inv, post_commit},
    consensus::{apply_commit, build_proposal, validate_commit_for_state},
    crypto::calculate_hash,
    http::{Request, reply},
    ledger::{IngestResult, create_local_transaction, ingest_transaction},
    models::{Commit, Transaction},
    state::NodeState,
    storage::{persist_commit, write_ledger},
};

pub fn handle_ping(stream: TcpStream, request: Request) -> Result<()> {
    let addr = request.node_addr().unwrap_or("");
    println!("ping: {addr}");
    reply(stream, 200, "".to_string())
}

pub fn handle_addr(stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let peers = state.lock().unwrap().clone().peers;
    let peers_json = serde_json::to_string(&peers).map_err(|e| Error::new(ErrorKind::Other, e))?;
    reply(stream, 200, peers_json)
}

pub fn handle_announce(
    stream: TcpStream,
    state: Arc<Mutex<NodeState>>,
    peer_json: String,
) -> Result<()> {
    println!("anno: {}", stream.peer_addr().unwrap());
    let json: Value = serde_json::from_str(&peer_json)?;
    let peer = json["address"]
        .as_str()
        .ok_or(Error::new(ErrorKind::InvalidData, "missing address"))?;

    let peers_json = {
        let mut node = state.lock().unwrap();
        if peer != node.addr && !node.peers.contains(&peer.to_string()) {
            node.peers.push(peer.to_string());
        }
        serde_json::to_string(&node.peers).map_err(|e| Error::new(ErrorKind::Other, e))?
    };
    reply(stream, 200, peers_json)
}

pub fn handle_not_found(stream: TcpStream) -> Result<()> {
    reply(stream, 404, "".to_string())
}

pub fn handle_get_blocks(stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let blocks: Vec<String> = state.lock().unwrap().blocks.keys().cloned().collect();
    let body = serde_json::to_string(&blocks).map_err(|e| Error::new(ErrorKind::Other, e))?;
    reply(stream, 200, body)
}

pub fn handle_get_data(stream: TcpStream, state: Arc<Mutex<NodeState>>, hash: &str) -> Result<()> {
    let blocks = state.lock().unwrap().blocks.clone();
    match blocks.get(hash) {
        Some(content) => {
            let body = serde_json::json!({
                "hash": hash,
                "content": content,
            })
            .to_string();
            reply(stream, 200, body)
        }
        None => reply(stream, 404, r#"{"error": "block not found"}"#.to_string()),
    }
}

pub fn handle_post_block(
    stream: TcpStream,
    state: Arc<Mutex<NodeState>>,
    body: String,
) -> Result<()> {
    let json: Value = serde_json::from_str(&body).map_err(|e| Error::new(ErrorKind::Other, e))?;
    let hash = json["hash"]
        .as_str()
        .ok_or(Error::new(ErrorKind::InvalidData, "missing hash"))?;
    let content = json["content"]
        .as_str()
        .ok_or(Error::new(ErrorKind::InvalidData, "missing content"))?;

    let already_have = state.lock().unwrap().blocks.contains_key(hash);
    if already_have {
        return reply(stream, 200, r#"{"status": "already have it"}"#.to_string());
    }

    let calculated_hash = calculate_hash(content);
    if calculated_hash != hash {
        return reply(stream, 400, r#"{"error": "invalid hash"}"#.to_string());
    }

    state
        .lock()
        .unwrap()
        .blocks
        .insert(hash.to_string(), content.to_string());
    println!("Stored block {hash}");

    reply(stream, 200, r#"{"status": "ok"}"#.to_string())?;

    let peers = state.lock().unwrap().peers.clone();
    let state = state.clone();
    thread::spawn(move || {
        for peer in peers {
            let _ = forward_block(&peer, &body, &state);
        }
    });

    Ok(())
}

pub fn handle_get_blocks_from(
    stream: TcpStream,
    state: Arc<Mutex<NodeState>>,
    from_hash: &str,
) -> Result<()> {
    let blocks = state.lock().unwrap().blocks.clone();
    let keys: Vec<String> = blocks
        .keys()
        .skip_while(|k| k.as_str() != from_hash)
        .cloned()
        .collect();
    let body = serde_json::to_string(&keys).map_err(|e| Error::new(ErrorKind::Other, e))?;
    reply(stream, 200, body)
}

pub fn handle_post_tx(stream: TcpStream, state: Arc<Mutex<NodeState>>, body: String) -> Result<()> {
    let json: Value = serde_json::from_str(&body).map_err(|e| Error::new(ErrorKind::Other, e))?;
    let tx_body = json["body"]
        .as_str()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "missing body"))?
        .to_string();

    let (tx, should_forward, peers, forward_body) = {
        let mut node = state.lock().unwrap();
        let tx = create_local_transaction(&mut node, tx_body)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        ingest_transaction(&mut node, tx.clone())
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        if !node.consensus_enabled {
            write_ledger(&node)?;
        }

        let should_forward = node.forward_inv_enabled;
        let peers = node.peers.clone();
        let forward_body =
            serde_json::to_string(&tx).map_err(|e| Error::new(ErrorKind::Other, e))?;
        (tx, should_forward, peers, forward_body)
    };

    let response = serde_json::json!({
        "status": "ok",
        "tx": tx,
    })
    .to_string();
    reply(stream, 200, response)?;

    if should_forward {
        let state = state.clone();
        thread::spawn(move || {
            for peer in peers {
                if let Err(e) = forward_inv(&peer, &forward_body, &state) {
                    println!("failed to forward transaction to {peer}: {e}");
                }
            }
        });
    }

    Ok(())
}

pub fn handle_post_inv(
    stream: TcpStream,
    state: Arc<Mutex<NodeState>>,
    body: String,
) -> Result<()> {
    let json: Value = serde_json::from_str(&body).map_err(|e| Error::new(ErrorKind::Other, e))?;
    let tx: Transaction = match serde_json::from_value(json) {
        Ok(tx) => tx,
        Err(e) => {
            return reply(
                stream,
                400,
                serde_json::json!({
                    "error": format!("invalid transaction format: {e}")
                })
                .to_string(),
            );
        }
    };

    let (status, should_forward, peers) = {
        let mut node = state.lock().unwrap();
        let result = match ingest_transaction(&mut node, tx) {
            Ok(result) => result,
            Err(e) => {
                return reply(
                    stream,
                    400,
                    serde_json::json!({ "error": format!("invalid transaction: {e}") }).to_string(),
                );
            }
        };

        if !node.consensus_enabled && result == IngestResult::Accepted {
            write_ledger(&node)?;
        }

        let status = match result {
            IngestResult::Accepted => "ok",
            IngestResult::AlreadyKnown => "already have it",
        };

        (
            status.to_string(),
            node.forward_inv_enabled && result == IngestResult::Accepted,
            node.peers.clone(),
        )
    };

    reply(
        stream,
        200,
        serde_json::json!({ "status": status }).to_string(),
    )?;

    if should_forward {
        let state = state.clone();
        thread::spawn(move || {
            for peer in peers {
                if let Err(e) = forward_inv(&peer, &body, &state) {
                    println!("failed to forward transaction to {peer}: {e}");
                }
            }
        });
    }

    Ok(())
}

pub fn handle_get_ledger(stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let ledger = state.lock().unwrap().ledger.clone();
    let body = serde_json::to_string(&ledger).map_err(|e| Error::new(ErrorKind::Other, e))?;
    reply(stream, 200, body)
}

pub fn handle_ledger_status(stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let node = state.lock().unwrap();
    let body = ledger_status_json(&node).to_string();
    reply(stream, 200, body)
}

pub fn handle_get_consensus_proposal(
    stream: TcpStream,
    state: Arc<Mutex<NodeState>>,
    round: u64,
) -> Result<()> {
    let proposal = {
        let node = state.lock().unwrap();
        build_proposal(&node, round)
    };
    let body = serde_json::to_string(&proposal).map_err(|e| Error::new(ErrorKind::Other, e))?;
    reply(stream, 200, body)
}

pub fn handle_post_consensus_commit(
    stream: TcpStream,
    state: Arc<Mutex<NodeState>>,
    body: String,
) -> Result<()> {
    let commit: Commit = match serde_json::from_str(&body) {
        Ok(commit) => commit,
        Err(e) => {
            return reply(
                stream,
                400,
                serde_json::json!({ "error": format!("invalid commit format: {e}") }).to_string(),
            );
        }
    };

    let mut peers = Vec::new();
    let mut accepted = false;
    let response = {
        let mut node = state.lock().unwrap();
        match validate_commit_for_state(&node, &commit) {
            Ok(()) if commit.payload.round < node.next_round => {
                serde_json::json!({ "status": "already_committed" })
            }
            Ok(()) => {
                apply_commit(&mut node, commit.clone())
                    .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
                persist_commit(&node, &commit)?;
                peers = node.peers.clone();
                accepted = true;
                serde_json::json!({ "status": "ok" })
            }
            Err(e) if is_conflict_error(&e) => {
                let body = serde_json::json!({
                    "error": e,
                    "ledger_hash": node.ledger_hash,
                    "next_round": node.next_round,
                });
                return reply(stream, 409, body.to_string());
            }
            Err(e) => {
                return reply(
                    stream,
                    400,
                    serde_json::json!({ "error": format!("bad commit: {e}") }).to_string(),
                );
            }
        }
    };

    reply(stream, 200, response.to_string())?;

    if accepted {
        let state = state.clone();
        thread::spawn(move || {
            for peer in peers {
                if let Err(e) = post_commit(&peer, &commit, &state) {
                    println!("failed to forward commit to {peer}: {e}");
                }
            }
        });
    }

    Ok(())
}

pub fn handle_get_consensus_commits(
    stream: TcpStream,
    state: Arc<Mutex<NodeState>>,
    from_round: u64,
) -> Result<()> {
    let commits: Vec<Commit> = {
        let node = state.lock().unwrap();
        node.commits
            .iter()
            .filter(|commit| commit.payload.round >= from_round)
            .cloned()
            .collect()
    };
    let body = serde_json::to_string(&commits).map_err(|e| Error::new(ErrorKind::Other, e))?;
    reply(stream, 200, body)
}

pub fn handle_debug_faults(
    stream: TcpStream,
    state: Arc<Mutex<NodeState>>,
    body: String,
) -> Result<()> {
    let json: Value = serde_json::from_str(&body).map_err(|e| Error::new(ErrorKind::Other, e))?;

    let status = {
        let mut node = state.lock().unwrap();

        if let Some(enabled) = json
            .get("forward_inv_enabled")
            .and_then(|value| value.as_bool())
        {
            node.forward_inv_enabled = enabled;
        }

        if let Some(peer) = json.get("block_peer").and_then(|value| value.as_str()) {
            if peer != node.addr {
                node.blocked_peers.insert(peer.to_string());
            }
        }

        if let Some(peer) = json.get("unblock_peer").and_then(|value| value.as_str()) {
            node.blocked_peers.remove(peer);
        }

        serde_json::json!({
            "status": "ok",
            "forward_inv_enabled": node.forward_inv_enabled,
            "blocked_peers": sorted_blocked_peers(&node),
        })
    };

    reply(stream, 200, status.to_string())
}

pub fn handle_status(stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let node = state.lock().unwrap();
    let body = ledger_status_json(&node).to_string();
    reply(stream, 200, body)
}

fn sorted_blocked_peers(node: &NodeState) -> Vec<String> {
    let mut peers: Vec<String> = node.blocked_peers.iter().cloned().collect();
    peers.sort();
    peers
}

fn is_conflict_error(error: &str) -> bool {
    error.contains("ledger hash mismatch")
        || error.contains("does not match next round")
        || error.contains("conflicts with already committed")
}

fn ledger_status_json(node: &NodeState) -> Value {
    serde_json::json!({
        "addr": node.addr,
        "peers": node.peers,
        "block_count": node.blocks.len(),
        "ledger_len": node.ledger.len(),
        "ledger_hash": node.ledger_hash,
        "next_round": node.next_round,
        "mempool_count": node.tx_pool.len(),
        "commit_count": node.commits.len(),
        "consensus_enabled": node.consensus_enabled,
        "forward_inv_enabled": node.forward_inv_enabled,
        "round_secs": node.round_secs,
        "data_dir": node.data_dir,
        "blocked_peer_count": node.blocked_peers.len(),
    })
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
