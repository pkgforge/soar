//! A local HTTP server for exercising the downloaders against real responses.

use std::{
    collections::HashMap,
    io::{BufRead as _, BufReader, Write as _},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
};

/// How the server behaves where a download cares.
#[derive(Default, Clone)]
pub struct ServeOptions {
    /// Send no `Accept-Ranges` header and answer range requests with the
    /// whole body, as a server without range support would.
    pub ignore_ranges: bool,
    /// Send `Accept-Ranges: bytes` but still answer range requests with the
    /// whole body, as a misbehaving server would.
    pub lie_about_ranges: bool,
    /// Close the connection mid-body once this many bytes of it are out, as
    /// an interrupted transfer would.
    pub close_after_bytes: Option<usize>,
    /// Close any range request whose start offset lies at or within the
    /// second field's bytes after a multiple of the first, then serve the
    /// rest whole, as a link that repeatedly cuts the first stretch of every
    /// chunk's transfer would. Deterministic under any worker interleaving.
    pub truncate_periodic: Option<(usize, usize)>,
    /// Send this many extra bytes past the requested range on 206 bodies,
    /// as a server that overshoots its Content-Range would.
    pub oversend: Option<usize>,
    /// Drop every HEAD connection without a response, as HEAD-refusing
    /// servers do.
    pub refuse_head: bool,
}

/// Serves each body in `routes` at its path until the test ends, and returns
/// the base URL. `Range` requests are honored with a 206, other paths are 404.
pub fn serve(routes: Vec<(String, Vec<u8>)>) -> String {
    serve_with(routes, ServeOptions::default()).0
}

/// Serves under `options` and reports every request with its `Range` header,
/// in arrival order.
pub fn serve_with(
    routes: Vec<(String, Vec<u8>)>,
    options: ServeOptions,
) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let routes: HashMap<_, _> = routes.into_iter().collect();
    let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let recorded = requests.clone();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            respond(stream, &routes, &options, &recorded);
        }
    });
    (format!("http://{addr}"), requests)
}

fn respond(
    mut stream: TcpStream,
    routes: &HashMap<String, Vec<u8>>,
    options: &ServeOptions,
    requests: &Arc<Mutex<Vec<String>>>,
) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();

    let mut range: Option<(usize, Option<usize>)> = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("range")
        {
            range = value.trim().strip_prefix("bytes=").and_then(parse_range);
        }
    }
    requests
        .lock()
        .unwrap()
        .push(format!("{method} {path} range={range:?}"));

    if options.refuse_head && method == "HEAD" {
        // Dropping the stream without a response is the refusal.
        return;
    }

    let range_honored = !options.ignore_ranges && !options.lie_about_ranges;
    let (status, extra, body) = match routes.get(&path) {
        None => ("404 Not Found", String::new(), &b""[..]),
        Some(body) => {
            match range.filter(|_| range_honored) {
                Some((start, end)) if start < body.len() => {
                    let end = end.map_or(body.len() - 1, |end| end.min(body.len() - 1));
                    (
                        "206 Partial Content",
                        format!("Content-Range: bytes {start}-{end}/{}\r\n", body.len()),
                        &body[start..=end],
                    )
                }
                _ => ("200 OK", String::new(), &body[..]),
            }
        }
    };

    // Only whole 206 responses overshoot, simulating a server whose body
    // does not stop where its Content-Range promised.
    let overshoot = if status == "206 Partial Content" {
        options.oversend.unwrap_or(0)
    } else {
        0
    };
    let truncated = options
        .truncate_periodic
        .filter(|_| method == "GET")
        .and_then(|(period, within)| {
            let (start, _) = range?;
            (start % period <= within).then_some(within)
        });

    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{extra}",
        body.len() + overshoot
    );
    if !options.ignore_ranges || options.lie_about_ranges {
        head.push_str("Accept-Ranges: bytes\r\n");
    }
    head.push_str("Connection: close\r\n\r\n");
    let _ = stream.write_all(head.as_bytes());
    if method == "HEAD" {
        return;
    }
    if let Some(limit) = truncated {
        let _ = stream.write_all(&body[..limit.min(body.len())]);
        // Dropping the stream here is the interruption.
        return;
    }
    match options.close_after_bytes {
        Some(limit) if limit < body.len() => {
            let _ = stream.write_all(&body[..limit]);
            // Dropping the stream here is the interruption.
        }
        _ => {
            let _ = stream.write_all(body);
            if overshoot > 0 {
                let _ = stream.write_all(&vec![0u8; overshoot]);
            }
        }
    }
}

/// Parses a `bytes=N-M` or `bytes=N-` range value.
fn parse_range(value: &str) -> Option<(usize, Option<usize>)> {
    let (start, end) = value.trim().split_once('-')?;
    Some((start.trim().parse().ok()?, end.trim().parse().ok()))
}
