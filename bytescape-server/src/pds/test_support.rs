//! Shared test helpers for the `pds` modules (HTTP stub for the PLC
//! directory / relay-crawl endpoints).
//!
//! Review #2 (R2-15): the original one-canned-response stub could not express
//! the re-identify defect classes — a lost mint response (server accepts then
//! the connection dies), a verify-after-failure reply, or per-connection
//! scripting. [`stub_http_seq`] scripts one reply per connection and captures
//! every full request (head + body) so tests can assert WHAT was sent (e.g.
//! two mint attempts carrying byte-identical genesis ops = one identity).

#[cfg(test)]
use std::io::{Read, Write};
#[cfg(test)]
use parking_lot::Mutex;
#[cfg(test)]
use std::sync::Arc;

/// One scripted stub reply (review #2 R2-15).
#[cfg(test)]
pub enum StubReply {
    /// Answer with this status line + JSON body.
    Respond(String, String),
    /// Accept the request (read it fully — it is captured) then close the
    /// connection WITHOUT a response: the client sees a transport error AFTER
    /// the server-side action happened — exactly the lost-mint-response crash
    /// window the re-identify claim must survive.
    CloseNoResponse,
}

/// Scripted HTTP stub: one reply per accepted connection from `steps` (the
/// last step repeats for extra connections), capturing each full request
/// (head + body) in order into one string. Returns `(base_url, captured,
/// thread)`.
#[cfg(test)]
pub fn stub_http_seq(
    steps: Vec<StubReply>,
) -> (String, Arc<Mutex<String>>, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind stub listener");
    let base = format!("http://{}", listener.local_addr().expect("stub addr"));
    let captured = Arc::new(Mutex::new(String::new()));
    let cap = Arc::clone(&captured);

    // Bounded accept (8s per step): a call-count regression must fail the
    // test's assertions, not hang the join.
    if listener.set_nonblocking(true).is_err() {
        return (base, captured, std::thread::spawn(|| {}));
    }
    let thread = std::thread::spawn(move || {
        for step in &steps {
            let mut accepted = None;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
            loop {
                match listener.accept() {
                    Ok(pair) => {
                        accepted = Some(pair);
                        break;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if std::time::Instant::now() >= deadline {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(25));
                    }
                    Err(_) => break,
                }
            }
            let Some((mut stream, _)) = accepted else { return };
            let _ = stream.set_nonblocking(false);
            let mut buf = Vec::new();
            let mut tmp = [0u8; 1024];
            loop {
                let Ok(n) = stream.read(&mut tmp) else { break };
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&tmp[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let head_text = String::from_utf8_lossy(&buf).to_string();
            let content_length: usize = head_text
                .lines()
                .find_map(|l| {
                    let (k, v) = l.split_once(':')?;
                    k.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse().ok())?
                })
                .unwrap_or(0);
            // Body bytes may already have arrived in the head's segment.
            let boundary = buf
                .windows(4)
                .position(|w| w == b"\r\n\r\n")
                .map_or(buf.len(), |i| i + 4);
            let mut body_buf = buf.split_off(boundary.min(buf.len()));
            while body_buf.len() < content_length {
                let Ok(n) = stream.read(&mut tmp) else { break };
                if n == 0 {
                    break;
                }
                body_buf.extend_from_slice(&tmp[..n]);
            }
            cap.lock().push_str(&head_text);
            cap.lock().push_str(&String::from_utf8_lossy(&body_buf));
            match step {
                StubReply::Respond(status_line, body) => {
                    let resp = format!(
                        "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(resp.as_bytes());
                }
                StubReply::CloseNoResponse => {
                    // Lost response: the request WAS processed server-side.
                    let _ = stream.flush();
                }
            }
        }
    });
    (base, captured, thread)
}

/// Accepts `calls` connections, capturing each full request (head + body)
/// into one string, and answers each with the same `status_line`/`body`
/// (correct Content-Length, `Connection: close`). Returns `(base_url,
/// captured_requests, thread)`.
#[cfg(test)]
pub fn stub_http(
    status_line: &str,
    body: &str,
    calls: usize,
) -> (String, Arc<Mutex<String>>, std::thread::JoinHandle<()>) {
    let steps = (0..calls.max(1))
        .map(|_| StubReply::Respond(status_line.to_string(), body.to_string()))
        .collect();
    stub_http_seq(steps)
}
