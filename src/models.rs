use serde::{Deserialize, Serialize};

pub const GENESIS_LEDGER_HASH: &str = "0";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transaction {
    pub id: String,
    pub origin: String,
    pub seq: u64,
    pub body: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnsignedTransaction {
    pub origin: String,
    pub seq: u64,
    pub body: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Proposal {
    pub addr: String,
    pub round: u64,
    pub ledger_len: usize,
    pub ledger_hash: String,
    pub pending: Vec<Transaction>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommitPayload {
    pub round: u64,
    pub prev_ledger_hash: String,
    pub leader: String,
    pub members: Vec<String>,
    pub votes: Vec<String>,
    pub txs: Vec<Transaction>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Commit {
    pub payload: CommitPayload,
    pub commit_hash: String,
}
