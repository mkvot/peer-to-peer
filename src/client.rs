use std::{
    io::{Read, Result, Write},
    net::TcpStream,
};

pub fn start(msg: Option<String>) -> Result<()> {
    let addr = "127.0.0.1:8080";
    let mut stream = TcpStream::connect(addr)?;
    let request = match msg.as_deref() {
        Some("peers") => "GET /peers HTTP/1.1\r\nHost:127.0.0.1:8082\r\n\r\n".to_string(),
        Some("ping") | None => "GET /ping HTTP/1.1\r\nHost:127.0.0.1:8082\r\n\r\n".to_string(),
        Some(msg) => msg.to_string(),
    };

    stream.write_all(request.as_bytes())?;
    println!("Sent message to {addr}");

    let mut buf = [0u8; 512];
    let n = stream.read(&mut buf).unwrap();
    println!(
        "Received {n} bytes:\n {}",
        String::from_utf8_lossy(&buf[..n])
    );
    Ok(())
}
