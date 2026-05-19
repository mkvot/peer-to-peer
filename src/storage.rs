use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
};

use crate::{
    chain::{self, BlockAddStatus},
    models::{Block, EventRecord, SignedTransaction},
    state::NodeState,
};

pub fn init_storage(state: &mut NodeState) -> io::Result<()> {
    fs::create_dir_all(&state.data_dir)?;
    load_events(state)?;
    load_peers(state)?;
    load_blocks(state)?;
    load_mempool(state)?;
    chain::prune_mempool(&mut state.chain);
    persist_mempool(state)?;
    persist_chain_outputs(state)
}

pub fn persist_block(state: &NodeState, block: &Block) -> io::Result<()> {
    fs::create_dir_all(&state.data_dir)?;
    let path = state.data_dir.join("blocks.jsonl");
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(block).map_err(invalid_data)?;
    writeln!(file, "{line}")?;
    Ok(())
}

pub fn persist_mempool(state: &NodeState) -> io::Result<()> {
    fs::create_dir_all(&state.data_dir)?;
    let path = state.data_dir.join("mempool.json");
    let txs: Vec<SignedTransaction> = state.chain.mempool.values().cloned().collect();
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, &txs).map_err(invalid_data)
}

pub fn persist_peers(state: &NodeState) -> io::Result<()> {
    fs::create_dir_all(&state.data_dir)?;
    let path = state.data_dir.join("peers.json");
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, &state.peers).map_err(invalid_data)
}

pub fn persist_chain_outputs(state: &NodeState) -> io::Result<()> {
    fs::create_dir_all(&state.data_dir)?;

    let status_path = state.data_dir.join("chain_status.json");
    let status_file = File::create(status_path)?;
    serde_json::to_writer_pretty(status_file, &chain::chain_status_json(&state.chain))
        .map_err(invalid_data)?;

    let balances_path = state.data_dir.join("balances.json");
    let balances_file = File::create(balances_path)?;
    serde_json::to_writer_pretty(balances_file, &chain::balances_for_best(&state.chain))
        .map_err(invalid_data)
}

fn load_events(state: &mut NodeState) -> io::Result<()> {
    let path = state.data_dir.join("events.jsonl");
    if !path.exists() {
        return Ok(());
    }

    let file = File::open(path)?;
    for line in BufReader::new(file).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_str::<EventRecord>(&line) {
            state.events.push_back(event);
            while state.events.len() > crate::models::EVENT_LIMIT {
                state.events.pop_front();
            }
        }
    }
    Ok(())
}

fn load_peers(state: &mut NodeState) -> io::Result<()> {
    let path = state.data_dir.join("peers.json");
    if !path.exists() {
        return Ok(());
    }

    let file = File::open(path)?;
    let peers: Vec<String> = serde_json::from_reader(file).map_err(invalid_data)?;
    state.peers.extend(peers);
    state.peers.retain(|peer| peer != &state.addr);
    state.peers.sort();
    state.peers.dedup();
    Ok(())
}

fn load_blocks(state: &mut NodeState) -> io::Result<()> {
    let path = state.data_dir.join("blocks.jsonl");
    if !path.exists() {
        return Ok(());
    }

    let file = File::open(path)?;
    for line in BufReader::new(file).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let block: Block = serde_json::from_str(&line).map_err(invalid_data)?;
        let outcome = chain::add_block(&mut state.chain, block, state.config.difficulty)
            .map_err(invalid_data)?;
        if outcome.status == BlockAddStatus::Orphan {
            continue;
        }
    }
    Ok(())
}

fn load_mempool(state: &mut NodeState) -> io::Result<()> {
    let path = state.data_dir.join("mempool.json");
    if !path.exists() {
        return Ok(());
    }

    let file = File::open(path)?;
    let txs: Vec<SignedTransaction> = serde_json::from_reader(file).map_err(invalid_data)?;
    for tx in txs {
        chain::add_transaction_to_mempool(&mut state.chain, tx).map_err(invalid_data)?;
    }
    Ok(())
}

fn invalid_data(error: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}
