use std::collections::{HashMap, HashSet, VecDeque};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

pub const BLOCK_REWARD: u64 = 1;
pub const EVENT_LIMIT: usize = 200;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalletFile {
    pub public_key: String,
    pub secret_key: String,
}

#[derive(Clone, Debug)]
pub struct Wallet {
    pub public_key: String,
    pub secret_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransactionPayload {
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub timestamp: u64,
    pub memo: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignedTransaction {
    pub id: String,
    pub payload: TransactionPayload,
    pub signature: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockHeader {
    pub height: u64,
    pub previous_hash: String,
    pub timestamp: u64,
    pub nonce: u64,
    pub difficulty: u32,
    pub creator: String,
    pub merkle_root: String,
    pub tx_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Block {
    pub header: BlockHeader,
    pub hash: String,
    pub transactions: Vec<SignedTransaction>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredBlock {
    pub block: Block,
    pub height: u64,
    pub total_transactions: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MiningStatus {
    pub active: bool,
    pub current_height: u64,
    pub candidate_parent: String,
    pub attempts: u64,
    pub last_hash: String,
    pub last_mined_hash: String,
    pub started_at_ms: u128,
}

impl Default for MiningStatus {
    fn default() -> Self {
        Self {
            active: false,
            current_height: 0,
            candidate_parent: String::new(),
            attempts: 0,
            last_hash: String::new(),
            last_mined_hash: String::new(),
            started_at_ms: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ChainState {
    pub blocks: IndexMap<String, StoredBlock>,
    pub best_tip: String,
    pub mempool: IndexMap<String, SignedTransaction>,
    pub orphan_blocks: IndexMap<String, Vec<Block>>,
    pub blocked_peers: HashSet<String>,
    pub mining: MiningStatus,
    pub balances_cache: IndexMap<String, HashMap<String, u64>>,
    pub tx_ids_cache: IndexMap<String, HashSet<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventRecord {
    pub time_ms: u128,
    pub kind: String,
    pub message: String,
}

pub type RecentEvents = VecDeque<EventRecord>;
