//! Read-only installed CLI smoke: prints no account identity or credentials.
fn main() {
    let mut failed = false;
    for provider in [
        atlas_subscription::Provider::Cursor,
        atlas_subscription::Provider::ClaudeCode,
    ] {
        match atlas_subscription::discover(provider) {
            Ok(found) => {
                println!(
                    "{} version={} auth={:?} method={:?}",
                    provider.id(),
                    found.version,
                    found.auth,
                    found.method
                );
                failed |= found.auth != atlas_subscription::AuthStatus::LoggedIn;
                match atlas_subscription::models(provider) {
                    Ok(models) => println!(
                        "{} catalog={}",
                        provider.id(),
                        serde_json::to_string(&models).unwrap()
                    ),
                    Err(error) => {
                        eprintln!("{} catalog: {error}", provider.id());
                        failed = true;
                    }
                }
            }
            Err(error) => {
                eprintln!("{}: {error}", provider.id());
                failed = true;
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}
