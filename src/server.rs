use std::{io::{Read, Result, Write}, net::{TcpListener, TcpStream}, str::{self, from_utf8}};

fn handle_client(mut stream: TcpStream) -> Result<()> {
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).unwrap();
    println!("Received {} bytes from {}:", n, stream.local_addr().unwrap());
    println!("{}", String::from_utf8_lossy(&buf[..n]));

    let first_line = from_utf8(&buf[..n]).unwrap().lines().next().unwrap_or("");

    match first_line {
        "GET /ping HTTP/1.1" => reply(stream),
        _ => Ok(()),
    }
}

fn reply(mut stream: TcpStream) -> Result<()> {
    let body = r#"{"status": "ok"}"#;
    let response = format!("HTTP/1.1 200 OK\r\n\
    Content-Type: application/json\r\n\
    Content-Length: {}\r\n\
    \r\n\
    {}",
    body.len(), body);

    stream.write_all(response.as_bytes())?;
    Ok(())
}

pub fn start() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080")?;

    for stream in listener.incoming() {
        handle_client(stream?)?;
    }
    Ok(())
}