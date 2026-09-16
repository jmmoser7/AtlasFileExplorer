fn main() {
    let exe = std::env::args_os().nth(1).expect("codex executable path");
    match atlas_codex::Client::probe(
        std::path::Path::new(&exe),
        &std::env::current_dir().unwrap(),
    ) {
        Ok(()) => println!("Codex initialize + account/read succeeded; no model turn started."),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
