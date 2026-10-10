//! Probe execution. Callers run this on a worker, never on a frame.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use crate::manifest::ProbeSpec;

/// Health of one pack this session. This is not `LinkStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackHealth {
    Ok,
    Missing,
    Unknown,
    Retired,
}

impl PackHealth {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "Ok",
            Self::Missing => "Missing",
            Self::Unknown => "Unknown",
            Self::Retired => "Retired",
        }
    }
}

pub(crate) struct ProbeOutcome {
    pub health: PackHealth,
    /// What was looked for, so a failure can name it (Article IX).
    pub looked_for: String,
}

pub(crate) fn run(spec: &ProbeSpec, custom: &dyn Fn(&str) -> Option<PackHealth>) -> ProbeOutcome {
    match spec {
        ProbeSpec::KnownPath { names, roots } => known_path(names, roots),
        ProbeSpec::CommandOnPath { command } => command_on_path(command),
        ProbeSpec::HttpGet { url, timeout_ms } => http_get(url, *timeout_ms),
        ProbeSpec::Custom { id } => {
            let looked_for = format!("custom probe `{id}`");
            let health = custom(id).unwrap_or(PackHealth::Unknown);
            ProbeOutcome { health, looked_for }
        }
    }
}

fn known_path(names: &[String], roots: &[String]) -> ProbeOutcome {
    let mut looked = Vec::new();
    for root in roots {
        let root = expand_root(root);
        if names.is_empty() {
            looked.push(root.display().to_string());
            if root.is_file() {
                return ProbeOutcome {
                    health: PackHealth::Ok,
                    looked_for: looked.join(", "),
                };
            }
            continue;
        }
        for name in names {
            let path = root.join(name);
            looked.push(path.display().to_string());
            if path.is_file() {
                return ProbeOutcome {
                    health: PackHealth::Ok,
                    looked_for: looked.join(", "),
                };
            }
        }
    }
    ProbeOutcome {
        health: PackHealth::Missing,
        looked_for: if looked.is_empty() {
            "a file path".into()
        } else {
            looked.join(", ")
        },
    }
}

fn command_on_path(command: &str) -> ProbeOutcome {
    let looked_for = format!("`{command}` on PATH");
    let Some(paths) = std::env::var_os("PATH") else {
        return ProbeOutcome {
            health: PackHealth::Missing,
            looked_for,
        };
    };
    for dir in std::env::split_paths(&paths) {
        for name in command_names(command) {
            if dir.join(name).is_file() {
                return ProbeOutcome {
                    health: PackHealth::Ok,
                    looked_for,
                };
            }
        }
    }
    ProbeOutcome {
        health: PackHealth::Missing,
        looked_for,
    }
}

fn command_names(command: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        vec![
            format!("{command}.exe"),
            format!("{command}.cmd"),
            command.to_string(),
        ]
    }
    #[cfg(not(windows))]
    {
        vec![command.to_string()]
    }
}

fn http_get(url: &str, timeout_ms: u64) -> ProbeOutcome {
    let looked_for = format!("GET {url}");
    let timeout = Duration::from_millis(timeout_ms.max(1));
    let Some((addr, path)) = loopback(url) else {
        return ProbeOutcome {
            health: PackHealth::Unknown,
            looked_for: format!("{looked_for} (not a loopback URL)"),
        };
    };
    let mut stream = match TcpStream::connect_timeout(&addr, timeout) {
        Ok(stream) => stream,
        Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
            return ProbeOutcome {
                health: PackHealth::Unknown,
                looked_for: format!("{looked_for} timed out"),
            };
        }
        Err(error) => {
            return ProbeOutcome {
                health: PackHealth::Missing,
                looked_for: format!("{looked_for} ({error})"),
            };
        }
    };
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    let request = format!(
        "GET {path} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
        addr.ip()
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return ProbeOutcome {
            health: PackHealth::Unknown,
            looked_for: format!("{looked_for} timed out"),
        };
    }
    let mut buf = [0_u8; 32];
    match stream.read(&mut buf) {
        Ok(0) | Err(_) => ProbeOutcome {
            health: PackHealth::Unknown,
            looked_for: format!("{looked_for} timed out"),
        },
        Ok(n) => {
            let text = String::from_utf8_lossy(&buf[..n]);
            let health = if text.starts_with("HTTP/") && text.contains(" 2") {
                PackHealth::Ok
            } else if text.starts_with("HTTP/") {
                PackHealth::Missing
            } else {
                PackHealth::Unknown
            };
            ProbeOutcome { health, looked_for }
        }
    }
}

/// `http://127.0.0.1:port/path` or `http://localhost:port/path` only.
fn loopback(url: &str) -> Option<(SocketAddr, String)> {
    let rest = url.strip_prefix("http://")?;
    let (host_port, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (host, port) = host_port.split_once(':')?;
    if host != "127.0.0.1" && host != "localhost" {
        return None;
    }
    let port: u16 = port.parse().ok()?;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    Some((addr, format!("/{path}")))
}

fn expand_root(root: &str) -> PathBuf {
    let mut out = String::new();
    let mut rest = root;
    while let Some(start) = rest.find(['%', '$']) {
        out.push_str(&rest[..start]);
        let mark = rest[start..].chars().next().unwrap();
        if mark == '%' {
            if let Some(end) = rest[start + 1..].find('%') {
                let name = &rest[start + 1..start + 1 + end];
                if let Ok(value) = std::env::var(name) {
                    out.push_str(&value);
                }
                rest = &rest[start + end + 2..];
                continue;
            }
        }
        if mark == '$' {
            let name: String = rest[start + 1..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                if let Ok(value) = std::env::var(&name) {
                    out.push_str(&value);
                }
                rest = &rest[start + 1 + name.len()..];
                continue;
            }
        }
        out.push(mark);
        rest = &rest[start + mark.len_utf8()..];
    }
    out.push_str(rest);
    PathBuf::from(out)
}
