use std::{io::{Read, Result, Write}, net::TcpStream};

pub fn start(msg: String) -> Result<()> {
    let addr = "127.0.0.1:8080";
    let mut stream = TcpStream::connect(addr)?;
    let msg = "GET /ping HTTP/1.1";

    stream.write_all(msg.as_bytes())?;
    println!("Sent message to {addr}");

    let mut buf = [0u8; 512];
    let n = stream.read(&mut buf).unwrap();
    println!("Received {n} bytes:\n {}", String::from_utf8_lossy(&buf[..n]));
    Ok(())
}