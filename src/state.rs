use std::{
    collections::{HashMap, HashSet},
    env,
    path::PathBuf,
};

use indexmap::IndexMap;

use crate::models::{Commit, GENESIS_LEDGER_HASH, Transaction};

#[derive(Clone)]
pub struct NodeState {
    pub addr: String,
    pub bind_addr: String,
    pub peers: Vec<String>,
    pub blocks: IndexMap<String, String>,
    pub transactions: HashMap<String, String>,
    pub tx_pool: IndexMap<String, Transaction>,
    pub ledger: Vec<Transaction>,
    pub ledger_ids: HashSet<String>,
    pub commits: Vec<Commit>,
    pub ledger_hash: String,
    pub next_round: u64,
    pub local_seq: u64,
    pub consensus_enabled: bool,
    pub forward_inv_enabled: bool,
    pub round_secs: u64,
    pub data_dir: PathBuf,
    pub blocked_peers: HashSet<String>,
}

impl NodeState {
    pub fn new(addr: String, bind_addr: String) -> Self {
        let data_dir = node_data_dir(&addr);
        NodeState {
            addr,
            bind_addr,
            peers: Vec::new(),
            blocks: IndexMap::new(),
            transactions: HashMap::new(),
            tx_pool: IndexMap::new(),
            ledger: Vec::new(),
            ledger_ids: HashSet::new(),
            commits: Vec::new(),
            ledger_hash: GENESIS_LEDGER_HASH.to_string(),
            next_round: 0,
            local_seq: 0,
            consensus_enabled: env_bool("P2P_CONSENSUS", false),
            forward_inv_enabled: env_bool("P2P_FORWARD_INV", true),
            round_secs: env_u64("P2P_ROUND_SECS", 5),
            data_dir,
            blocked_peers: HashSet::new(),
        }
    }
}

fn env_bool(name: &str, default: bool) -> bool {
    match env::var(name) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => default,
        },
        Err(_) => default,
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn node_data_dir(addr: &str) -> PathBuf {
    let port = addr.rsplit(':').next().unwrap_or("unknown");
    let base = env::var("P2P_DATA_DIR").unwrap_or_else(|_| "data".to_string());
    PathBuf::from(base).join(port)
}
