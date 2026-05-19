use std::{
    io::{Error, ErrorKind, Result, Write},
    net::TcpStream,
    sync::{Arc, Mutex},
};

use serde::Deserialize;
use serde_json::Value;

use crate::{
    chain::{self, BlockAddStatus},
    client,
    crypto::short_hash,
    http::{Request, reply},
    mining,
    models::{Block, SignedTransaction},
    state::NodeState,
    storage, transaction,
};

#[derive(Deserialize)]
struct CreateTransactionRequest {
    to: String,
    amount: u64,
    memo: Option<String>,
}

#[derive(Deserialize)]
struct MineRequest {
    blocks: Option<u64>,
    max_txs: Option<usize>,
}

pub fn handle_ping(stream: TcpStream) -> Result<()> {
    reply(stream, 200, "{}".to_string())
}

pub fn handle_status(stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let body = state.lock().unwrap().status_json().to_string();
    reply(stream, 200, body)
}

pub fn handle_get_peers(stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let peers = state.lock().unwrap().peers.clone();
    reply_json(stream, &peers)
}

pub fn handle_post_peers(
    stream: TcpStream,
    state: Arc<Mutex<NodeState>>,
    request: Request,
) -> Result<()> {
    let peers = match parse_peer_request(&request) {
        Ok(peers) => peers,
        Err(error) => return bad_request(stream, error),
    };
    {
        let mut node = state.lock().unwrap();
        let mut changed = false;
        for peer in peers {
            if peer != node.addr && !node.peers.contains(&peer) {
                node.record_event("peer", format!("discovered {peer}"));
                node.peers.push(peer);
                changed = true;
            }
        }
        if changed {
            node.peers.sort();
            node.peers.dedup();
            storage::persist_peers(&node)?;
        }
    }
    let body = state.lock().unwrap().peers.clone();
    reply_json(stream, &body)
}

pub fn handle_wallet(stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let public_key = state.lock().unwrap().wallet.public_key.clone();
    reply(
        stream,
        200,
        serde_json::json!({ "public_key": public_key }).to_string(),
    )
}

pub fn handle_create_transaction(
    stream: TcpStream,
    state: Arc<Mutex<NodeState>>,
    body: String,
) -> Result<()> {
    let request: CreateTransactionRequest = match serde_json::from_str(&body) {
        Ok(request) => request,
        Err(error) => return bad_request(stream, format!("invalid transaction request: {error}")),
    };

    let tx = {
        let mut node = state.lock().unwrap();
        let tx = match transaction::create_signed_transaction(
            &node.wallet,
            request.to,
            request.amount,
            request.memo.unwrap_or_default(),
        ) {
            Ok(tx) => tx,
            Err(error) => return bad_request(stream, error),
        };
        match chain::add_transaction_to_mempool(&mut node.chain, tx.clone()) {
            Ok(true) => {
                storage::persist_mempool(&node)?;
                node.record_event("tx", tx_event_message("accepted", &tx));
            }
            Ok(false) => {}
            Err(error) => return bad_request(stream, error),
        }
        tx
    };

    reply_json(stream, &tx)?;
    client::broadcast_transaction(tx, state);
    Ok(())
}

pub fn handle_post_transaction(
    stream: TcpStream,
    state: Arc<Mutex<NodeState>>,
    body: String,
) -> Result<()> {
    let tx: SignedTransaction = match serde_json::from_str(&body) {
        Ok(tx) => tx,
        Err(error) => return bad_request(stream, format!("invalid transaction: {error}")),
    };

    let accepted = {
        let mut node = state.lock().unwrap();
        match chain::add_transaction_to_mempool(&mut node.chain, tx.clone()) {
            Ok(true) => {
                storage::persist_mempool(&node)?;
                node.record_event("tx", tx_event_message("accepted", &tx));
                true
            }
            Ok(false) => false,
            Err(error) => {
                node.record_event("tx", format!("rejected reason=\"{error}\""));
                return bad_request(stream, error);
            }
        }
    };

    let status = if accepted {
        "accepted"
    } else {
        "already_known"
    };
    reply(
        stream,
        200,
        serde_json::json!({ "status": status }).to_string(),
    )?;
    if accepted {
        client::broadcast_transaction(tx, state);
    }
    Ok(())
}

pub fn handle_get_transactions(stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let txs: Vec<SignedTransaction> = state
        .lock()
        .unwrap()
        .chain
        .mempool
        .values()
        .cloned()
        .collect();
    reply_json(stream, &txs)
}

pub fn handle_mine(stream: TcpStream, state: Arc<Mutex<NodeState>>, body: String) -> Result<()> {
    let request = if body.trim().is_empty() {
        MineRequest {
            blocks: Some(1),
            max_txs: Some(50),
        }
    } else {
        match serde_json::from_str::<MineRequest>(&body) {
            Ok(request) => request,
            Err(error) => return bad_request(stream, format!("invalid mine request: {error}")),
        }
    };
    let blocks = request.blocks.unwrap_or(1).max(1);
    let max_txs = request.max_txs.unwrap_or(50);

    let mined = match mining::mine_blocks(state.clone(), blocks, max_txs) {
        Ok(blocks) => blocks,
        Err(error) => return bad_request(stream, error),
    };
    let hashes: Vec<String> = mined.iter().map(|block| block.hash.clone()).collect();

    reply(
        stream,
        200,
        serde_json::json!({
            "status": "mined",
            "count": mined.len(),
            "blocks": hashes,
        })
        .to_string(),
    )?;

    for block in mined {
        client::broadcast_block(block, state.clone());
    }
    Ok(())
}

pub fn handle_mining_status(stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let status = state.lock().unwrap().chain.mining.clone();
    reply_json(stream, &status)
}

pub fn handle_post_block(
    stream: TcpStream,
    state: Arc<Mutex<NodeState>>,
    body: String,
) -> Result<()> {
    let block: Block = match serde_json::from_str(&body) {
        Ok(block) => block,
        Err(error) => {
            return reply(
                stream,
                400,
                serde_json::json!({
                    "status": "rejected",
                    "error": format!("invalid block: {error}")
                })
                .to_string(),
            );
        }
    };

    let mut added_for_broadcast = Vec::new();
    let response = {
        let mut node = state.lock().unwrap();
        let difficulty = node.config.difficulty;
        match chain::add_block(&mut node.chain, block.clone(), difficulty) {
            Ok(outcome) => {
                if outcome.status == BlockAddStatus::Added {
                    client::handle_block_outcome_events(&mut node, &outcome);
                    for added in &outcome.added_blocks {
                        storage::persist_block(&node, added)?;
                    }
                    storage::persist_mempool(&node)?;
                    storage::persist_chain_outputs(&node)?;
                    added_for_broadcast = outcome.added_blocks.clone();
                } else if outcome.status == BlockAddStatus::Orphan {
                    node.record_event(
                        "block",
                        format!(
                            "orphan hash={} waiting_for={}",
                            short_hash(&block.hash),
                            short_hash(&block.header.previous_hash)
                        ),
                    );
                }
                serde_json::json!({ "status": outcome.status.as_str() })
            }
            Err(error) => {
                node.record_event("block", format!("rejected reason=\"{error}\""));
                return reply(
                    stream,
                    400,
                    serde_json::json!({ "status": "rejected", "error": error }).to_string(),
                );
            }
        }
    };

    reply(stream, 200, response.to_string())?;
    for block in added_for_broadcast {
        client::broadcast_block(block, state.clone());
    }
    Ok(())
}

pub fn handle_get_block(stream: TcpStream, state: Arc<Mutex<NodeState>>, hash: &str) -> Result<()> {
    let block = state
        .lock()
        .unwrap()
        .chain
        .blocks
        .get(hash)
        .map(|stored| stored.block.clone());
    match block {
        Some(block) => reply_json(stream, &block),
        None => reply(
            stream,
            404,
            serde_json::json!({ "error": "block not found" }).to_string(),
        ),
    }
}

pub fn handle_hashes(stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let hashes = chain::canonical_hashes(&state.lock().unwrap().chain);
    reply_json(stream, &hashes)
}

pub fn handle_chain(stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let blocks = chain::canonical_blocks(&state.lock().unwrap().chain);
    reply_json(stream, &blocks)
}

pub fn handle_chain_status(stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let status = chain::chain_status_json(&state.lock().unwrap().chain);
    reply(stream, 200, status.to_string())
}

pub fn handle_balances(stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let balances = chain::balances_for_best(&state.lock().unwrap().chain);
    reply_json(stream, &balances)
}

pub fn handle_events(stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let events: Vec<_> = state.lock().unwrap().events.iter().cloned().collect();
    reply_json(stream, &events)
}

pub fn handle_debug_faults(
    stream: TcpStream,
    state: Arc<Mutex<NodeState>>,
    body: String,
) -> Result<()> {
    let json: Value = match serde_json::from_str(&body) {
        Ok(value) => value,
        Err(error) => return bad_request(stream, format!("invalid debug request: {error}")),
    };

    let response = {
        let mut node = state.lock().unwrap();

        if json
            .get("clear_blocked_peers")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            node.chain.blocked_peers.clear();
            node.record_event("peer", "cleared blocked peers");
        }

        if let Some(peer) = json.get("block_peer").and_then(|value| value.as_str()) {
            if peer != node.addr {
                node.chain.blocked_peers.insert(peer.to_string());
                node.record_event("peer", format!("blocked {peer}"));
            }
        }

        if let Some(peer) = json.get("unblock_peer").and_then(|value| value.as_str()) {
            node.chain.blocked_peers.remove(peer);
            node.record_event("peer", format!("unblocked {peer}"));
        }

        let mut blocked: Vec<String> = node.chain.blocked_peers.iter().cloned().collect();
        blocked.sort();
        serde_json::json!({
            "status": "ok",
            "blocked_peers": blocked,
        })
    };

    reply(stream, 200, response.to_string())
}

pub fn handle_not_found(stream: TcpStream) -> Result<()> {
    reply(
        stream,
        404,
        serde_json::json!({ "error": "not found" }).to_string(),
    )
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

fn parse_peer_request(request: &Request) -> Result<Vec<String>> {
    if request.body.trim().is_empty() {
        return Ok(request
            .node_addr()
            .map(|addr| vec![addr.to_string()])
            .unwrap_or_default());
    }

    let json: Value =
        serde_json::from_str(&request.body).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
    if let Some(address) = json.get("address").and_then(|value| value.as_str()) {
        return Ok(vec![address.to_string()]);
    }
    if let Some(array) = json.as_array() {
        return Ok(array
            .iter()
            .filter_map(|value| value.as_str().map(ToString::to_string))
            .collect());
    }
    Err(Error::new(ErrorKind::InvalidData, "missing peer address"))
}

fn tx_event_message(action: &str, tx: &SignedTransaction) -> String {
    format!(
        "{action} id={} from={} to={} amount={}",
        short_hash(&tx.id),
        short_hash(&tx.payload.from),
        short_hash(&tx.payload.to),
        tx.payload.amount
    )
}

fn reply_json<T: serde::Serialize>(stream: TcpStream, value: &T) -> Result<()> {
    let body = serde_json::to_string(value).map_err(|e| Error::new(ErrorKind::Other, e))?;
    reply(stream, 200, body)
}

fn bad_request(stream: TcpStream, error: impl ToString) -> Result<()> {
    reply(
        stream,
        400,
        serde_json::json!({ "error": error.to_string() }).to_string(),
    )
}
