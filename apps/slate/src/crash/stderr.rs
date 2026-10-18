//! Bounded rolling stderr capture. Runs only on the launcher's drain thread.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

const CAP: usize = 64 * 1024;

pub(super) fn capture(mut input: impl Read, path: &Path) {
    let mut tail = Vec::with_capacity(CAP);
    let mut chunk = [0; 4096];
    // Opening failure must not stop the drain and block the GUI on stderr.
    let mut output = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .ok();
    loop {
        let n = match input.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        append(&mut tail, &chunk[..n]);
        if let Some(file) = &mut output {
            if persist(file, &tail).is_err() {
                output = None;
            }
        }
    }
}

fn append(tail: &mut Vec<u8>, chunk: &[u8]) {
    let excess = (tail.len() + chunk.len()).saturating_sub(CAP);
    tail.drain(..excess.min(tail.len()));
    tail.extend_from_slice(&chunk[chunk.len().saturating_sub(CAP)..]);
}

fn persist(file: &mut File, tail: &[u8]) -> io::Result<()> {
    file.seek(SeekFrom::Start(0))?;
    file.write_all(tail)?;
    file.set_len(tail.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_the_most_recent_output() {
        let mut tail = vec![b'a'; CAP - 2];
        append(&mut tail, b"bcde");
        assert_eq!(tail.len(), CAP);
        assert!(tail.ends_with(b"bcde"));
        append(&mut tail, &vec![b'z'; CAP * 2]);
        assert_eq!(tail, vec![b'z'; CAP]);
    }
}
