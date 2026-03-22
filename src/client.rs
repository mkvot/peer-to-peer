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
            match ping(peer) {
                Ok(_) => {
                    announce(peer, &state)?;
                },
                Err(e) => println!("failed to reach {peer}, {e}"),
            }
        }
    }

    loop {
        println!("~~~");
        thread::sleep(Duration::from_secs(5));

        let peers = state.lock().unwrap().peers.clone();
        for peer in peers.iter() {
            if peer == &addr { continue; }
            match ping(peer) {
                Ok(_) => { announce(peer, &state)?; }
                Err(_) => {
                    println!("peer {peer} is dead, removing");
                    state.lock().unwrap().peers.retain(|p| p != peer);
                }
            }
        }
    }
}

fn ping(addr: &str) -> Result<()> {
    let mut stream = TcpStream::connect(addr)?;
    stream.write_all("GET /ping HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n".as_bytes())?;
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf)?;

    let response = parse_response(&buf[..n]);
    
    match response.status {
        200 => Ok(()),
        _ => Err(Error::new(ErrorKind::Other, format!("failed to ping {addr}"))),
    }
}

fn announce(addr: &str, state: &Arc<Mutex<NodeState>>) -> Result<()> {
    println!("announcing to {addr}");
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

    println!("announce response status: {}", response.status);
    println!("announce response body: {}", response.body);

    Ok(())
}
