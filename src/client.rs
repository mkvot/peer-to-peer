use std::{
    io::{Error, ErrorKind, Read, Result, Write},
    net::TcpStream, sync::{Arc, Mutex}, thread, time::Duration,
};

use crate::{http::parse_response, state::NodeState};

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

fn ping(addr: &str, state: &Arc<Mutex<NodeState>>) -> Result<()> {
    let mut stream = TcpStream::connect(addr)?;
    let my_addr = state.lock().unwrap().addr.clone();
    let request = format!("GET /ping HTTP/1.1\r\nHost: {addr}\r\nX-Node-Addr: {my_addr}\r\n\r\n").to_string();
    stream.write_all(request.as_bytes())?;
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf)?;

    let response = parse_response(&buf[..n]);
    
    match response.status {
        200 => Ok(()),
        _ => Err(Error::new(ErrorKind::Other, format!("failed to ping {addr}"))),
    }
}

fn announce(addr: &str, state: &Arc<Mutex<NodeState>>) -> Result<()> {
    let mut stream = TcpStream::connect(addr)?;
    let my_addr = state.lock().unwrap().addr.clone();
    let body = format!(r#"{{"address": "{}"}}"#, my_addr);
    let request = format!(
        "POST /peers/announce HTTP/1.1\r\nHost: {addr}\r\nContent-Length: {}\r\n\r\n{}",
        body.len(), body
    );
    stream.write_all(request.as_bytes())?;


    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf)?;

    let response = parse_response(&buf[..n]);
    
    match response.status {
        200 => {},
        _ => {},
    }

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
    let mut stream = TcpStream::connect(addr)?;
    stream.write_all(format!("GET /addr HTTP/1.1\r\nHost: {addr}\r\n\r\n").as_bytes())?;

    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf)?;
    let response = parse_response(&buf[..n]);
    
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

pub fn forward_block(addr: &str, body: &str) -> Result<()> {
    let mut stream = TcpStream::connect(addr)?;
    let request = format!(
        "POST /block HTTP/1.1\r\nHost: {addr}\r\nContent-Length: {}\r\n\r\n{}",
        body.len(), body
    );
    stream.write_all(request.as_bytes())?;
    
    let mut buf = [0u8; 4096];
    stream.read(&mut buf)?;
    Ok(())
}
