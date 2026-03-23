use std::{
    io::{Result, Write},
    net::TcpStream,
};

pub struct Request {
    pub method: String,
    pub path: String,
    pub version: String,
    pub headers: Vec<String>,
    pub body: String,
}

impl Request {
    pub fn new(method: &str, path: &str, my_addr: &str, addr: &str) -> Self {
        Self {
            method: method.to_string(),
            path: path.to_string(),
            headers: vec![
                format!("Host: {addr}"),
                format!("X-Node-Addr: {my_addr}"),
            ],
            body: String::new(),
            ..Default::default()
        }
    }

    pub fn with_body(mut self, body: String) -> Self {
        if !body.is_empty() {
            self.headers.push(format!("Content-Length: {}", body.len()));
            self.body = body;
        }
        self
    }

    pub fn get(path: &str, my_addr: &str, addr: &str) -> Self {
        Self::new("GET", path, my_addr, addr)
    }

    pub fn post(path: &str, my_addr: &str, addr: &str, body: String) -> Self {
        Self::new("POST", path, my_addr, addr).with_body(body)
    }

    pub fn node_addr(&self) -> Option<&str> {
        self.headers.iter()
            .find(|h| h.to_lowercase().starts_with("x-node-addr:"))
            .map(|h| h["x-node-addr:".len()..].trim())
    }
}

impl Default for Request {
    fn default() -> Self {
        Self {
            method: "GET".to_string(),
            path: "/".to_string(),
            version: "HTTP/1.1".to_string(),
            headers: Vec::new(),
            body: String::new(),
        }
    }
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
    let version = request_line.next().unwrap_or("").to_string();
    let headers: Vec<String> = lines.map(|x| x.to_string()).collect();
    
    Request {
        method,
        path,
        version,
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

pub fn reply(mut stream: TcpStream, status: u16, body: String) -> Result<()> {
    let status_text = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    };

    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );

    stream.write_all(response.as_bytes())?;
    Ok(())
}