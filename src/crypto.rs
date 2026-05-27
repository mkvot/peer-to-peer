use std::time::{SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn sha256_bytes(content: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(content);
    hasher.finalize().to_vec()
}

pub fn canonical_json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| e.to_string())
}

pub fn hash_json<T: Serialize>(value: &T) -> Result<String, String> {
    Ok(sha256_hex(&canonical_json(value)?))
}

pub fn sign_hex(secret_key: &str, message: &str) -> Result<String, String> {
    let secret = decode_32(secret_key)?;
    let signing_key = SigningKey::from_bytes(&secret);
    let signature = signing_key.sign(message.as_bytes());
    Ok(hex::encode(signature.to_bytes()))
}

pub fn verify_hex(public_key: &str, message: &str, signature: &str) -> Result<(), String> {
    let public = decode_32(public_key)?;
    let signature_bytes = hex::decode(signature).map_err(|e| e.to_string())?;
    let signature = Signature::from_slice(&signature_bytes).map_err(|e| e.to_string())?;
    let verifying_key = VerifyingKey::from_bytes(&public).map_err(|e| e.to_string())?;
    verifying_key
        .verify(message.as_bytes(), &signature)
        .map_err(|e| e.to_string())
}

pub fn public_key_for_secret(secret_key: &str) -> Result<String, String> {
    let secret = decode_32(secret_key)?;
    let signing_key = SigningKey::from_bytes(&secret);
    Ok(hex::encode(signing_key.verifying_key().to_bytes()))
}

pub fn seed_from_material(material: &str) -> String {
    let digest = sha256_bytes(material.as_bytes());
    hex::encode(&digest[..32])
}

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn short_hash(hash: &str) -> String {
    hash.chars().take(8).collect()
}

fn decode_32(hex_value: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(hex_value).map_err(|e| e.to_string())?;
    bytes
        .try_into()
        .map_err(|_| "expected 32-byte hex value".to_string())
}
