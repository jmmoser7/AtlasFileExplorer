//! Independent application connector handshake and round-trip runner.
#[cfg(windows)]
#[path = "../conformance.rs"]
mod conformance;

fn main() {
    #[cfg(windows)]
    if let Err(error) = conformance::run() {
        eprintln!("Connector conformance failed: {error}");
        std::process::exit(1);
    }
    #[cfg(not(windows))]
    {
        eprintln!(
            "Named-pipe conformance requires Windows; pure protocol tests run on every platform."
        );
        std::process::exit(2);
    }
}
