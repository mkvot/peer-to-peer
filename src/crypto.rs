use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::models::{CommitPayload, UnsignedTransaction};

pub fn calculate_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn hash_json<T: Serialize>(value: &T) -> Result<String, String> {
    let json = serde_json::to_string(value).map_err(|e| e.to_string())?;
    Ok(calculate_hash(&json))
}

pub fn transaction_id(tx: &UnsignedTransaction) -> Result<String, String> {
    hash_json(tx)
}

pub fn commit_hash(payload: &CommitPayload) -> Result<String, String> {
    hash_json(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CommitPayload, GENESIS_LEDGER_HASH, Transaction};

    #[test]
    fn transaction_hash_is_stable() {
        let tx = UnsignedTransaction {
            origin: "127.0.0.1:9001".to_string(),
            seq: 1,
            body: "S1 sends T1".to_string(),
        };

        let first = transaction_id(&tx).unwrap();
        let second = transaction_id(&tx).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn transaction_hash_changes_when_fields_change() {
        let tx = UnsignedTransaction {
            origin: "127.0.0.1:9001".to_string(),
            seq: 1,
            body: "S1 sends T1".to_string(),
        };
        let changed = UnsignedTransaction {
            origin: "127.0.0.1:9001".to_string(),
            seq: 2,
            body: "S1 sends T1".to_string(),
        };

        assert_ne!(
            transaction_id(&tx).unwrap(),
            transaction_id(&changed).unwrap()
        );
    }

    #[test]
    fn commit_hash_is_stable() {
        let tx = Transaction {
            id: "tx-1".to_string(),
            origin: "127.0.0.1:9001".to_string(),
            seq: 1,
            body: "S1 sends T1".to_string(),
        };
        let payload = CommitPayload {
            round: 0,
            prev_ledger_hash: GENESIS_LEDGER_HASH.to_string(),
            leader: "127.0.0.1:9001".to_string(),
            members: vec!["127.0.0.1:9001".to_string(), "127.0.0.1:9002".to_string()],
            votes: vec!["127.0.0.1:9001".to_string(), "127.0.0.1:9002".to_string()],
            txs: vec![tx],
        };

        let first = commit_hash(&payload).unwrap();
        let second = commit_hash(&payload).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }
}
