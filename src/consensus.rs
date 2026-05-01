use std::{
    collections::HashSet,
    io,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use crate::{
    crypto::commit_hash,
    ledger,
    models::{Commit, CommitPayload, Proposal, Transaction},
    state::NodeState,
};

pub fn start_consensus_loop(state: Arc<Mutex<NodeState>>) -> io::Result<()> {
    thread::spawn(move || {
        loop {
            let round_secs = state.lock().unwrap().round_secs;
            thread::sleep(Duration::from_secs(round_secs));
        }
    });
    Ok(())
}

pub fn build_members(state: &NodeState) -> Vec<String> {
    let mut members = Vec::with_capacity(state.peers.len() + 1);
    members.push(state.addr.clone());
    members.extend(state.peers.iter().cloned());
    members.retain(|peer| peer == &state.addr || !state.blocked_peers.contains(peer));
    members.sort();
    members.dedup();
    members
}

pub fn expected_leader(members: &[String], round: u64) -> Option<String> {
    if members.is_empty() {
        return None;
    }

    Some(members[round as usize % members.len()].clone())
}

pub fn quorum(member_count: usize) -> usize {
    member_count / 2 + 1
}

pub fn build_proposal(state: &NodeState, round: u64) -> Proposal {
    let pending = state
        .tx_pool
        .values()
        .filter(|tx| !state.ledger_ids.contains(&tx.id))
        .cloned()
        .collect();

    Proposal {
        addr: state.addr.clone(),
        round,
        ledger_len: state.ledger.len(),
        ledger_hash: state.ledger_hash.clone(),
        pending,
    }
}

pub fn build_commit(payload: CommitPayload) -> Commit {
    let commit_hash = commit_hash(&payload).expect("commit payload serialization should not fail");
    Commit {
        payload,
        commit_hash,
    }
}

pub fn validate_transaction(tx: &Transaction) -> Result<(), String> {
    ledger::validate_transaction(tx)
}

pub fn validate_commit_for_state(state: &NodeState, commit: &Commit) -> Result<(), String> {
    let expected_hash = commit_hash(&commit.payload)?;
    if commit.commit_hash != expected_hash {
        return Err("commit hash does not match payload".to_string());
    }

    let round = commit.payload.round;
    if round < state.next_round {
        let existing = state
            .commits
            .get(round as usize)
            .ok_or_else(|| "commit round is before next_round but missing locally".to_string())?;
        if existing.commit_hash == commit.commit_hash {
            return Ok(());
        }
        return Err("commit conflicts with already committed round".to_string());
    }

    if round != state.next_round {
        return Err(format!(
            "commit round {} does not match next round {}",
            round, state.next_round
        ));
    }

    if commit.payload.prev_ledger_hash != state.ledger_hash {
        return Err("ledger hash mismatch".to_string());
    }

    if !is_sorted_unique(&commit.payload.members) {
        return Err("members must be sorted and unique".to_string());
    }

    if commit.payload.members.is_empty() {
        return Err("members cannot be empty".to_string());
    }

    if !commit.payload.members.contains(&commit.payload.leader) {
        return Err("leader is not in members".to_string());
    }

    let expected = expected_leader(&commit.payload.members, round)
        .ok_or_else(|| "cannot select leader from empty members".to_string())?;
    if commit.payload.leader != expected {
        return Err("leader does not match round membership".to_string());
    }

    if !is_unique(&commit.payload.votes) {
        return Err("votes must be unique".to_string());
    }

    for vote in &commit.payload.votes {
        if !commit.payload.members.contains(vote) {
            return Err("vote is not in members".to_string());
        }
    }

    if commit.payload.votes.len() < quorum(commit.payload.members.len()) {
        return Err("commit does not have quorum".to_string());
    }

    let mut tx_ids = HashSet::new();
    for tx in &commit.payload.txs {
        validate_transaction(tx)?;

        if !tx_ids.insert(tx.id.clone()) {
            return Err("commit contains duplicate transaction".to_string());
        }

        if state.ledger_ids.contains(&tx.id) {
            return Err("transaction is already committed".to_string());
        }
    }

    Ok(())
}

pub fn apply_commit(state: &mut NodeState, commit: Commit) -> Result<(), String> {
    validate_commit_for_state(state, &commit)?;

    if commit.payload.round < state.next_round {
        return Ok(());
    }

    for tx in &commit.payload.txs {
        state.tx_pool.shift_remove(&tx.id);
        state.ledger_ids.insert(tx.id.clone());
        state.ledger.push(tx.clone());
    }

    state.ledger_hash = commit.commit_hash.clone();
    state.next_round += 1;
    state.commits.push(commit);

    Ok(())
}

pub fn sort_transactions(txs: &mut [Transaction]) {
    txs.sort_by(|a, b| {
        a.origin
            .cmp(&b.origin)
            .then(a.seq.cmp(&b.seq))
            .then(a.id.cmp(&b.id))
    });
}

fn is_sorted_unique(values: &[String]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn is_unique(values: &[String]) -> bool {
    let mut seen = HashSet::new();
    values.iter().all(|value| seen.insert(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crypto::transaction_id,
        models::{GENESIS_LEDGER_HASH, UnsignedTransaction},
    };

    fn state(addr: &str, peers: &[&str]) -> NodeState {
        let mut state = NodeState::new(addr.to_string(), "127.0.0.1:0".to_string());
        state.peers = peers.iter().map(|peer| peer.to_string()).collect();
        state
    }

    fn tx(origin: &str, seq: u64, body: &str) -> Transaction {
        let unsigned = UnsignedTransaction {
            origin: origin.to_string(),
            seq,
            body: body.to_string(),
        };
        Transaction {
            id: transaction_id(&unsigned).unwrap(),
            origin: unsigned.origin,
            seq: unsigned.seq,
            body: unsigned.body,
        }
    }

    fn valid_commit_for(state: &NodeState, txs: Vec<Transaction>) -> Commit {
        let members = build_members(state);
        let leader = expected_leader(&members, state.next_round).unwrap();
        build_commit(CommitPayload {
            round: state.next_round,
            prev_ledger_hash: state.ledger_hash.clone(),
            leader,
            members: members.clone(),
            votes: members,
            txs,
        })
    }

    #[test]
    fn selects_rotating_leader() {
        let members = vec![
            "127.0.0.1:9000".to_string(),
            "127.0.0.1:9001".to_string(),
            "127.0.0.1:9002".to_string(),
        ];

        assert_eq!(expected_leader(&members, 0).unwrap(), members[0]);
        assert_eq!(expected_leader(&members, 1).unwrap(), members[1]);
        assert_eq!(expected_leader(&members, 2).unwrap(), members[2]);
        assert_eq!(expected_leader(&members, 3).unwrap(), members[0]);
    }

    #[test]
    fn calculates_majority_quorum() {
        assert_eq!(quorum(1), 1);
        assert_eq!(quorum(2), 2);
        assert_eq!(quorum(3), 2);
        assert_eq!(quorum(4), 3);
        assert_eq!(quorum(5), 3);
    }

    #[test]
    fn sorts_transactions_deterministically() {
        let mut txs = vec![
            tx("127.0.0.1:9002", 1, "b"),
            tx("127.0.0.1:9001", 2, "c"),
            tx("127.0.0.1:9001", 1, "a"),
        ];

        sort_transactions(&mut txs);

        let ordered: Vec<_> = txs
            .iter()
            .map(|tx| (tx.origin.as_str(), tx.seq, tx.body.as_str()))
            .collect();
        assert_eq!(
            ordered,
            vec![
                ("127.0.0.1:9001", 1, "a"),
                ("127.0.0.1:9001", 2, "c"),
                ("127.0.0.1:9002", 1, "b"),
            ]
        );
    }

    #[test]
    fn rejects_bad_commit_hash() {
        let state = state("127.0.0.1:9000", &["127.0.0.1:9001"]);
        let mut commit = valid_commit_for(&state, vec![tx("127.0.0.1:9000", 1, "a")]);
        commit.commit_hash = "bad".to_string();

        assert!(
            validate_commit_for_state(&state, &commit)
                .unwrap_err()
                .contains("hash")
        );
    }

    #[test]
    fn rejects_duplicate_transactions() {
        let state = state("127.0.0.1:9000", &["127.0.0.1:9001"]);
        let tx = tx("127.0.0.1:9000", 1, "a");
        let commit = valid_commit_for(&state, vec![tx.clone(), tx]);

        assert!(
            validate_commit_for_state(&state, &commit)
                .unwrap_err()
                .contains("duplicate")
        );
    }

    #[test]
    fn rejects_previous_hash_mismatch() {
        let mut state = state("127.0.0.1:9000", &["127.0.0.1:9001"]);
        state.ledger_hash = "local-tip".to_string();
        let mut commit = valid_commit_for(&state, vec![tx("127.0.0.1:9000", 1, "a")]);
        commit.payload.prev_ledger_hash = GENESIS_LEDGER_HASH.to_string();
        commit.commit_hash = commit_hash(&commit.payload).unwrap();

        assert!(
            validate_commit_for_state(&state, &commit)
                .unwrap_err()
                .contains("ledger hash mismatch")
        );
    }

    #[test]
    fn applies_commit_and_advances_state() {
        let mut state = state("127.0.0.1:9000", &["127.0.0.1:9001"]);
        let tx = tx("127.0.0.1:9000", 1, "a");
        state.tx_pool.insert(tx.id.clone(), tx.clone());
        let commit = valid_commit_for(&state, vec![tx.clone()]);
        let commit_hash = commit.commit_hash.clone();

        apply_commit(&mut state, commit).unwrap();

        assert_eq!(state.ledger, vec![tx]);
        assert_eq!(state.ledger_hash, commit_hash);
        assert_eq!(state.next_round, 1);
        assert_eq!(state.commits.len(), 1);
        assert!(state.tx_pool.is_empty());
    }
}
