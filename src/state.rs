use std::{
    collections::VecDeque,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::PathBuf,
};

use crate::{
    chain,
    config::NodeConfig,
    crypto::now_millis,
    models::{ChainState, EVENT_LIMIT, EventRecord, RecentEvents, Wallet},
    wallet::load_or_create_wallet,
};

pub struct NodeState {
    pub addr: String,
    pub bind_addr: String,
    pub peers: Vec<String>,
    pub wallet: Wallet,
    pub chain: ChainState,
    pub config: NodeConfig,
    pub data_dir: PathBuf,
    pub events: RecentEvents,
}

impl NodeState {
    pub fn new(addr: String, bind_addr: String, config: NodeConfig) -> io::Result<Self> {
        let data_dir = node_data_dir(&addr, config.data_dir_base.clone());
        let wallet = load_or_create_wallet(&data_dir)?;
        Ok(Self {
            addr,
            bind_addr,
            peers: Vec::new(),
            wallet,
            chain: ChainState::new(config.difficulty),
            config,
            data_dir,
            events: VecDeque::new(),
        })
    }

    pub fn record_event(&mut self, kind: &str, message: impl Into<String>) {
        let event = EventRecord {
            time_ms: now_millis() as u128,
            kind: kind.to_string(),
            message: message.into(),
        };

        println!("[{}] {}", event.kind, event.message);

        self.events.push_back(event.clone());
        while self.events.len() > EVENT_LIMIT {
            self.events.pop_front();
        }

        if fs::create_dir_all(&self.data_dir).is_ok() {
            let path = self.data_dir.join("events.jsonl");
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                if let Ok(line) = serde_json::to_string(&event) {
                    let _ = writeln!(file, "{line}");
                }
            }
        }
    }

    pub fn status_json(&self) -> serde_json::Value {
        let best = chain::best_block(&self.chain);
        serde_json::json!({
            "addr": self.addr,
            "height": best.map(|block| block.height).unwrap_or(0),
            "tip": self.chain.best_tip,
            "mempool": self.chain.mempool.len(),
            "peers": self.peers.len(),
            "orphans": chain::orphan_count(&self.chain),
            "mining": self.chain.mining.active,
            "difficulty": self.config.difficulty,
        })
    }
}

pub fn node_data_dir(addr: &str, base: PathBuf) -> PathBuf {
    let port = addr.rsplit(':').next().unwrap_or("unknown");
    base.join(port)
}
