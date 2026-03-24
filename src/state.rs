use std::collections::HashMap;

use indexmap::IndexMap;

#[derive(Clone)]
pub struct NodeState {
    pub addr: String,
    pub bind_addr: String,
    pub peers: Vec<String>,
    pub blocks: IndexMap<String, String>,
    pub transactions: HashMap<String, String>,
}

impl NodeState {
    pub fn new(addr: String, bind_addr: String) -> Self {
        NodeState {
            addr,
            bind_addr,
            peers: Vec::new(),
            blocks: IndexMap::new(),
            transactions: HashMap::new(),
        }
    }
}