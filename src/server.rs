use crate::http::read_request;
use crate::routes::{
    handle_addr, handle_announce, handle_get_blocks, handle_get_blocks_from,
    handle_get_consensus_commits, handle_get_consensus_proposal, handle_get_data,
    handle_get_ledger, handle_ledger_status, handle_not_found, handle_options, handle_ping,
    handle_post_block, handle_post_consensus_commit, handle_post_inv, handle_post_tx,
    handle_status,
};
use crate::state::NodeState;
use std::sync::{Arc, Mutex};
use std::thread;
use std::{
    io::Result,
    net::{TcpListener, TcpStream},
};

fn handle_client(mut stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let request = read_request(&mut stream)?;

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/ping") => handle_ping(stream, request),
        ("GET", "/addr") => handle_addr(stream, state),
        ("POST", "/peers/announce") => handle_announce(stream, state, request.body),
        ("GET", "/getblocks") => handle_get_blocks(stream, state),
        ("GET", path) if path.starts_with("/getdata/") => {
            let hash = path.trim_start_matches("/getdata/");
            handle_get_data(stream, state, hash)
        }
        ("POST", "/block") => handle_post_block(stream, state, request.body),
        ("GET", path) if path.starts_with("/getblocks/") => {
            let hash = path.trim_start_matches("/getblocks/");
            handle_get_blocks_from(stream, state, hash)
        }
        ("POST", "/inv") => handle_post_inv(stream, state, request.body),
        ("POST", "/tx") => handle_post_tx(stream, state, request.body),
        ("GET", "/ledger") => handle_get_ledger(stream, state),
        ("GET", "/ledger/status") => handle_ledger_status(stream, state),
        ("GET", path) if path.starts_with("/consensus/proposal/") => {
            let round = path
                .trim_start_matches("/consensus/proposal/")
                .parse()
                .unwrap_or(0);
            handle_get_consensus_proposal(stream, state, round)
        }
        ("POST", "/consensus/commit") => handle_post_consensus_commit(stream, state, request.body),
        ("GET", path) if path.starts_with("/consensus/commits/") => {
            let from_round = path
                .trim_start_matches("/consensus/commits/")
                .parse()
                .unwrap_or(0);
            handle_get_consensus_commits(stream, state, from_round)
        }
        ("GET", "/status") => handle_status(stream, state),
        ("OPTIONS", _) => handle_options(stream),
        _ => handle_not_found(stream),
    }
}

pub fn start(state: Arc<Mutex<NodeState>>) -> Result<()> {
    let addr = state.lock().unwrap().bind_addr.clone();
    let listener = TcpListener::bind(addr)?;

    for stream in listener.incoming() {
        let node = state.clone();
        thread::spawn(move || {
            if let Err(e) = handle_client(stream.unwrap(), node) {
                println!("Error handling client: {}", e);
            }
        });
    }

    Ok(())
}
