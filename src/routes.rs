use std::{io::Result, net::TcpStream};

use crate::http::reply;

pub fn handle_ping(stream: TcpStream) -> Result<()> {
    reply(stream, r#"{"status": "ok"}"#.to_string())
}

pub fn handle_peers(stream: TcpStream, peers: String) -> Result<()> {
    reply(stream, peers)
}
