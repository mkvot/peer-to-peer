use std::collections::HashMap;

#[derive(Clone)]
pub struct NodeState {
    pub addr: String,
    pub peers: Vec<String>,
    pub blocks: HashMap<String, String>,
    pub transactions: HashMap<String, String>,
}

impl NodeState {
    pub fn new(addr: String) -> Self {
        NodeState {
            addr,
            peers: Vec::new(),
            blocks: HashMap::new(),
            transactions: HashMap::new(),
        }
    }
}