use std::{io::{Result, Write}, net::TcpStream};

pub fn start(msg: String) -> Result<()> {
    let addr = "127.0.0.1:8080";
    let mut stream = TcpStream::connect(addr)?;

    stream.write_all(msg.as_bytes())?;
    println!("Sent message to {addr}");
    Ok(())
}