use crate::{
    crypto::{canonical_json, now_millis, sha256_hex, sign_hex, verify_hex},
    models::{BLOCK_REWARD, SignedTransaction, TransactionPayload, Wallet},
};

pub fn transaction_id(payload: &TransactionPayload, signature: &str) -> Result<String, String> {
    let payload_json = canonical_json(payload)?;
    Ok(sha256_hex(&format!("{payload_json}{signature}")))
}

pub fn create_signed_transaction(
    wallet: &Wallet,
    to: String,
    amount: u64,
    memo: String,
) -> Result<SignedTransaction, String> {
    if amount == 0 {
        return Err("amount must be greater than zero".to_string());
    }

    let payload = TransactionPayload {
        from: wallet.public_key.clone(),
        to,
        amount,
        timestamp: now_millis(),
        memo,
    };
    sign_payload(wallet, payload)
}

pub fn sign_payload(
    wallet: &Wallet,
    payload: TransactionPayload,
) -> Result<SignedTransaction, String> {
    let payload_json = canonical_json(&payload)?;
    let signature = sign_hex(&wallet.secret_key, &payload_json)?;
    let id = transaction_id(&payload, &signature)?;
    Ok(SignedTransaction {
        id,
        payload,
        signature,
    })
}

pub fn reward_transaction(creator: &str, timestamp: u64) -> Result<SignedTransaction, String> {
    let payload = TransactionPayload {
        from: "0".to_string(),
        to: creator.to_string(),
        amount: BLOCK_REWARD,
        timestamp,
        memo: "block reward".to_string(),
    };
    let signature = String::new();
    let id = transaction_id(&payload, &signature)?;
    Ok(SignedTransaction {
        id,
        payload,
        signature,
    })
}

pub fn is_reward_transaction(tx: &SignedTransaction) -> bool {
    tx.payload.from == "0" && tx.signature.is_empty()
}

pub fn validate_transaction_id(tx: &SignedTransaction) -> Result<(), String> {
    let expected = transaction_id(&tx.payload, &tx.signature)?;
    if tx.id != expected {
        return Err("transaction id does not match payload and signature".to_string());
    }
    Ok(())
}

pub fn validate_normal_transaction(tx: &SignedTransaction) -> Result<(), String> {
    validate_transaction_id(tx)?;
    if is_reward_transaction(tx) {
        return Err("reward transactions are only valid inside mined blocks".to_string());
    }
    if tx.payload.amount == 0 {
        return Err("amount must be greater than zero".to_string());
    }

    let payload_json = canonical_json(&tx.payload)?;
    verify_hex(&tx.payload.from, &payload_json, &tx.signature)
        .map_err(|e| format!("invalid signature: {e}"))
}

pub fn validate_reward_transaction(tx: &SignedTransaction, creator: &str) -> Result<(), String> {
    validate_transaction_id(tx)?;
    if !is_reward_transaction(tx) {
        return Err("expected reward transaction".to_string());
    }
    if tx.payload.to != creator {
        return Err("reward recipient must match block creator".to_string());
    }
    if tx.payload.amount != BLOCK_REWARD {
        return Err("invalid reward amount".to_string());
    }
    Ok(())
}
