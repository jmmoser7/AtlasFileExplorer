//! Read-only audit probe. Link against the workspace's compiled libraries.
use std::{path::PathBuf, time::Instant};

fn main() {
    let path = PathBuf::from(std::env::args_os().nth(1).expect("model path"));
    if atlas_core::cloud::is_dehydrated(&path) {
        eprintln!("Refusing byte reads: cloud-only or unavailable source.");
        std::process::exit(2);
    }
    println!("source={}", path.display());
    println!("bytes={}", std::fs::metadata(&path).unwrap().len());
    let start = Instant::now();
    match rhino_mesh::read_render_meshes(&path) {
        Ok(model) => {
            println!("result=ready");
            println!("parts={}", model.parts.len());
            println!("vertices={}", model.parts.iter().map(|p| p.positions.len()).sum::<usize>());
            println!("triangles={}", model.parts.iter().map(|p| p.indices.len() / 3).sum::<usize>());
            println!("bounds_min={:?}", model.bounds_min);
            println!("bounds_max={:?}", model.bounds_max);
        }
        Err(error) => println!("result={error:?}\nmessage={error}"),
    }
    println!("read_and_parse_ms={:.3}", start.elapsed().as_secs_f64() * 1000.0);
}
