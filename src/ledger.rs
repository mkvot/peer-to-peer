use crate::{
    crypto::{hash_json, transaction_id},
    models::{GENESIS_LEDGER_HASH, Transaction, UnsignedTransaction},
    state::NodeState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IngestResult {
    Accepted,
    AlreadyKnown,
}

pub fn create_local_transaction(
    state: &mut NodeState,
    body: String,
) -> Result<Transaction, String> {
    state.local_seq += 1;
    let unsigned = UnsignedTransaction {
        origin: state.addr.clone(),
        seq: state.local_seq,
        body,
    };
    let id = transaction_id(&unsigned)?;

    Ok(Transaction {
        id,
        origin: unsigned.origin,
        seq: unsigned.seq,
        body: unsigned.body,
    })
}

pub fn ingest_transaction(state: &mut NodeState, tx: Transaction) -> Result<IngestResult, String> {
    validate_transaction(&tx)?;

    if state.ledger_ids.contains(&tx.id) || state.tx_pool.contains_key(&tx.id) {
        return Ok(IngestResult::AlreadyKnown);
    }

    state.tx_pool.insert(tx.id.clone(), tx.clone());

    if !state.consensus_enabled {
        append_direct(state, tx)?;
    }

    Ok(IngestResult::Accepted)
}

pub fn validate_transaction(tx: &Transaction) -> Result<(), String> {
    let unsigned = UnsignedTransaction {
        origin: tx.origin.clone(),
        seq: tx.seq,
        body: tx.body.clone(),
    };
    let expected = transaction_id(&unsigned)?;

    if tx.id != expected {
        return Err("transaction id does not match transaction content".to_string());
    }

    Ok(())
}

fn append_direct(state: &mut NodeState, tx: Transaction) -> Result<(), String> {
    state.tx_pool.shift_remove(&tx.id);

    if state.ledger_ids.insert(tx.id.clone()) {
        state.ledger.push(tx);
        state.ledger_hash = ledger_hash(&state.ledger)?;
    }

    Ok(())
}

fn ledger_hash(ledger: &[Transaction]) -> Result<String, String> {
    if ledger.is_empty() {
        return Ok(GENESIS_LEDGER_HASH.to_string());
    }

    hash_json(&ledger)
}
