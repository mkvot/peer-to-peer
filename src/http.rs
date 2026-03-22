use std::{
    io::{Result, Write},
    net::TcpStream,
};

pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: Vec<String>,
    pub body: String,
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<String>,
    pub body: String,
}

pub fn parse_request(buf: &[u8]) -> Request {
    let request = String::from_utf8_lossy(&buf);
    let (head, body) = request.split_once("\r\n\r\n").unwrap_or(("", ""));

    let mut lines = head.split("\r\n");
    let mut request_line = lines.next().unwrap_or("").split_whitespace();
    let method = request_line.next().unwrap_or("").to_string();
    let path = request_line.next().unwrap_or("").to_string();
    let headers: Vec<String> = lines.map(|x| x.to_string()).collect();

    Request {
        method,
        path,
        headers,
        body: body.to_string(),
    }
}

pub fn parse_response(buf: &[u8]) -> Response {
    let request = String::from_utf8_lossy(&buf);
    let (head, body) = request.split_once("\r\n\r\n").unwrap_or(("", ""));

    let mut lines = head.split("\r\n");
    
    let status = lines.next().unwrap_or("")
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let headers: Vec<String> = lines.map(|x| x.to_string()).collect();

    Response {
        status,
        headers,
        body: body.to_string(),
    }
}

pub fn reply(mut stream: TcpStream, body: String) -> Result<()> {
    let response = format!(
        "HTTP/1.1 200 OK\r\n\
    Content-Type: application/json\r\n\
    Content-Length: {}\r\n\
    \r\n\
    {}",
        body.len(),
        body
    );

    stream.write_all(response.as_bytes())?;
    Ok(())
}