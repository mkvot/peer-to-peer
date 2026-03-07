mod server;
mod client;
use std::env;

fn main() -> std::io::Result<()> {
    let mode = env::args().nth(1).unwrap_or("".to_string());
    println!("Started application");
    if mode == "c" {
        let msg = env::args().nth(2).unwrap_or("hi".to_string());
        client::start(msg)?
    } else {
        server::start()?
    }
    Ok(())
}
