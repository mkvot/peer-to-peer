use std::sync::{Arc, Mutex};

use crate::{
    block::{block_hash, has_valid_pow, merkle_root},
    chain::{self, BlockAddStatus},
    crypto::{now_millis, short_hash},
    models::{Block, BlockHeader, SignedTransaction, StoredBlock},
    state::NodeState,
    storage,
    transaction::reward_transaction,
};

struct Candidate {
    parent: StoredBlock,
    header: BlockHeader,
    transactions: Vec<SignedTransaction>,
}

enum MineResult {
    Mined(Block),
    Restart,
}

pub fn mine_blocks(
    state: Arc<Mutex<NodeState>>,
    blocks: u64,
    max_txs: usize,
) -> Result<Vec<Block>, String> {
    let mut mined = Vec::new();

    for _ in 0..blocks {
        loop {
            let candidate = prepare_candidate(&state, max_txs)?;
            match mine_candidate(&state, candidate)? {
                MineResult::Mined(block) => {
                    mined.push(block);
                    break;
                }
                MineResult::Restart => continue,
            }
        }
    }

    if let Ok(mut node) = state.lock() {
        node.chain.mining.active = false;
    }

    Ok(mined)
}

fn prepare_candidate(state: &Arc<Mutex<NodeState>>, max_txs: usize) -> Result<Candidate, String> {
    let mut node = state.lock().unwrap();
    let parent = chain::best_block(&node.chain)
        .cloned()
        .ok_or_else(|| "best chain tip is missing".to_string())?;
    let selected = chain::select_mempool_transactions(&node.chain, &parent, max_txs);
    let timestamp = now_millis().max(parent.block.header.timestamp + 1);
    let reward = reward_transaction(&node.wallet.public_key, timestamp)?;

    let mut transactions = Vec::with_capacity(selected.len() + 1);
    transactions.push(reward);
    transactions.extend(selected);

    let merkle_root = merkle_root(&transactions)?;
    let header = BlockHeader {
        height: parent.height + 1,
        previous_hash: parent.block.hash.clone(),
        timestamp,
        nonce: 0,
        difficulty: node.config.difficulty,
        creator: node.wallet.public_key.clone(),
        merkle_root,
        tx_count: transactions.len(),
    };

    node.chain.mining.active = true;
    node.chain.mining.current_height = header.height;
    node.chain.mining.candidate_parent = parent.block.hash.clone();
    node.chain.mining.attempts = 0;
    node.chain.mining.last_hash.clear();
    node.chain.mining.started_at_ms = now_millis() as u128;
    node.record_event(
        "mine",
        format!(
            "start parent={} height={} txs={} difficulty={}",
            short_hash(&parent.block.hash),
            header.height,
            transactions.len(),
            header.difficulty
        ),
    );

    Ok(Candidate {
        parent,
        header,
        transactions,
    })
}

fn mine_candidate(
    state: &Arc<Mutex<NodeState>>,
    mut candidate: Candidate,
) -> Result<MineResult, String> {
    let parent_hash = candidate.parent.block.hash.clone();
    let difficulty = candidate.header.difficulty;
    let mut nonce = 0u64;

    loop {
        candidate.header.nonce = nonce;
        let hash = block_hash(&candidate.header)?;
        let attempts = nonce + 1;

        if nonce % 10_000 == 0 {
            let mut node = state.lock().unwrap();
            if node.chain.best_tip != parent_hash {
                let old = short_hash(&parent_hash);
                let new = short_hash(&node.chain.best_tip);
                node.record_event(
                    "mine",
                    format!("restart parent_changed old={old} new={new}"),
                );
                return Ok(MineResult::Restart);
            }
            node.chain.mining.attempts = attempts;
            node.chain.mining.last_hash = hash.clone();
            if nonce > 0 && nonce % 100_000 == 0 {
                node.record_event(
                    "mine",
                    format!(
                        "progress height={} attempts={} last={}",
                        candidate.header.height,
                        attempts,
                        short_hash(&hash)
                    ),
                );
            }
        }

        if has_valid_pow(&hash, difficulty) {
            let block = Block {
                header: candidate.header.clone(),
                hash: hash.clone(),
                transactions: candidate.transactions.clone(),
            };

            let mut node = state.lock().unwrap();
            if node.chain.best_tip != parent_hash {
                let old = short_hash(&parent_hash);
                let new = short_hash(&node.chain.best_tip);
                node.record_event(
                    "mine",
                    format!("restart parent_changed old={old} new={new}"),
                );
                return Ok(MineResult::Restart);
            }

            let node_difficulty = node.config.difficulty;
            let outcome = chain::add_block(&mut node.chain, block.clone(), node_difficulty)?;
            if outcome.status != BlockAddStatus::Added {
                node.chain.mining.active = false;
                return Err(format!(
                    "mined block was not added: {}",
                    outcome.status.as_str()
                ));
            }

            for added in &outcome.added_blocks {
                storage::persist_block(&node, added).map_err(|e| e.to_string())?;
                node.record_event(
                    "block",
                    format!(
                        "added height={} hash={}",
                        added.header.height,
                        short_hash(&added.hash)
                    ),
                );
            }
            storage::persist_mempool(&node).map_err(|e| e.to_string())?;
            storage::persist_chain_outputs(&node).map_err(|e| e.to_string())?;

            node.chain.mining.attempts = attempts;
            node.chain.mining.last_hash = hash.clone();
            node.chain.mining.last_mined_hash = hash.clone();
            node.record_event(
                "mine",
                format!(
                    "mined height={} hash={} attempts={}",
                    block.header.height,
                    short_hash(&block.hash),
                    attempts
                ),
            );
            return Ok(MineResult::Mined(block));
        }

        nonce = nonce.wrapping_add(1);
    }
}
