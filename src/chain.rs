use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use serde_json::json;

use crate::{
    block::{genesis_block, validate_basic_block},
    models::{Block, ChainState, SignedTransaction, StoredBlock},
    transaction::{
        is_reward_transaction, validate_normal_transaction, validate_reward_transaction,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockAddStatus {
    Added,
    Duplicate,
    Orphan,
}

#[derive(Clone, Debug)]
pub struct BlockAddOutcome {
    pub status: BlockAddStatus,
    pub added_blocks: Vec<Block>,
    pub rejected_orphans: Vec<(String, String)>,
    pub old_tip: String,
    pub new_tip: String,
}

impl BlockAddStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Duplicate => "duplicate",
            Self::Orphan => "orphan",
        }
    }
}

impl ChainState {
    pub fn new(difficulty: u32) -> Self {
        let genesis = genesis_block(difficulty);
        let genesis_hash = genesis.hash.clone();
        let stored = StoredBlock {
            block: genesis,
            height: 0,
            total_transactions: 0,
        };

        let mut blocks = IndexMap::new();
        blocks.insert(genesis_hash.clone(), stored);

        let mut balances_cache = IndexMap::new();
        balances_cache.insert(genesis_hash.clone(), HashMap::new());

        let mut tx_ids_cache = IndexMap::new();
        tx_ids_cache.insert(genesis_hash.clone(), HashSet::new());

        Self {
            blocks,
            best_tip: genesis_hash,
            mempool: IndexMap::new(),
            orphan_blocks: IndexMap::new(),
            blocked_peers: HashSet::new(),
            mining: Default::default(),
            balances_cache,
            tx_ids_cache,
        }
    }
}

pub fn add_transaction_to_mempool(
    chain: &mut ChainState,
    tx: SignedTransaction,
) -> Result<bool, String> {
    validate_normal_transaction(&tx)?;

    if chain.mempool.contains_key(&tx.id) || canonical_tx_ids(chain).contains(&tx.id) {
        return Ok(false);
    }

    chain.mempool.insert(tx.id.clone(), tx);
    Ok(true)
}

pub fn add_block(
    chain: &mut ChainState,
    block: Block,
    difficulty: u32,
) -> Result<BlockAddOutcome, String> {
    let old_tip = chain.best_tip.clone();

    if chain.blocks.contains_key(&block.hash) {
        return Ok(BlockAddOutcome {
            status: BlockAddStatus::Duplicate,
            added_blocks: Vec::new(),
            rejected_orphans: Vec::new(),
            old_tip: old_tip.clone(),
            new_tip: old_tip,
        });
    }

    validate_basic_block(&block, difficulty)?;

    if !chain.blocks.contains_key(&block.header.previous_hash) {
        let previous_hash = block.header.previous_hash.clone();
        if !orphan_contains(chain, &block.hash) {
            chain
                .orphan_blocks
                .entry(previous_hash)
                .or_default()
                .push(block);
        }
        return Ok(BlockAddOutcome {
            status: BlockAddStatus::Orphan,
            added_blocks: Vec::new(),
            rejected_orphans: Vec::new(),
            old_tip: old_tip.clone(),
            new_tip: old_tip,
        });
    }

    let mut outcome = BlockAddOutcome {
        status: BlockAddStatus::Added,
        added_blocks: Vec::new(),
        rejected_orphans: Vec::new(),
        old_tip,
        new_tip: String::new(),
    };
    connect_block_recursive(chain, block, difficulty, &mut outcome)?;
    outcome.new_tip = chain.best_tip.clone();
    Ok(outcome)
}

pub fn validate_block_against_parent(
    block: &Block,
    parent: &StoredBlock,
    chain: &ChainState,
    difficulty: u32,
) -> Result<(), String> {
    validate_basic_block(block, difficulty)?;
    if block.header.previous_hash != parent.block.hash {
        return Err("block parent hash does not match parent".to_string());
    }
    if block.header.height != parent.height + 1 {
        return Err("block height does not extend parent".to_string());
    }
    derive_child_caches(parent, block, chain).map(|_| ())
}

pub fn better_tip(candidate: &StoredBlock, current: &StoredBlock) -> bool {
    if candidate.height != current.height {
        return candidate.height > current.height;
    }
    if candidate.total_transactions != current.total_transactions {
        return candidate.total_transactions > current.total_transactions;
    }
    if candidate.block.header.timestamp != current.block.header.timestamp {
        return candidate.block.header.timestamp > current.block.header.timestamp;
    }
    candidate.block.hash < current.block.hash
}

pub fn canonical_hashes(chain: &ChainState) -> Vec<String> {
    let mut hashes = Vec::new();
    let mut next = chain.best_tip.clone();

    while let Some(stored) = chain.blocks.get(&next) {
        hashes.push(next.clone());
        if stored.height == 0 {
            break;
        }
        next = stored.block.header.previous_hash.clone();
    }

    hashes.reverse();
    hashes
}

pub fn canonical_blocks(chain: &ChainState) -> Vec<Block> {
    canonical_hashes(chain)
        .iter()
        .filter_map(|hash| chain.blocks.get(hash).map(|stored| stored.block.clone()))
        .collect()
}

pub fn canonical_tx_ids(chain: &ChainState) -> HashSet<String> {
    chain
        .tx_ids_cache
        .get(&chain.best_tip)
        .cloned()
        .unwrap_or_default()
}

pub fn balances_for_best(chain: &ChainState) -> HashMap<String, u64> {
    chain
        .balances_cache
        .get(&chain.best_tip)
        .cloned()
        .unwrap_or_default()
}

pub fn best_block(chain: &ChainState) -> Option<&StoredBlock> {
    chain.blocks.get(&chain.best_tip)
}

pub fn chain_status_json(chain: &ChainState) -> serde_json::Value {
    let best = best_block(chain);
    json!({
        "height": best.map(|b| b.height).unwrap_or(0),
        "tip": chain.best_tip,
        "total_transactions": best.map(|b| b.total_transactions).unwrap_or(0),
        "known_blocks": chain.blocks.len(),
        "orphans": orphan_count(chain),
        "mempool": chain.mempool.len(),
    })
}

pub fn orphan_count(chain: &ChainState) -> usize {
    chain.orphan_blocks.values().map(Vec::len).sum()
}

pub fn select_mempool_transactions(
    chain: &ChainState,
    parent: &StoredBlock,
    max_txs: usize,
) -> Vec<SignedTransaction> {
    let mut balances = chain
        .balances_cache
        .get(&parent.block.hash)
        .cloned()
        .unwrap_or_default();
    let mut tx_ids = chain
        .tx_ids_cache
        .get(&parent.block.hash)
        .cloned()
        .unwrap_or_default();
    let mut selected = Vec::new();

    for tx in chain.mempool.values() {
        if selected.len() >= max_txs {
            break;
        }
        if is_reward_transaction(tx) || tx_ids.contains(&tx.id) {
            continue;
        }
        if validate_normal_transaction(tx).is_err() {
            continue;
        }

        let from = tx.payload.from.clone();
        let amount = tx.payload.amount;
        let available = balances.get(&from).copied().unwrap_or(0);
        if available < amount {
            continue;
        }

        balances.insert(from.clone(), available - amount);
        let to_balance = balances.get(&tx.payload.to).copied().unwrap_or(0);
        balances.insert(tx.payload.to.clone(), to_balance + amount);
        tx_ids.insert(tx.id.clone());
        selected.push(tx.clone());
    }

    selected
}

pub fn prune_mempool(chain: &mut ChainState) {
    let tx_ids = canonical_tx_ids(chain);
    let to_remove: Vec<String> = chain
        .mempool
        .keys()
        .filter(|id| tx_ids.contains(*id))
        .cloned()
        .collect();
    for id in to_remove {
        chain.mempool.shift_remove(&id);
    }
}

fn connect_block_recursive(
    chain: &mut ChainState,
    block: Block,
    difficulty: u32,
    outcome: &mut BlockAddOutcome,
) -> Result<(), String> {
    if chain.blocks.contains_key(&block.hash) {
        return Ok(());
    }

    let parent = chain
        .blocks
        .get(&block.header.previous_hash)
        .cloned()
        .ok_or_else(|| "parent block is unknown".to_string())?;
    validate_block_against_parent(&block, &parent, chain, difficulty)?;

    let (balances, tx_ids) = derive_child_caches(&parent, &block, chain)?;
    let stored = StoredBlock {
        height: block.header.height,
        total_transactions: parent.total_transactions + block.transactions.len() as u64,
        block: block.clone(),
    };
    let stored_for_choice = stored.clone();
    let hash = block.hash.clone();

    chain.blocks.insert(hash.clone(), stored);
    chain.balances_cache.insert(hash.clone(), balances);
    chain.tx_ids_cache.insert(hash.clone(), tx_ids);
    outcome.added_blocks.push(block);

    let current = chain
        .blocks
        .get(&chain.best_tip)
        .cloned()
        .ok_or_else(|| "current best tip is missing".to_string())?;
    if better_tip(&stored_for_choice, &current) {
        chain.best_tip = hash.clone();
        prune_mempool(chain);
    }

    if let Some(children) = chain.orphan_blocks.shift_remove(&hash) {
        for child in children {
            let child_hash = child.hash.clone();
            if let Err(error) = connect_block_recursive(chain, child, difficulty, outcome) {
                outcome.rejected_orphans.push((child_hash, error));
            }
        }
    }

    Ok(())
}

fn derive_child_caches(
    parent: &StoredBlock,
    block: &Block,
    chain: &ChainState,
) -> Result<(HashMap<String, u64>, HashSet<String>), String> {
    let mut balances = chain
        .balances_cache
        .get(&parent.block.hash)
        .cloned()
        .ok_or_else(|| "parent balances cache is missing".to_string())?;
    let mut tx_ids = chain
        .tx_ids_cache
        .get(&parent.block.hash)
        .cloned()
        .ok_or_else(|| "parent transaction cache is missing".to_string())?;

    validate_and_apply_transactions(block, &mut balances, &mut tx_ids)?;
    Ok((balances, tx_ids))
}

fn validate_and_apply_transactions(
    block: &Block,
    balances: &mut HashMap<String, u64>,
    tx_ids: &mut HashSet<String>,
) -> Result<(), String> {
    if block.transactions.is_empty() {
        return Err("block must contain one reward transaction".to_string());
    }

    let mut reward_count = 0;
    for (index, tx) in block.transactions.iter().enumerate() {
        if is_reward_transaction(tx) {
            reward_count += 1;
            if index != 0 {
                return Err("reward transaction must be first".to_string());
            }
            validate_reward_transaction(tx, &block.header.creator)?;
        } else {
            validate_normal_transaction(tx)?;
        }

        if !tx_ids.insert(tx.id.clone()) {
            return Err("duplicate transaction id in branch".to_string());
        }

        if is_reward_transaction(tx) {
            let balance = balances.get(&tx.payload.to).copied().unwrap_or(0);
            balances.insert(tx.payload.to.clone(), balance + tx.payload.amount);
        } else {
            let from_balance = balances.get(&tx.payload.from).copied().unwrap_or(0);
            if from_balance < tx.payload.amount {
                return Err("sender balance is insufficient".to_string());
            }
            balances.insert(tx.payload.from.clone(), from_balance - tx.payload.amount);
            let to_balance = balances.get(&tx.payload.to).copied().unwrap_or(0);
            balances.insert(tx.payload.to.clone(), to_balance + tx.payload.amount);
        }
    }

    if reward_count != 1 {
        return Err("block must contain exactly one reward transaction".to_string());
    }

    Ok(())
}

fn orphan_contains(chain: &ChainState, hash: &str) -> bool {
    chain
        .orphan_blocks
        .values()
        .any(|blocks| blocks.iter().any(|block| block.hash == hash))
}
