use model_preview::view_meta::{hash_bytes, hash_reader};
#[test]
fn model_fingerprint_stream_matches_bytes_across_buffer_boundaries() {
    let bytes: Vec<u8> = (0..131_079usize).map(|i| (i % 251) as u8).collect();
    assert_eq!(
        hash_reader(std::io::Cursor::new(&bytes)).unwrap(),
        hash_bytes(&bytes)
    );
}
