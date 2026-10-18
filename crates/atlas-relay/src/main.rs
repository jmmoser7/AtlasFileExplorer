//! Small self-hosted participant relay. The invite is an explicit user-facing
//! output, not a diagnostic. No key, invite, or conversation is written to disk.
use atlas_relay::Host;
use std::net::SocketAddr;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut bind: SocketAddr = "127.0.0.1:0".parse()?;
    let mut advertise = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bind" => bind = args.next().ok_or("--bind needs IP:port")?.parse()?,
            "--advertise" => advertise = Some(args.next().ok_or("--advertise needs IP:port")?.parse()?),
            "--help" | "-h" => {
                println!("atlas-relay [--bind 127.0.0.1:0] [--advertise IP:port]\nEncrypted participant chat only; press Enter to stop.");
                return Ok(());
            }
            _ => return Err("Unknown argument; use --help".into()),
        }
    }
    let host = Host::start(bind)?;
    let mut invite = host.invite();
    if let Some(addr) = advertise { invite = invite.with_address(addr)?; }
    if host.address().ip().is_unspecified() && advertise.is_none() {
        return Err("An unspecified bind address needs --advertise with a reachable IP and the bound port".into());
    }
    println!("Participant relay at {}", invite.endpoint());
    println!("Share this private invite only with intended participants:\n{}", invite.encode()?);
    println!("Certificate SHA-256: {}\nPress Enter to stop.", invite.certificate_fingerprint());
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    host.stop();
    Ok(())
}
