mod server;

fn main() -> std::io::Result<()> {
    println!("Started application");
    server::start()
}
