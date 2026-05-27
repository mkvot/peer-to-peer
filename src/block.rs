use crate::{
    crypto::{hash_json, sha256_hex},
    models::{Block, BlockHeader, SignedTransaction},
};

pub fn merkle_root(txs: &[SignedTransaction]) -> Result<String, String> {
    if txs.is_empty() {
        return Ok(sha256_hex(""));
    }

    let mut level: Vec<String> = txs.iter().map(|tx| tx.id.clone()).collect();
    while level.len() > 1 {
        if level.len() % 2 == 1 {
            let last = level.last().cloned().unwrap();
            level.push(last);
        }

        let mut next = Vec::with_capacity(level.len() / 2);
        for pair in level.chunks(2) {
            next.push(sha256_hex(&format!("{}{}", pair[0], pair[1])));
        }
        level = next;
    }

    Ok(level[0].clone())
}

pub fn block_hash(header: &BlockHeader) -> Result<String, String> {
    hash_json(header)
}

pub fn proof_target(difficulty: u32) -> String {
    "0".repeat(difficulty as usize)
}

pub fn has_valid_pow(hash: &str, difficulty: u32) -> bool {
    hash.starts_with(&proof_target(difficulty))
}

pub fn validate_basic_block(block: &Block, difficulty: u32) -> Result<(), String> {
    let expected_hash = block_hash(&block.header)?;
    if block.hash != expected_hash {
        return Err("block hash does not match header".to_string());
    }
    if block.header.difficulty != difficulty {
        return Err("block difficulty does not match this node".to_string());
    }
    if !has_valid_pow(&block.hash, difficulty) {
        return Err("invalid proof of work".to_string());
    }
    if block.header.tx_count != block.transactions.len() {
        return Err("block transaction count does not match header".to_string());
    }
    let expected_root = merkle_root(&block.transactions)?;
    if block.header.merkle_root != expected_root {
        return Err("block Merkle root does not match transactions".to_string());
    }
    if block.header.height > 0 && block.header.timestamp == 0 {
        return Err("non-genesis block timestamp must be nonzero".to_string());
    }
    Ok(())
}

pub fn genesis_block(difficulty: u32) -> Block {
    let transactions = Vec::new();
    let header = BlockHeader {
        height: 0,
        previous_hash: "0".to_string(),
        timestamp: 0,
        nonce: 0,
        difficulty,
        creator: "genesis".to_string(),
        merkle_root: merkle_root(&transactions).expect("empty merkle root should hash"),
        tx_count: 0,
    };
    let hash = block_hash(&header).expect("genesis header should serialize");
    Block {
        header,
        hash,
        transactions,
    }
}
