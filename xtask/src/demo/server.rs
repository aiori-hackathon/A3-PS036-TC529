//! A loopback-only static server for the built site.
//!
//! The demonstration needs the real production output, including the
//! deployment's 404 behaviour: the site's own `404.html` when the build ships
//! one, otherwise a short plain-text 404 with the matching status code. It
//! binds `127.0.0.1` only and never follows a path outside the document root.

use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context, Result};

/// A running server handle.
pub(crate) struct Server {
    pub(crate) addr: SocketAddr,
    stop: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
}

impl Server {
    pub(crate) fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Ask the accept loop to stop and wait for it.
    pub(crate) fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Bind `addr` and serve `root` until the returned handle is stopped.
pub(crate) fn start(root: &Path, addr: SocketAddr) -> Result<Server> {
    if !root.is_dir() {
        bail!(
            "{} is not a directory; build the site first (cd site && npm run build)",
            root.display()
        );
    }
    let listener = TcpListener::bind(addr)
        .with_context(|| format!("binding {addr} (is port 8080 already in use?)"))?;
    listener
        .set_nonblocking(true)
        .context("making the listener non-blocking")?;
    let bound = listener.local_addr().context("reading the bound address")?;
    let root = root.to_path_buf();
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let join = thread::spawn(move || accept_loop(&listener, &root, &flag));
    Ok(Server {
        addr: bound,
        stop,
        join: Some(join),
    })
}

fn accept_loop(listener: &TcpListener, root: &Path, stop: &AtomicBool) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let root = root.to_path_buf();
                thread::spawn(move || {
                    let _ = handle(stream, &root);
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(_) => break,
        }
    }
}

fn handle(mut stream: TcpStream, root: &Path) -> Result<()> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .context("setting the read timeout")?;
    let mut request = Vec::new();
    let mut buffer = [0_u8; 2048];
    loop {
        let read = stream.read(&mut buffer).context("reading the request")?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") || request.len() > 16_384 {
            break;
        }
    }
    let text = String::from_utf8_lossy(&request);
    let Some(line) = text.lines().next() else {
        return respond(&mut stream, 400, "text/plain", b"400 Bad Request\n");
    };
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    if method != "GET" && method != "HEAD" {
        return respond(&mut stream, 405, "text/plain", b"405 Method Not Allowed\n");
    }
    let head_only = method == "HEAD";
    let path = target.split('?').next().unwrap_or("/");
    let Some(relative) = safe_relative(path) else {
        return not_found(&mut stream, root, head_only);
    };
    let mut file = root.join(&relative);
    if file.is_dir() {
        if !path.ends_with('/') {
            let location = format!("{path}/");
            let body = format!("301 Moved Permanently\nLocation: {location}\n");
            return respond_with(
                &mut stream,
                301,
                "text/plain; charset=utf-8",
                body.as_bytes(),
                head_only,
                Some(&location),
            );
        }
        file = file.join("index.html");
    }
    match fs::read(&file) {
        Ok(body) => respond_with(
            &mut stream,
            200,
            content_type(&file),
            &body,
            head_only,
            None,
        ),
        Err(_) => not_found(&mut stream, root, head_only),
    }
}

fn not_found(stream: &mut TcpStream, root: &Path, head_only: bool) -> Result<()> {
    match fs::read(root.join("404.html")) {
        Ok(body) => respond_with(
            stream,
            404,
            "text/html; charset=utf-8",
            &body,
            head_only,
            None,
        ),
        Err(_) => respond_with(
            stream,
            404,
            "text/plain; charset=utf-8",
            b"404 Not Found\n",
            head_only,
            None,
        ),
    }
}

/// Turn a request path into a relative path that cannot escape the root.
fn safe_relative(raw: &str) -> Option<PathBuf> {
    let decoded = percent_decode(raw);
    let mut out = PathBuf::new();
    for component in Path::new(decoded.trim_start_matches('/')).components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(out)
}

fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if let Some(hex) = raw.get(index + 1..index + 3) {
                if let Ok(value) = u8::from_str_radix(hex, 16) {
                    out.push(value);
                    index += 3;
                    continue;
                }
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn content_type(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json",
        "wasm" => "application/wasm",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "webp" => "image/webp",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "txt" => "text/plain; charset=utf-8",
        "pdf" => "application/pdf",
        "gz" => "application/gzip",
        "tar" => "application/x-tar",
        "xml" => "application/xml",
        "webmanifest" => "application/manifest+json",
        _ => "application/octet-stream",
    }
}

fn respond(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) -> Result<()> {
    respond_with(stream, status, content_type, body, false, None)
}

fn respond_with(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
    head_only: bool,
    location: Option<&str>,
) -> Result<()> {
    let reason = match status {
        200 => "OK",
        301 => "Moved Permanently",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Unknown",
    };
    let mut head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n",
        body.len()
    );
    if let Some(location) = location {
        head.push_str(&format!("Location: {location}\r\n"));
    }
    head.push_str("\r\n");
    stream
        .write_all(head.as_bytes())
        .context("writing the response head")?;
    if !head_only {
        stream
            .write_all(body)
            .context("writing the response body")?;
    }
    stream.flush().context("flushing the response")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::time::Instant;

    fn temp_root(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("climb-demo-server-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("tui")).expect("temp root");
        fs::write(dir.join("index.html"), "<h1>home</h1>").expect("index");
        fs::write(dir.join("tui/index.html"), "<h1>tui</h1>").expect("tui");
        fs::write(dir.join("app.wasm"), b"\0asm").expect("wasm");
        dir
    }

    fn get(addr: SocketAddr, path: &str) -> (u16, String, Vec<u8>) {
        let mut stream = TcpStream::connect(addr).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");
        write!(stream, "GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").expect("write");
        let mut reader = BufReader::new(stream);
        let mut head = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("read line");
            if line == "\r\n" || line.is_empty() {
                break;
            }
            head.push_str(&line);
        }
        let status: u16 = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse().ok())
            .expect("status");
        let mut body = Vec::new();
        reader.read_to_end(&mut body).expect("body");
        (status, head, body)
    }

    #[test]
    fn serves_the_index_a_directory_index_and_a_wasm_content_type() {
        let dir = temp_root("serve");
        let server = start(&dir, "127.0.0.1:0".parse().expect("addr")).expect("server");
        let (status, head, body) = get(server.addr, "/");
        assert_eq!(status, 200);
        assert!(head.contains("text/html"), "{head}");
        assert_eq!(body, b"<h1>home</h1>");
        let (status, _, body) = get(server.addr, "/tui/");
        assert_eq!(status, 200);
        assert_eq!(body, b"<h1>tui</h1>");
        let (status, head, _) = get(server.addr, "/app.wasm");
        assert_eq!(status, 200);
        assert!(head.contains("application/wasm"), "{head}");
        server.stop();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn redirects_a_directory_without_a_trailing_slash() {
        let dir = temp_root("redirect");
        let server = start(&dir, "127.0.0.1:0".parse().expect("addr")).expect("server");
        let (status, head, _) = get(server.addr, "/tui");
        assert_eq!(status, 301);
        assert!(head.contains("Location: /tui/"), "{head}");
        server.stop();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_path_uses_the_sites_own_404_page() {
        let dir = temp_root("404-page");
        fs::write(dir.join("404.html"), "<h1>not here</h1>").expect("404");
        let server = start(&dir, "127.0.0.1:0".parse().expect("addr")).expect("server");
        let (status, head, body) = get(server.addr, "/missing");
        assert_eq!(status, 404);
        assert!(head.contains("text/html"), "{head}");
        assert_eq!(body, b"<h1>not here</h1>");
        server.stop();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_path_without_a_404_page_still_returns_404() {
        let dir = temp_root("404-plain");
        let server = start(&dir, "127.0.0.1:0".parse().expect("addr")).expect("server");
        let (status, _, body) = get(server.addr, "/missing");
        assert_eq!(status, 404);
        assert_eq!(body, b"404 Not Found\n");
        server.stop();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_traversal_attempt_never_leaves_the_root() {
        let temp = temp_root("traversal");
        let dir = temp.join("public");
        fs::create_dir_all(&dir).expect("public");
        fs::write(temp.join("secret.txt"), "secret").expect("secret");
        let server = start(&dir, "127.0.0.1:0".parse().expect("addr")).expect("server");
        for path in [
            "/../secret.txt",
            "/%2e%2e/secret.txt",
            "/tui/../../secret.txt",
        ] {
            let (status, _, body) = get(server.addr, path);
            assert_eq!(status, 404, "{path}");
            assert_ne!(body, b"secret", "{path}");
        }
        server.stop();
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn a_head_request_carries_the_length_but_no_body() {
        let dir = temp_root("head");
        let server = start(&dir, "127.0.0.1:0".parse().expect("addr")).expect("server");
        let mut stream = TcpStream::connect(server.addr).expect("connect");
        write!(stream, "HEAD / HTTP/1.1\r\nHost: x\r\n\r\n").expect("write");
        let mut text = String::new();
        stream.read_to_string(&mut text).expect("read");
        assert!(text.starts_with("HTTP/1.1 200 OK"), "{text}");
        assert!(text.contains("Content-Length: 13"), "{text}");
        assert!(!text.contains("<h1>home</h1>"), "{text}");
        server.stop();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn stop_returns_promptly_and_frees_the_port() {
        let dir = temp_root("stop");
        let server = start(&dir, "127.0.0.1:0".parse().expect("addr")).expect("server");
        let addr = server.addr;
        let started = Instant::now();
        server.stop();
        assert!(started.elapsed() < Duration::from_secs(5));
        let listener = TcpListener::bind(addr).expect("port is free again");
        drop(listener);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn safe_relative_rejects_escape_and_accepts_ordinary_paths() {
        assert_eq!(safe_relative("/"), Some(PathBuf::new()));
        assert_eq!(safe_relative("/tui/"), Some(PathBuf::from("tui")));
        assert_eq!(safe_relative("/a%20b.css"), Some(PathBuf::from("a b.css")));
        assert_eq!(safe_relative("/../etc/passwd"), None);
        assert_eq!(safe_relative("/a/../../b"), None);
    }
}
