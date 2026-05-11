mod client;
mod consensus;
mod crypto;
mod http;
mod ledger;
mod models;
mod routes;
mod server;
mod state;
mod storage;

use crate::state::{NodeConfig, NodeState};
use crate::storage::init_storage;
use std::{
    env, fs,
    io::Result,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};

struct CliOptions {
    port: String,
    advertise_ip: String,
    bind_ip: String,
    peers: Vec<String>,
    config: NodeConfig,
}

fn main() -> Result<()> {
    let options = parse_args();

    let bind_addr = format!("{}:{}", options.bind_ip, options.port);
    let announce_addr = format!("{}:{}", options.advertise_ip, options.port);
    let mut node_state = NodeState::with_config(announce_addr, bind_addr, options.config);
    init_storage(&mut node_state)?;
    let state: Arc<Mutex<NodeState>> = Arc::new(Mutex::new(node_state));

    if !options.peers.is_empty() {
        let mut state = state.lock().unwrap();
        state.peers = options.peers;
        for peer in state.peers.iter() {
            println!("  {peer}");
        }
    }

    let client_state = state.clone();
    thread::spawn(move || {
        client::start(client_state).unwrap();
    });

    if state.lock().unwrap().consensus_enabled {
        consensus::start_consensus_loop(state.clone())?;
    }

    let server_state = state.clone();
    server::start(server_state)?;
    Ok(())
}

fn parse_args() -> CliOptions {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_usage();
        std::process::exit(if args.is_empty() { 1 } else { 0 });
    }

    let port = args[0].clone();
    let mut advertise_ip = "127.0.0.1".to_string();
    let mut bind_ip = "0.0.0.0".to_string();
    let mut peers = Vec::new();
    let mut config = NodeConfig::default();
    let mut legacy_positionals = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--peer" => {
                let value = required_value(&args, i, "--peer");
                push_peer_values(&mut peers, value, &port);
                i += 2;
            }
            "--peers" => {
                let value = required_value(&args, i, "--peers");
                push_peer_values(&mut peers, value, &port);
                i += 2;
            }
            "--peers-file" => {
                let value = required_value(&args, i, "--peers-file");
                peers.extend(load_peers_file(value, &port));
                i += 2;
            }
            "--advertise-ip" | "--ip" => {
                advertise_ip = required_value(&args, i, args[i].as_str()).to_string();
                i += 2;
            }
            "--bind-ip" => {
                bind_ip = required_value(&args, i, "--bind-ip").to_string();
                i += 2;
            }
            "--data-dir" => {
                config.data_dir_base = PathBuf::from(required_value(&args, i, "--data-dir"));
                i += 2;
            }
            "--round-secs" => {
                config.round_secs = required_value(&args, i, "--round-secs")
                    .parse()
                    .unwrap_or_else(|_| {
                        eprintln!("--round-secs must be a positive integer");
                        std::process::exit(2);
                    });
                i += 2;
            }
            "--consensus" => {
                config.consensus_enabled = true;
                i += 1;
            }
            "--no-consensus" => {
                config.consensus_enabled = false;
                i += 1;
            }
            "--forward-inv" => {
                config.forward_inv_enabled = true;
                i += 1;
            }
            "--no-forward-inv" => {
                config.forward_inv_enabled = false;
                i += 1;
            }
            value if value.starts_with("--") => {
                eprintln!("unknown option: {value}");
                print_usage();
                std::process::exit(2);
            }
            value => {
                legacy_positionals.push(value.to_string());
                i += 1;
            }
        }
    }

    if let Some(path) = legacy_positionals.first() {
        peers.extend(load_peers_file(path, &port));
    }
    if let Some(ip) = legacy_positionals.get(1) {
        advertise_ip = ip.clone();
    }

    peers.sort();
    peers.dedup();

    CliOptions {
        port,
        advertise_ip,
        bind_ip,
        peers,
        config,
    }
}

fn required_value<'a>(args: &'a [String], index: usize, name: &str) -> &'a str {
    args.get(index + 1).map(String::as_str).unwrap_or_else(|| {
        eprintln!("{name} needs a value");
        std::process::exit(2);
    })
}

fn push_peer_values(peers: &mut Vec<String>, value: &str, default_port: &str) {
    for peer in value.split(',') {
        let peer = normalize_peer(peer, default_port);
        if !peer.is_empty() {
            peers.push(peer);
        }
    }
}

fn load_peers_file(path: &str, default_port: &str) -> Vec<String> {
    let json = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("failed to read peers file {path}: {e}");
        std::process::exit(2);
    });
    let peer_info: Vec<String> = serde_json::from_str(&json).unwrap_or_else(|e| {
        eprintln!("failed to parse peers file {path}: {e}");
        std::process::exit(2);
    });

    peer_info
        .iter()
        .map(|peer| normalize_peer(peer, default_port))
        .filter(|peer| !peer.is_empty())
        .collect()
}

fn normalize_peer(peer: &str, default_port: &str) -> String {
    let peer = peer
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/');
    if peer.is_empty() {
        return String::new();
    }
    if peer.contains(':') {
        peer.to_string()
    } else {
        format!("{peer}:{default_port}")
    }
}

fn print_usage() {
    println!(
        "Usage: peer-to-peer <port> [options]\n\
\n\
Options:\n\
  --peer <addr-or-ip>       Add one peer. If no port is given, this node's port is used.\n\
  --peers <a,b,c>           Add comma-separated peers.\n\
  --peers-file <path>       Load peers from a JSON array.\n\
  --advertise-ip <ip>       Address other machines should use for this node. Default: 127.0.0.1.\n\
  --bind-ip <ip>            Local bind address. Default: 0.0.0.0.\n\
  --data-dir <path>         Base directory for per-node ledgers. Default: data.\n\
  --round-secs <seconds>    Consensus tick interval. Default: 2.\n\
  --no-consensus            Append transactions locally without consensus.\n\
  --forward-inv             Gossip transactions directly between peers.\n\
\n\
Examples:\n\
  peer-to-peer 9000 --data-dir /tmp/p2p-demo\n\
  peer-to-peer 9001 --peer 127.0.0.1:9000 --data-dir /tmp/p2p-demo\n\
  peer-to-peer 9000 --advertise-ip 192.168.1.10 --peer 192.168.1.20\n"
    );
}
