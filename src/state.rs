use std::collections::HashMap;

#[derive(Clone)]
pub struct NodeState {
    pub peers: Vec<String>,
    pub blocks: HashMap<String, String>,
    pub transactions: HashMap<String, String>,
}

impl NodeState {
    pub fn new() -> Self {
        NodeState {
            peers: Vec::new(),
            blocks: HashMap::new(),
            transactions: HashMap::new(),
        }
    }
}