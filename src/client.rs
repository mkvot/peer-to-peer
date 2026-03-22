use std::{
    io::{Error, ErrorKind, Read, Result, Write},
    net::TcpStream, sync::{Arc, Mutex}, thread, time::Duration,
};

use crate::http::parse_response;

pub fn start(state: Arc<Mutex<Vec<String>>>, my_addr: &str) -> Result<()> {
    let peers = state.lock().unwrap().clone();
    if peers.is_empty() {
        println!("No peers, waiting for incoming connections");
    } else {
        for peer in peers.iter() {
            if peer == &my_addr {
                continue;
            }
            match ping(peer) {
                Ok(_) => {
                    announce(peer, my_addr, &state)?;
                },
                Err(e) => println!("failed to reach {peer}, {e}"),
            }
        }
    }

    loop {
        println!("Waiting 5s");
        thread::sleep(Duration::from_secs(5));

        let peers = state.lock().unwrap().clone();
        for peer in peers.iter() {
            match ping(peer) {
                Ok(_) => {},
                Err(_) => state.lock().unwrap().retain(|p| p != peer),
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

fn announce(addr: &str, my_addr: &str, state: &Arc<Mutex<Vec<String>>>) -> Result<()> {
    let mut stream = TcpStream::connect(addr)?;
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

    let peers: Vec<String> = serde_json::from_str(&response.body)?;
    let mut guard = state.lock().unwrap();
    for peer in peers.iter() {
        if peer != &my_addr && !guard.contains(peer) {
            guard.push(peer.to_string());
        }
    }

    Ok(())
}
