use std::{
    io::{Error, ErrorKind, Result, Write},
    net::TcpStream,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use crate::{
    chain::{self, BlockAddOutcome, BlockAddStatus},
    crypto::short_hash,
    http::{Request, Response, read_response},
    models::{Block, SignedTransaction},
    state::NodeState,
    storage,
};

pub fn start(state: Arc<Mutex<NodeState>>) -> Result<()> {
    announce_to_initial_peers(&state);

    let mut tick = 0u64;
    loop {
        thread::sleep(Duration::from_secs(5));
        tick += 1;

        let peers = active_peers(&state);
        for peer in peers {
            let _ = sync_transactions(&peer, &state);
            let _ = sync_blocks(&peer, &state);
            if tick % 2 == 0 {
                let _ = sync_peers(&peer, &state);
            }
        }
    }
}

pub fn broadcast_transaction(tx: SignedTransaction, state: Arc<Mutex<NodeState>>) {
    thread::spawn(move || {
        let peers = active_peers(&state);
        let body = match serde_json::to_string(&tx) {
            Ok(body) => body,
            Err(_) => return,
        };
        for peer in peers {
            let _ = post_json(&peer, "/transactions", body.clone(), &state);
        }
    });
}

pub fn broadcast_block(block: Block, state: Arc<Mutex<NodeState>>) {
    thread::spawn(move || {
        let peers = active_peers(&state);
        let body = match serde_json::to_string(&block) {
            Ok(body) => body,
            Err(_) => return,
        };
        for peer in peers {
            let _ = post_json(&peer, "/blocks", body.clone(), &state);
        }
    });
}

pub fn send_request(addr: &str, request: Request) -> Result<Response> {
    let mut stream = TcpStream::connect(addr)?;
    stream.write_all(&request.to_http_bytes())?;
    read_response(&mut stream)
}

fn announce_to_initial_peers(state: &Arc<Mutex<NodeState>>) {
    let peers = active_peers(state);
    for peer in peers {
        let _ = announce(&peer, state);
    }
}

fn active_peers(state: &Arc<Mutex<NodeState>>) -> Vec<String> {
    let node = state.lock().unwrap();
    node.peers
        .iter()
        .filter(|peer| *peer != &node.addr && !node.chain.blocked_peers.contains(*peer))
        .cloned()
        .collect()
}

fn my_addr(state: &Arc<Mutex<NodeState>>) -> String {
    state.lock().unwrap().addr.clone()
}

fn get_json(addr: &str, path: &str, state: &Arc<Mutex<NodeState>>) -> Result<Response> {
    let my_addr = my_addr(state);
    let request = Request::get(path, &my_addr, addr);
    send_request(addr, request)
}

fn post_json(
    addr: &str,
    path: &str,
    body: String,
    state: &Arc<Mutex<NodeState>>,
) -> Result<Response> {
    let my_addr = my_addr(state);
    let request = Request::post(path, &my_addr, addr, body);
    send_request(addr, request)
}

fn announce(addr: &str, state: &Arc<Mutex<NodeState>>) -> Result<()> {
    let my_addr = my_addr(state);
    let body = serde_json::json!({ "address": my_addr }).to_string();
    let response = post_json(addr, "/peers", body, state)?;
    if response.status != 200 {
        return Err(Error::new(ErrorKind::Other, "peer announcement failed"));
    }
    merge_peers_from_body(&response.body, state)
}

fn sync_peers(addr: &str, state: &Arc<Mutex<NodeState>>) -> Result<()> {
    let response = get_json(addr, "/peers", state)?;
    if response.status != 200 {
        return Ok(());
    }
    merge_peers_from_body(&response.body, state)
}

fn sync_transactions(addr: &str, state: &Arc<Mutex<NodeState>>) -> Result<()> {
    let response = get_json(addr, "/transactions", state)?;
    if response.status != 200 {
        return Ok(());
    }
    let txs: Vec<SignedTransaction> =
        serde_json::from_str(&response.body).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

    for tx in txs {
        let mut node = state.lock().unwrap();
        match chain::add_transaction_to_mempool(&mut node.chain, tx.clone()) {
            Ok(true) => {
                node.record_event(
                    "tx",
                    format!(
                        "accepted id={} from={} to={} amount={}",
                        short_hash(&tx.id),
                        short_hash(&tx.payload.from),
                        short_hash(&tx.payload.to),
                        tx.payload.amount
                    ),
                );
                let _ = storage::persist_mempool(&node);
            }
            Ok(false) => {}
            Err(error) => {
                node.record_event("tx", format!("rejected reason=\"{error}\""));
            }
        }
    }

    Ok(())
}

fn sync_blocks(addr: &str, state: &Arc<Mutex<NodeState>>) -> Result<()> {
    let response = get_json(addr, "/hashes", state)?;
    if response.status != 200 {
        return Ok(());
    }
    let peer_hashes: Vec<String> =
        serde_json::from_str(&response.body).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
    if peer_hashes.is_empty() {
        return Ok(());
    }

    let local_hashes = {
        let node = state.lock().unwrap();
        chain::canonical_hashes(&node.chain)
    };
    let common = common_prefix_len(&local_hashes, &peer_hashes);

    if peer_hashes.len() < local_hashes.len() {
        return Ok(());
    }
    if peer_hashes.len() == local_hashes.len() && local_hashes.last() == peer_hashes.last() {
        return Ok(());
    }

    for hash in peer_hashes.iter().skip(common) {
        if state.lock().unwrap().chain.blocks.contains_key(hash) {
            continue;
        }
        let response = get_json(addr, &format!("/blocks/{hash}"), state)?;
        if response.status != 200 {
            continue;
        }
        let block: Block = serde_json::from_str(&response.body)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        add_synced_block(block, state)?;
    }

    Ok(())
}

fn add_synced_block(block: Block, state: &Arc<Mutex<NodeState>>) -> Result<()> {
    let mut node = state.lock().unwrap();
    let difficulty = node.config.difficulty;
    match chain::add_block(&mut node.chain, block.clone(), difficulty) {
        Ok(outcome) => {
            handle_block_outcome_events(&mut node, &outcome);
            if outcome.status == BlockAddStatus::Added {
                for added in &outcome.added_blocks {
                    storage::persist_block(&node, added)?;
                }
                storage::persist_mempool(&node)?;
                storage::persist_chain_outputs(&node)?;
            }
            Ok(())
        }
        Err(error) => {
            node.record_event("block", format!("rejected reason=\"{error}\""));
            Ok(())
        }
    }
}

pub fn handle_block_outcome_events(node: &mut NodeState, outcome: &BlockAddOutcome) {
    match outcome.status {
        BlockAddStatus::Added => {
            for block in &outcome.added_blocks {
                node.record_event(
                    "block",
                    format!(
                        "added height={} hash={}",
                        block.header.height,
                        short_hash(&block.hash)
                    ),
                );
            }

            if outcome.old_tip != outcome.new_tip && tip_switch_is_reorg(node, outcome) {
                let old_height = node
                    .chain
                    .blocks
                    .get(&outcome.old_tip)
                    .map(|block| block.height)
                    .unwrap_or(0);
                let new_height = node
                    .chain
                    .blocks
                    .get(&outcome.new_tip)
                    .map(|block| block.height)
                    .unwrap_or(0);
                node.record_event(
                    "chain",
                    format!(
                        "reorg old={} height={} new={} height={}",
                        short_hash(&outcome.old_tip),
                        old_height,
                        short_hash(&outcome.new_tip),
                        new_height
                    ),
                );
            }

            for (hash, error) in &outcome.rejected_orphans {
                node.record_event(
                    "block",
                    format!("rejected orphan={} reason=\"{}\"", short_hash(hash), error),
                );
            }
        }
        BlockAddStatus::Duplicate => {}
        BlockAddStatus::Orphan => {}
    }
}

fn tip_switch_is_reorg(node: &NodeState, outcome: &BlockAddOutcome) -> bool {
    let Some(new_tip) = node.chain.blocks.get(&outcome.new_tip) else {
        return false;
    };
    new_tip.block.header.previous_hash != outcome.old_tip && outcome.old_tip != outcome.new_tip
}

fn merge_peers_from_body(body: &str, state: &Arc<Mutex<NodeState>>) -> Result<()> {
    let peers: Vec<String> =
        serde_json::from_str(body).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
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
    Ok(())
}

fn common_prefix_len(left: &[String], right: &[String]) -> usize {
    left.iter()
        .zip(right.iter())
        .take_while(|(a, b)| a == b)
        .count()
}
