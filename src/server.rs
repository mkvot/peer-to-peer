use std::{io::{Read}, net::{TcpListener, TcpStream}};

fn handle_client(mut stream: TcpStream) {
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).unwrap();
    println!("Received {} bytes from {}:", n, stream.local_addr().unwrap());
    println!("{}", String::from_utf8_lossy(&buf[..n]));
}

pub fn start() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080")?;

    for stream in listener.incoming() {
        handle_client(stream?);
    }
    Ok(())
}