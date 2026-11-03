//! Persistent JSONL adapter for historical Rust source blobs. This command
//! performs no Git access, checkout, build, source write or cross-file analysis.

use std::io::{self, BufRead, Write};
use std::path::{Component, Path};

use code_lens::observatory::{analyze_source, SingleSymbol};
use serde::{Deserialize, Serialize};

use crate::collect::line_counts;
use crate::rust_parse;

/// Bump when source metric or symbol extraction semantics change, so the
/// historical collector can invalidate its path/blob/version cache.
pub const EXTRACTOR_VERSION: u32 = 1;

#[derive(Deserialize)]
struct Request {
    path: String,
    blob: String,
    source: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    extractor_version: u32,
    path: String,
    blob: String,
    available: bool,
    lines_total: Option<u32>,
    lines_code: Option<u32>,
    functions: Option<usize>,
    tests: Option<u32>,
    unsafe_blocks: Option<u32>,
    file_id: Option<String>,
    file_self_loc: Option<u32>,
    symbols: Vec<SingleSymbol>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl Response {
    fn unavailable(path: String, blob: String, error: String) -> Self {
        Self {
            extractor_version: EXTRACTOR_VERSION,
            path,
            blob,
            available: false,
            lines_total: None,
            lines_code: None,
            functions: None,
            tests: None,
            unsafe_blocks: None,
            file_id: None,
            file_self_loc: None,
            symbols: Vec::new(),
            error: Some(error),
        }
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let input = io::stdin();
    let output = io::stdout();
    process(input.lock(), output.lock())?;
    Ok(())
}

fn process(mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    let mut line = Vec::new();
    loop {
        line.clear();
        if input.read_until(b'\n', &mut line)? == 0 {
            return Ok(());
        }
        let response = row(&line);
        serde_json::to_writer(&mut output, &response).map_err(io::Error::other)?;
        output.write_all(b"\n")?;
        // The collector is request/response: never wait for a buffer to fill.
        output.flush()?;
    }
}

fn row(line: &[u8]) -> Response {
    let value = match serde_json::from_slice::<serde_json::Value>(line) {
        Ok(value) => value,
        Err(error) => {
            return Response::unavailable(
                String::new(),
                String::new(),
                format!("Invalid JSON request: {error}"),
            )
        }
    };
    let path = value
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let blob = value
        .get("blob")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let request = match serde_json::from_value::<Request>(value) {
        Ok(request) => request,
        Err(error) => {
            return Response::unavailable(path, blob, format!("Invalid source request: {error}"))
        }
    };
    let normalized = request.path.replace('\\', "/");
    let path = Path::new(&normalized);
    let mut response = Response::unavailable(request.path, request.blob, String::new());
    let (total, code) = line_counts(&request.source);
    response.lines_total = Some(total);
    response.lines_code = Some(code);
    if normalized.is_empty()
        || normalized.contains(':')
        || path.is_absolute()
        || normalized
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        response.error = Some("Path must be a clean repository-relative source path.".into());
        return response;
    }
    match analyze_source(path, &request.source) {
        Ok(analysis) => {
            response.available = analysis.available;
            response.error = analysis.error;
            response.file_id = Some(analysis.file_id);
            response.file_self_loc = Some(analysis.file_self_loc);
            if let Some(ast) = &analysis.syntax {
                let counts = rust_parse::file_counts(ast);
                response.tests = Some(counts.tests);
                response.unsafe_blocks = Some(counts.unsafe_blocks);
                response.functions = Some(
                    analysis
                        .symbols
                        .iter()
                        .filter(|s| ["function", "method"].contains(&s.kind))
                        .count(),
                );
            }
            response.symbols = analysis.symbols;
        }
        Err(error) => response.error = Some(format!("Source extraction failed: {error}")),
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(source: &str) -> Vec<u8> {
        serde_json::to_vec(
            &serde_json::json!({"path":"crates/demo/src/lib.rs","blob":"sample","source":source}),
        )
        .unwrap()
    }

    #[test]
    fn ast_metrics_ignore_mentions_and_symbols_include_nested_functions_and_methods() {
        let source = "// #[test] unsafe {}\nconst NOTE: &str = \"UNIQUE_BODY_MARKER #[test] unsafe {}\";\nstruct Worker;\nimpl Worker { fn run(&self) { let _ = unsafe { 1 }; } }\n#[test]\nfn actual() { fn nested() {} }\n";
        let response = row(&request(source));
        assert!(response.available);
        assert_eq!(response.tests, Some(1));
        assert_eq!(response.unsafe_blocks, Some(1));
        assert_eq!(response.functions, Some(3));
        assert_eq!(response.lines_total, Some(line_counts(source).0));
        assert!(!serde_json::to_string(&response)
            .unwrap()
            .contains("UNIQUE_BODY_MARKER"));
        assert!(response.file_id.is_some());
    }

    #[test]
    fn parse_failure_has_physical_metrics_and_unknown_ast_counts() {
        let response = row(&request("fn broken(\n\n"));
        assert!(!response.available && response.symbols.is_empty());
        assert_eq!(
            (response.lines_total, response.lines_code),
            (Some(2), Some(1))
        );
        assert!(
            response.functions.is_none()
                && response.tests.is_none()
                && response.unsafe_blocks.is_none()
        );
        assert!(response.error.unwrap().contains("Rust parse failed"));
    }

    #[test]
    fn stream_recovers_after_bad_rows_and_emits_one_response_per_input_line() {
        let mut input = b"not-json\n".to_vec();
        input.extend(request("fn café() {}\n"));
        input.push(b'\n');
        input.extend(request("fn later() {}"));
        let mut output = Vec::new();
        process(io::Cursor::new(input), &mut output).unwrap();
        let rows: Vec<serde_json::Value> = output
            .split(|b| *b == b'\n')
            .filter(|r| !r.is_empty())
            .map(|r| serde_json::from_slice(r).unwrap())
            .collect();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["available"], false);
        assert_eq!(rows[1]["available"], true);
        assert_eq!(rows[1]["symbols"][0]["name"], "café");
        assert_eq!(rows[2]["functions"], 1);
        assert_eq!(rows[2]["extractorVersion"], EXTRACTOR_VERSION);
    }

    #[test]
    fn rejects_absolute_and_parent_paths_without_reading_files() {
        for path in ["../outside.rs", "C:/private/file.rs", "/private/file.rs"] {
            let input = serde_json::to_vec(
                &serde_json::json!({"path":path,"blob":"x","source":"fn ok() {}"}),
            )
            .unwrap();
            let response = row(&input);
            assert!(!response.available);
            assert!(response.file_id.is_none());
        }
    }
}
