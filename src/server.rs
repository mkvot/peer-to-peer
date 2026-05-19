use std::{
    io::Result,
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
};

use crate::{
    http::read_request,
    routes::{
        handle_balances, handle_chain, handle_chain_status, handle_create_transaction,
        handle_debug_faults, handle_events, handle_get_block, handle_get_peers,
        handle_get_transactions, handle_hashes, handle_mine, handle_mining_status,
        handle_not_found, handle_options, handle_ping, handle_post_block, handle_post_peers,
        handle_post_transaction, handle_status, handle_wallet,
    },
    state::NodeState,
};

fn handle_client(mut stream: TcpStream, state: Arc<Mutex<NodeState>>) -> Result<()> {
    let request = read_request(&mut stream)?;
    let path = request
        .path
        .split_once('?')
        .map_or(request.path.as_str(), |(path, _)| path);

    match (request.method.as_str(), path) {
        ("GET", "/ping") => handle_ping(stream),
        ("GET", "/status") => handle_status(stream, state),
        ("GET", "/peers") => handle_get_peers(stream, state),
        ("POST", "/peers") => handle_post_peers(stream, state, request),
        ("GET", "/wallet") => handle_wallet(stream, state),
        ("POST", "/transactions/create") => handle_create_transaction(stream, state, request.body),
        ("POST", "/transactions") => handle_post_transaction(stream, state, request.body),
        ("GET", "/transactions") => handle_get_transactions(stream, state),
        ("POST", "/mine") => handle_mine(stream, state, request.body),
        ("GET", "/mining/status") => handle_mining_status(stream, state),
        ("POST", "/blocks") => handle_post_block(stream, state, request.body),
        ("GET", path) if path.starts_with("/blocks/") => {
            let hash = path.trim_start_matches("/blocks/");
            handle_get_block(stream, state, hash)
        }
        ("GET", "/hashes") => handle_hashes(stream, state),
        ("GET", "/chain") => handle_chain(stream, state),
        ("GET", "/chain/status") => handle_chain_status(stream, state),
        ("GET", "/balances") => handle_balances(stream, state),
        ("GET", "/events") => handle_events(stream, state),
        ("POST", "/debug/faults") => handle_debug_faults(stream, state, request.body),
        ("OPTIONS", _) => handle_options(stream),
        _ => handle_not_found(stream),
    }
}

pub fn start(state: Arc<Mutex<NodeState>>) -> Result<()> {
    let addr = state.lock().unwrap().bind_addr.clone();
    let listener = TcpListener::bind(addr)?;

    for stream in listener.incoming() {
        let node = state.clone();
        thread::spawn(move || match stream {
            Ok(stream) => {
                if let Err(error) = handle_client(stream, node) {
                    eprintln!("request failed: {error}");
                }
            }
            Err(error) => eprintln!("connection failed: {error}"),
        });
    }

    Ok(())
}
