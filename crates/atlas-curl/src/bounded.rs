use super::{Failure, Request};
use std::io::Read;

impl Request {
    /// Drain at most `limit` response bytes, killing and reaping oversized
    /// transfers even when the server omits Content-Length.
    pub fn send_bounded(&self, limit: usize) -> Result<Vec<u8>, Failure> {
        read_bounded(self.spawn()?, limit)
    }

    /// Return a non-error HTTP status and a bounded body without loading curl's user configuration.
    pub fn send_bounded_with_status(&self, limit: usize) -> Result<(u16, Vec<u8>), Failure> {
        let mut config = self.config()?;
        config.push_str("write-out = \"%{http_code}\"\n");
        let mut bytes = read_bounded(Self::spawn_config(&config, true)?, limit.saturating_add(3))?;
        let body_len = bytes.len().checked_sub(3);
        let status = body_len
            .and_then(|n| std::str::from_utf8(&bytes[n..]).ok())
            .and_then(|code| code.parse::<u16>().ok())
            .ok_or_else(|| Failure::Transport("The HTTP response had no status code.".into()))?;
        bytes.truncate(bytes.len() - 3);
        Ok((status, bytes))
    }
}

fn read_bounded(mut child: std::process::Child, limit: usize) -> Result<Vec<u8>, Failure> {
    let result = (|| {
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Failure::Transport("curl gave no output pipe.".into()))?;
        let mut bytes = Vec::new();
        stdout
            .take((limit as u64).saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|e| Failure::Transport(e.to_string()))?;
        if bytes.len() > limit {
            return Err(Failure::Transport(
                "The downloaded resource is too large.".into(),
            ));
        }
        Ok(bytes)
    })();
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child
        .wait()
        .map_err(|e| Failure::Transport(e.to_string()))?;
    let bytes = result?;
    if !status.success() {
        return Err(Failure::Transport(
            "The image could not be downloaded.".into(),
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    #[test]
    fn chunked_response_cannot_exceed_the_cap() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 2048];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n10\r\n0123456789abcdef\r\n0\r\n\r\n");
        });
        assert!(Request::get(format!("http://{address}/image"))
            .loopback()
            .send_bounded(8)
            .is_err());
        server.join().unwrap();
    }

    #[test]
    fn image_redirects_are_bounded_and_http_only() {
        let mut request = Request::get("https://example.test/image");
        request.follow_redirects = true;
        let config = request.config().unwrap();
        assert!(config.contains("max-redirs = 5"));
        assert!(config.contains("proto-redir = \"=http,https\""));
    }
}
