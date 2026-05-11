use std::{collections::HashSet, path::PathBuf};

use indexmap::IndexMap;

use crate::models::{Commit, GENESIS_LEDGER_HASH, Transaction};

pub struct NodeConfig {
    pub consensus_enabled: bool,
    pub forward_inv_enabled: bool,
    pub round_secs: u64,
    pub data_dir_base: PathBuf,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            consensus_enabled: true,
            forward_inv_enabled: false,
            round_secs: 2,
            data_dir_base: PathBuf::from("data"),
        }
    }
}

#[derive(Clone)]
pub struct NodeState {
    pub addr: String,
    pub bind_addr: String,
    pub peers: Vec<String>,
    pub blocks: IndexMap<String, String>,
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
    #[cfg(test)]
    pub fn new(addr: String, bind_addr: String) -> Self {
        Self::with_config(addr, bind_addr, NodeConfig::default())
    }

    pub fn with_config(addr: String, bind_addr: String, config: NodeConfig) -> Self {
        let data_dir = node_data_dir(&addr, config.data_dir_base);
        NodeState {
            addr,
            bind_addr,
            peers: Vec::new(),
            blocks: IndexMap::new(),
            tx_pool: IndexMap::new(),
            ledger: Vec::new(),
            ledger_ids: HashSet::new(),
            commits: Vec::new(),
            ledger_hash: GENESIS_LEDGER_HASH.to_string(),
            next_round: 0,
            local_seq: 0,
            consensus_enabled: config.consensus_enabled,
            forward_inv_enabled: config.forward_inv_enabled,
            round_secs: config.round_secs,
            data_dir,
            blocked_peers: HashSet::new(),
        }
    }
}

fn node_data_dir(addr: &str, base: PathBuf) -> PathBuf {
    let port = addr.rsplit(':').next().unwrap_or("unknown");
    base.join(port)
}
