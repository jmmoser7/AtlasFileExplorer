//! Regenerate the public, language-neutral contract from its Rust owner.
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let check = std::env::args().skip(1).any(|arg| arg == "--check");
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/application-connectors/v0.1");
    for (name, schema) in atlas_connector::schema_documents() {
        publish(&root.join("schemas").join(name), &schema, check)?;
    }
    for (name, frame) in atlas_connector::fixture_frames() {
        publish(
            &root.join("fixtures").join(name),
            &serde_json::to_value(frame)?,
            check,
        )?;
    }
    for (name, value) in atlas_connector::artifact_fixture_documents() {
        publish(&root.join("fixtures").join(name), &value, check)?;
    }
    println!(
        "Connector v0.1 schemas and fixtures {}.",
        if check {
            "match their source"
        } else {
            "generated"
        }
    );
    Ok(())
}

fn publish(
    path: &Path,
    value: &serde_json::Value,
    check: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let expected = format!("{}\n", serde_json::to_string_pretty(value)?);
    if check {
        if std::fs::read_to_string(path)? != expected {
            return Err(format!("Generated contract is stale: {}", path.display()).into());
        }
    } else {
        std::fs::create_dir_all(path.parent().ok_or("Missing output directory")?)?;
        std::fs::write(path, expected)?;
    }
    Ok(())
}
