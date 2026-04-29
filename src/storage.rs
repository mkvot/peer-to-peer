use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
};

use crate::{
    crypto::commit_hash,
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

    for commit in commits {
        validate_stored_commit(state, &commit)?;

        for tx in &commit.payload.txs {
            if state.ledger_ids.insert(tx.id.clone()) {
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
    let expected_hash = commit_hash(&commit.payload).map_err(invalid_data)?;
    if commit.commit_hash != expected_hash {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "stored commit hash does not match payload",
        ));
    }

    if commit.payload.prev_ledger_hash != state.ledger_hash {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "stored commit does not extend current ledger hash",
        ));
    }

    if commit.payload.round != state.next_round {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "stored commit round is out of sequence",
        ));
    }

    Ok(())
}

fn invalid_data(error: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}
