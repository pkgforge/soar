//! A local HTTP server for exercising the downloaders against real responses.

use std::{
    collections::HashMap,
    io::{BufRead as _, BufReader, Write as _},
    net::{TcpListener, TcpStream},
    thread,
};

/// Serves each body in `routes` at its path until the test ends, and returns
/// the base URL. `Range: bytes=N-` is honored with a 206, other paths are 404.
pub fn serve(routes: Vec<(String, Vec<u8>)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let routes: HashMap<_, _> = routes.into_iter().collect();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            respond(stream, &routes);
        }
    });
    format!("http://{addr}")
}

fn respond(mut stream: TcpStream, routes: &HashMap<String, Vec<u8>>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();

    let mut range_start = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("range")
        {
            range_start = value
                .trim()
                .strip_prefix("bytes=")
                .and_then(|r| r.strip_suffix('-'))
                .and_then(|r| r.parse::<usize>().ok());
        }
    }

    let (status, extra, body) = match routes.get(&path) {
        None => ("404 Not Found", String::new(), &b""[..]),
        Some(body) => {
            match range_start {
                Some(start) if start < body.len() => {
                    (
                        "206 Partial Content",
                        format!(
                            "Content-Range: bytes {start}-{}/{}\r\n",
                            body.len() - 1,
                            body.len()
                        ),
                        &body[start..],
                    )
                }
                _ => ("200 OK", String::new(), &body[..]),
            }
        }
    };

    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    if method != "HEAD" {
        let _ = stream.write_all(body);
    }
}
