//! Lightweight HTTP/HTTPS URL fetcher for package registry interactions.
//! Uses pure-Rust std::net::TcpStream for HTTP/1.1 and system curl fallback for HTTPS.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Fetches raw bytes from a URL (http://, https://, or file://).
pub fn fetch_url(url: &str) -> Result<Vec<u8>, String> {
    if let Some(mut file_path) = url.strip_prefix("file://") {
        if cfg!(windows)
            && file_path.starts_with('/')
            && file_path.len() > 2
            && file_path.as_bytes()[2] == b':'
        {
            file_path = &file_path[1..];
        }
        let p = Path::new(file_path);
        return fs::read(p)
            .map_err(|e| format!("Failed to read local file '{}': {}", file_path, e));
    }

    if url.starts_with("http://") {
        return fetch_http(url);
    }

    if url.starts_with("https://") {
        return fetch_https(url);
    }

    Err(format!(
        "Unsupported URL scheme in '{}': must be http://, https://, or file://",
        url
    ))
}

/// Pure-Rust HTTP/1.1 client over TcpStream (zero external dependencies, handles HTTP GET).
pub fn fetch_http(url: &str) -> Result<Vec<u8>, String> {
    let mut last_err = String::new();
    for attempt in 0..3 {
        match fetch_http_once(url) {
            Ok(bytes) => return Ok(bytes),
            Err(e) => {
                last_err = e;
                if attempt < 2 {
                    std::thread::sleep(Duration::from_millis(50 * (attempt as u64 + 1)));
                }
            }
        }
    }
    Err(last_err)
}

fn fetch_http_once(url: &str) -> Result<Vec<u8>, String> {
    let without_scheme = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("Invalid HTTP URL: '{}'", url))?;

    let (host_port, path) = match without_scheme.find('/') {
        Some(pos) => (&without_scheme[..pos], &without_scheme[pos..]),
        None => (without_scheme, "/"),
    };

    let (host, port) = match host_port.find(':') {
        Some(pos) => {
            let h = &host_port[..pos];
            let p: u16 = host_port[pos + 1..]
                .parse()
                .map_err(|e| format!("Invalid port in '{}': {}", host_port, e))?;
            (h, p)
        }
        None => (host_port, 80),
    };

    let addr = format!("{}:{}", host, port);
    let mut stream = TcpStream::connect(&addr)
        .map_err(|e| format!("Failed to connect to HTTP host '{}': {}", addr, e))?;

    let timeout = Duration::from_secs(15);
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));

    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: datara-dpm/1.0.0\r\nConnection: close\r\nAccept: */*\r\n\r\n",
        path, host_port
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("Failed to send HTTP request to '{}': {}", addr, e))?;
    stream.flush().map_err(|e| e.to_string())?;

    let mut response_bytes = Vec::new();
    stream
        .read_to_end(&mut response_bytes)
        .map_err(|e| format!("Failed to read HTTP response from '{}': {}", addr, e))?;

    parse_http_response(&response_bytes)
}

/// Parses an HTTP/1.1 response buffer and returns the response body bytes.
fn parse_http_response(raw: &[u8]) -> Result<Vec<u8>, String> {
    let header_delim = b"\r\n\r\n";
    let header_end = raw
        .windows(4)
        .position(|w| w == header_delim)
        .ok_or_else(|| "Malformed HTTP response: missing header delimiter".to_string())?;

    let header_str = std::str::from_utf8(&raw[..header_end])
        .map_err(|e| format!("Malformed HTTP header encoding: {}", e))?;

    let mut lines = header_str.lines();
    let status_line = lines
        .next()
        .ok_or_else(|| "Empty HTTP response status line".to_string())?;

    let mut status_parts = status_line.split_whitespace();
    let _http_ver = status_parts.next();
    let status_code_str = status_parts
        .next()
        .ok_or_else(|| format!("Invalid status line: '{}'", status_line))?;
    let status_code: u16 = status_code_str
        .parse()
        .map_err(|e| format!("Invalid HTTP status code '{}': {}", status_code_str, e))?;

    if !(200..300).contains(&status_code) {
        return Err(format!(
            "HTTP request failed with status {}: {}",
            status_code, status_line
        ));
    }

    let mut is_chunked = false;
    let mut content_length: Option<usize> = None;

    for line in lines {
        if let Some(pos) = line.find(':') {
            let key = line[..pos].trim().to_lowercase();
            let val = line[pos + 1..].trim();
            if key == "transfer-encoding" && val.to_lowercase().contains("chunked") {
                is_chunked = true;
            } else if key == "content-length" {
                if let Ok(len) = val.parse::<usize>() {
                    content_length = Some(len);
                }
            }
        }
    }

    let body_slice = &raw[header_end + 4..];

    if is_chunked {
        decode_chunked_body(body_slice)
    } else if let Some(len) = content_length {
        if body_slice.len() >= len {
            Ok(body_slice[..len].to_vec())
        } else {
            // EOF before the advertised Content-Length: a truncated download
            // must not be silently treated as a complete package archive.
            Err(format!(
                "Truncated HTTP response: got {} of {} advertised bytes",
                body_slice.len(),
                len
            ))
        }
    } else {
        Ok(body_slice.to_vec())
    }
}

/// Decodes an HTTP chunked transfer-encoded body.
fn decode_chunked_body(mut data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();

    loop {
        if data.is_empty() {
            break;
        }
        let line_end = data
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or_else(|| "Malformed chunk header in chunked encoding".to_string())?;

        let hex_str = std::str::from_utf8(&data[..line_end])
            .map_err(|e| format!("Invalid hex in chunk size: {}", e))?
            .trim();
        let chunk_size = usize::from_str_radix(hex_str, 16)
            .map_err(|e| format!("Invalid chunk size '{}': {}", hex_str, e))?;

        if chunk_size == 0 {
            break;
        }

        let chunk_start = line_end + 2;
        let chunk_end = chunk_start
            .checked_add(chunk_size)
            .ok_or_else(|| "Chunk size overflow in chunked encoding".to_string())?;

        if chunk_end > data.len() {
            return Err("Unexpected EOF while reading chunk data".to_string());
        }

        out.extend_from_slice(&data[chunk_start..chunk_end]);

        data = if chunk_end + 2 <= data.len() {
            &data[chunk_end + 2..] // Skip trailing \r\n
        } else {
            &data[chunk_end..]
        };
    }

    Ok(out)
}

/// Fetches an HTTPS URL using system curl.
fn fetch_https(url: &str) -> Result<Vec<u8>, String> {
    let output = Command::new("curl")
        .args(["-sSL", "--fail", "--max-time", "15", url])
        .output()
        .map_err(|e| {
            format!(
                "Failed to execute system 'curl' to fetch HTTPS URL '{}': {}. Ensure curl is installed or use http:// / file://",
                url, e
            )
        })?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Failed to fetch HTTPS URL '{}': curl exited with code {:?}. Details: {}",
            url,
            output.status.code(),
            err_msg.trim()
        ));
    }

    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_http_response_truncated_body_rejected() {
        // A server advertising Content-Length: 10 but sending only 4 bytes
        // must be reported as a truncated response, not silently accepted as
        // a complete body (a truncated package archive would otherwise be
        // treated as a successful download).
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nABCD";
        let result = parse_http_response(raw);
        assert!(
            result.is_err(),
            "Truncated body must not be silently accepted: {:?}",
            result
        );
    }

    #[test]
    fn test_parse_http_response_complete_body() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nABCD";
        assert_eq!(parse_http_response(raw).unwrap(), b"ABCD".to_vec());
    }

    #[test]
    fn test_parse_http_response_error_status() {
        let raw = b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
        assert!(parse_http_response(raw).is_err());
    }

    #[test]
    fn test_decode_chunked_body_no_overflow_panic() {
        // A hostile server advertising a chunk size close to usize::MAX must
        // produce an error, not a debug-build arithmetic overflow panic.
        let mut raw = b"FFFFFFFFFFFFFFFF\r\nAB\r\n".to_vec();
        raw.extend_from_slice(b"0\r\n\r\n");
        assert!(decode_chunked_body(&raw).is_err());
    }

    #[test]
    fn test_decode_chunked_body_roundtrip() {
        let raw = b"3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n";
        assert_eq!(decode_chunked_body(raw).unwrap(), b"abcde".to_vec());
    }
}
