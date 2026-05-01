use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
};

use crate::{
    consensus::validate_commit_for_state,
    models::{Commit, GENESIS_LEDGER_HASH},
    state::NodeState,
};

pub fn init_storage(state: &mut NodeState) -> io::Result<()> {
    fs::create_dir_all(&state.data_dir)?;
    load_commits(state)
}

pub fn persist_commit(state: &NodeState, commit: &Commit) -> io::Result<()> {
    fs::create_dir_all(&state.data_dir)?;

    let path = state.data_dir.join("commits.jsonl");
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(commit).map_err(invalid_data)?;
    writeln!(file, "{line}")?;

    write_ledger(state)
}

pub fn write_ledger(state: &NodeState) -> io::Result<()> {
    fs::create_dir_all(&state.data_dir)?;

    let path = state.data_dir.join("ledger.json");
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, &state.ledger).map_err(invalid_data)
}

fn load_commits(state: &mut NodeState) -> io::Result<()> {
    let path = state.data_dir.join("commits.jsonl");
    if !path.exists() {
        return Ok(());
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut commits = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let commit: Commit = serde_json::from_str(&line).map_err(invalid_data)?;
        commits.push(commit);
    }

    rebuild_from_commits(state, commits)
}

fn rebuild_from_commits(state: &mut NodeState, commits: Vec<Commit>) -> io::Result<()> {
    state.ledger.clear();
    state.ledger_ids.clear();
    state.commits.clear();
    state.ledger_hash = GENESIS_LEDGER_HASH.to_string();
    state.next_round = 0;
    state.local_seq = 0;

    for commit in commits {
        validate_stored_commit(state, &commit)?;

        for tx in &commit.payload.txs {
            if state.ledger_ids.insert(tx.id.clone()) {
                if tx.origin == state.addr {
                    state.local_seq = state.local_seq.max(tx.seq);
                }
                state.ledger.push(tx.clone());
            }
        }

        state.ledger_hash = commit.commit_hash.clone();
        state.next_round += 1;
        state.commits.push(commit);
    }

    Ok(())
}

fn validate_stored_commit(state: &NodeState, commit: &Commit) -> io::Result<()> {
    validate_commit_for_state(state, commit).map_err(invalid_data)
}

fn invalid_data(error: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        consensus::build_commit,
        crypto::transaction_id,
        ledger::create_local_transaction,
        models::{CommitPayload, Transaction, UnsignedTransaction},
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_data_dir(name: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("p2p-storage-test-{name}-{suffix}"))
    }

    fn state_with_dir(name: &str) -> NodeState {
        let mut state = NodeState::new("127.0.0.1:9000".to_string(), "127.0.0.1:0".to_string());
        state.data_dir = temp_data_dir(name);
        state
    }

    fn tx(seq: u64, body: &str) -> Transaction {
        let unsigned = UnsignedTransaction {
            origin: "127.0.0.1:9000".to_string(),
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

    fn commit_with_txs(txs: Vec<Transaction>) -> Commit {
        build_commit(CommitPayload {
            round: 0,
            prev_ledger_hash: GENESIS_LEDGER_HASH.to_string(),
            leader: "127.0.0.1:9000".to_string(),
            members: vec!["127.0.0.1:9000".to_string()],
            votes: vec!["127.0.0.1:9000".to_string()],
            txs,
        })
    }

    fn write_commits_jsonl(state: &NodeState, commits: &[Commit]) {
        fs::create_dir_all(&state.data_dir).unwrap();
        let path = state.data_dir.join("commits.jsonl");
        let mut file = File::create(path).unwrap();
        for commit in commits {
            writeln!(file, "{}", serde_json::to_string(commit).unwrap()).unwrap();
        }
    }

    #[test]
    fn rejects_stored_commit_with_invalid_transaction_id() {
        let mut state = state_with_dir("bad-tx-id");
        let mut bad_tx = tx(1, "bad id");
        bad_tx.id = "not-the-real-id".to_string();
        let commit = commit_with_txs(vec![bad_tx]);
        write_commits_jsonl(&state, &[commit]);

        let err = init_storage(&mut state).unwrap_err();
        assert!(err.to_string().contains("transaction id"));
    }

    #[test]
    fn rejects_stored_commit_with_duplicate_transaction() {
        let mut state = state_with_dir("duplicate-tx");
        let tx = tx(1, "duplicate");
        let commit = commit_with_txs(vec![tx.clone(), tx]);
        write_commits_jsonl(&state, &[commit]);

        let err = init_storage(&mut state).unwrap_err();
        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn restores_local_sequence_from_committed_ledger() {
        let mut state = state_with_dir("local-seq");
        let commit = commit_with_txs(vec![tx(1, "first"), tx(2, "second")]);
        write_commits_jsonl(&state, &[commit]);

        init_storage(&mut state).unwrap();
        assert_eq!(state.local_seq, 2);

        let next = create_local_transaction(&mut state, "third".to_string()).unwrap();
        assert_eq!(next.seq, 3);
    }
}
