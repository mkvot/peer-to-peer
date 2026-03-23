use std::{
    io::{Error, ErrorKind, Read, Result, Write},
    net::TcpStream, sync::{Arc, Mutex}, thread, time::Duration,
};

use crate::{http::{Request, Response, parse_response}, state::NodeState};

pub fn start(state: Arc<Mutex<NodeState>>) -> Result<()> {
    let node = state.lock().unwrap().clone();
    let addr = node.addr.clone();
    let peers = node.peers.clone();

    if peers.is_empty() {
        println!("waiting for connections");
    } else {
        for peer in peers.iter() {
            if peer == &node.addr { continue };
            match ping(peer, &state) {
                Ok(_) => {
                    announce(peer, &state)?;
                },
                Err(e) => println!("failed to reach {peer}, {e}"),
            }
        }
    }

    for peer in state.lock().unwrap().peers.clone().iter() {
        if let Err(e) = sync_blocks(peer, &state) {
            println!("failed to sync blocks from {peer}: {e}");
        }
    }

    let mut tick = 0u32;

    loop {
        thread::sleep(Duration::from_secs(3));
        println!("~~~");
        tick += 1;

        let peers = state.lock().unwrap().peers.clone();
        println!("~~~ known peers: {:?}", peers);
        for peer in peers.iter() {
            if peer == &addr { continue; }
            match ping(peer, &state) {
                Ok(_) => {
                    if tick % 3 == 0 {
                        if let Err(e) = sync_peers(peer, &state) {
                            println!("failed to sync peers with {peer}: {e}");
                        }
                    }
                }
                Err(_) => {
                    println!("{peer} is dead, removing");
                    state.lock().unwrap().peers.retain(|p| p != peer);
                }
            }
        }
    }
}

fn send_request(addr: &str, request: Request) -> Result<Response> {
    let mut stream = TcpStream::connect(addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;

    let headers = if request.headers.is_empty() {
        String::new()
    } else {
        format!("{}\r\n", request.headers.join("\r\n"))
    };

    let msg = format!(
        "{} {} {}\r\n{}\r\n\r\n{}",
        request.method, 
        request.path, 
        request.version, 
        headers, 
        request.body
    );

    stream.write_all(msg.as_bytes())?;
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf)?;

    Ok(parse_response(&buf[..n]))
}

fn ping(addr: &str, state: &Arc<Mutex<NodeState>>) -> Result<()> {
    let my_addr = state.lock().unwrap().addr.clone();

    let request = Request::get("/ping", &my_addr, addr);
    let response = send_request(addr, request)?;
    
    match response.status {
        200 => Ok(()),
        _ => Err(Error::new(ErrorKind::Other, format!("failed to ping {addr}"))),
    }
}

fn announce(addr: &str, state: &Arc<Mutex<NodeState>>) -> Result<()> {
    let my_addr = state.lock().unwrap().addr.clone();
    let body = format!(r#"{{"address": "{}"}}"#, my_addr);

    let request = Request::post("/peers/announce", &my_addr, addr, body);
    let response = send_request(addr, request)?;
    
    let new_peers: Vec<String> = serde_json::from_str(&response.body)?;
    let mut guard = state.lock().unwrap();

    for peer in new_peers.iter() {
        if peer != &my_addr && !guard.peers.contains(peer) {
            guard.peers.push(peer.to_string());
        }
    }

    Ok(())
}

fn sync_peers(addr: &str, state: &Arc<Mutex<NodeState>>) -> Result<()> {
    let my_addr = state.lock().unwrap().addr.clone();

    let request = Request::get("/addr", &my_addr, addr);
    let response = send_request(addr, request)?;
    
    let new_peers: Vec<String> = match serde_json::from_str(&response.body) {
        Ok(p) => p,
        Err(e) => {
            println!("failed to parse peers from {addr}: {e}, body: {}", response.body);
            return Ok(());
        }
    };

    let mut newly_discovered = vec![];
    {
        let mut guard = state.lock().unwrap();
        for peer in new_peers {
            if peer != guard.addr && !guard.peers.contains(&peer) {
                println!("discovered new peer {peer}");
                guard.peers.push(peer.clone());
                newly_discovered.push(peer);
            }
        }
    }

    for peer in newly_discovered {
        if let Err(e) = announce(&peer, state) {
            println!("failed to announce to new peer {peer}: {e}");
        }
    }

    Ok(())
}

pub fn forward_block(addr: &str, body: &str, state: &Arc<Mutex<NodeState>>) -> Result<()> {
    let my_addr = state.lock().unwrap().addr.clone();

    let request = Request::post("/block", &my_addr, addr, body.to_string());
    
    match send_request(addr, request) {
        Ok(response) => {
            if response.status == 200 {
                println!("Successfully forwarded block to {}", addr);
                Ok(())
            } else {
                println!("Node {} returned error: {}", addr, response.status);
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Node rejected block with status {}", response.status)
                ))
            }
        }
        Err(e) => {
            println!("Failed to reach node {}: {}", addr, e);
            Err(e) 
        }
    }
}

fn sync_blocks(addr: &str, state: &Arc<Mutex<NodeState>>) -> Result<()> {
    let my_addr = state.lock().unwrap().addr.clone();

    let request = Request::get("/getblocks", &my_addr, addr);
    let response = send_request(addr, request)?;

    let hashes: Vec<String> = match serde_json::from_str(&response.body) {
        Ok(h) => h,
        Err(_) => return Ok(()),
    };

    for hash in hashes {
        if state.lock().unwrap().blocks.contains_key(&hash) {
            continue;
        }
        get_block(addr, state, hash)?;
    }
    Ok(())
}

fn get_block(addr:&str, state: &Arc<Mutex<NodeState>>, hash: String) -> Result<()>{
    let my_addr = state.lock().unwrap().addr.clone();

    let request = Request::get(&format!("/getdata/{hash}"), &my_addr, addr);
    let response = send_request(addr, request)?;

    if response.status == 200 {
        println!("synced block {hash} from {addr}");
        state.lock().unwrap().blocks.insert(hash, response.body);
        Ok(())
    } else {
        println!("failed to sync block {hash} from {addr}");
        Ok(())
    }
}