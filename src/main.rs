mod block;
mod chain;
mod client;
mod config;
mod crypto;
mod http;
mod mining;
mod models;
mod peers;
mod routes;
mod server;
mod state;
mod storage;
mod transaction;
mod wallet;

use std::{
    env,
    io::Result,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};

use crate::{
    config::NodeConfig,
    peers::{push_peer_values, sort_dedup},
    state::NodeState,
};

struct CliOptions {
    port: String,
    bind_ip: String,
    peers: Vec<String>,
    config: NodeConfig,
}

fn main() -> Result<()> {
    let options = parse_args();
    let bind_addr = format!("{}:{}", options.bind_ip, options.port);
    let node_addr = bind_addr.clone();

    let mut node_state = NodeState::new(node_addr, bind_addr, options.config)?;
    node_state.peers = options.peers;
    storage::init_storage(&mut node_state)?;

    println!(
        "[node] addr={} difficulty={} data_dir={}",
        node_state.addr,
        node_state.config.difficulty,
        node_state.data_dir.display()
    );

    let state: Arc<Mutex<NodeState>> = Arc::new(Mutex::new(node_state));

    let client_state = state.clone();
    thread::spawn(move || {
        if let Err(error) = client::start(client_state) {
            eprintln!("client loop failed: {error}");
        }
    });

    server::start(state)
}

fn parse_args() -> CliOptions {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_usage();
        std::process::exit(if args.is_empty() { 1 } else { 0 });
    }

    let port = args[0].clone();
    let mut bind_ip = "127.0.0.1".to_string();
    let mut peers = Vec::new();
    let mut config = NodeConfig::default();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--peers" => {
                let value = required_value(&args, i, "--peers");
                push_peer_values(&mut peers, value, &port);
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
            "--difficulty" => {
                config.difficulty = required_value(&args, i, "--difficulty")
                    .parse()
                    .unwrap_or_else(|_| {
                        eprintln!("--difficulty must be a non-negative integer");
                        std::process::exit(2);
                    });
                i += 2;
            }
            value if value.starts_with("--") => {
                eprintln!("unknown option: {value}");
                print_usage();
                std::process::exit(2);
            }
            value => {
                eprintln!("unexpected positional argument: {value}");
                print_usage();
                std::process::exit(2);
            }
        }
    }

    sort_dedup(&mut peers);
    CliOptions {
        port,
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

fn print_usage() {
    println!(
        "Usage: peer-to-peer <port> [options]\n\
\n\
Options:\n\
  --peers <addr,addr>       Add one or more peers.\n\
  --bind-ip <ip>            Local bind address. Default: 127.0.0.1.\n\
  --data-dir <path>         Base directory for per-node data. Default: ledger_data.\n\
  --difficulty <n>          Leading-zero proof-of-work difficulty. Default: 4.\n\
\n\
Examples:\n\
  peer-to-peer 9000 --data-dir ledger_data --difficulty 4\n\
  peer-to-peer 9001 --peers 127.0.0.1:9000 --data-dir ledger_data\n"
    );
}
