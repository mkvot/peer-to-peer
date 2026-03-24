use std::collections::HashMap;

#[derive(Clone)]
pub struct NodeState {
    pub addr: String,
    pub bind_addr: String,
    pub peers: Vec<String>,
    pub blocks: HashMap<String, String>,
    pub transactions: HashMap<String, String>,
}

impl NodeState {
    pub fn new(addr: String, bind_addr: String) -> Self {
        NodeState {
            addr,
            bind_addr,
            peers: Vec::new(),
            blocks: HashMap::new(),
            transactions: HashMap::new(),
        }
    }
}