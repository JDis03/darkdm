// ============================================================
// DarkDM Proxy — Binario independiente
// Se ejecuta como proceso separado del native messaging host
// para que el proxy SIGA VIVO aunque el host se cierre.
// Uso: darkdm-proxy <session_name> <output_dir> [port]
// ============================================================

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const VIDEO_CONTENT_TYPES: &[&str] = &[
    "video/mp2t", "video/mp4", "video/webm", "video/x-matroska",
    "video/quicktime", "video/x-flv", "video/3gpp",
    "application/vnd.apple.mpegurl", "application/x-mpegurl",
    "application/dash+xml", "video/mpeg",
];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: darkdm-proxy <session> <output_dir> [port]");
        std::process::exit(1);
    }

    let session = &args[1];
    let output_dir = PathBuf::from(&args[2]);
    let port: u16 = args.get(3).and_then(|p| p.parse().ok()).unwrap_or(8899);

    let seg_dir = output_dir.join("_proxy_segments").join(session);
    if std::fs::create_dir_all(&seg_dir).is_err() {
        eprintln!("[DarkDM Proxy] Failed to create output dir");
        std::process::exit(1);
    }

    let addr = format!("127.0.0.1:{}", port);
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[DarkDM Proxy] Failed to bind {}: {}", addr, e);
            std::process::exit(1);
        }
    };
    let _ = listener.set_nonblocking(true);

    let running = Arc::new(AtomicBool::new(true));
    let seg_count = Arc::new(AtomicU64::new(0));

    eprintln!("[DarkDM Proxy] Started on {} for session {}", addr, session);

    // Handle SIGTERM gracefully
    let r = running.clone();
    ctrlc_handler(r);

    for stream in listener.incoming() {
        if !running.load(Ordering::SeqCst) {
            break;
        }
        match stream {
            Ok(client) => {
                let sd = seg_dir.clone();
                let sc = seg_count.clone();
                thread::spawn(move || handle_client(client, &sd, &sc));
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                eprintln!("[DarkDM Proxy] Accept error: {}", e);
            }
        }
    }

    eprintln!("[DarkDM Proxy] Stopped. {} segments captured.", seg_count.load(Ordering::SeqCst));
}

#[cfg(unix)]
fn ctrlc_handler(running: Arc<AtomicBool>) {
    let r = running.clone();
    thread::spawn(move || {
        let mut sigterm = false;
        // Simple: just wait for stdin close, which happens when parent dies
        let mut buf = [0u8; 1];
        loop {
            match std::io::stdin().read(&mut buf) {
                Ok(0) | Err(_) => { // stdin closed = parent died
                    eprintln!("[DarkDM Proxy] Parent process died, shutting down");
                    r.store(false, Ordering::SeqCst);
                    break;
                }
                Ok(_) => {}
            }
            thread::sleep(Duration::from_secs(1));
        }
    });
}

#[cfg(not(unix))]
fn ctrlc_handler(running: Arc<AtomicBool>) {
    let r = running.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(3600));
        r.store(false, Ordering::SeqCst);
    });
}

fn handle_client(mut client: TcpStream, seg_dir: &std::path::Path, seg_count: &Arc<AtomicU64>) {
    let mut buf = [0u8; 16384];
    let mut request = Vec::new();

    loop {
        match client.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => {
                request.extend_from_slice(&buf[..n]);
                if request.windows(4).any(|w| w == b"\r\n\r\n") { break; }
                if request.len() > 65536 { return; }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5)); continue;
            }
            Err(_) => return,
        }
    }

    let request_str = String::from_utf8_lossy(&request);
    let first_line = request_str.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 3 { return; }

    let method = parts[0];
    let target = parts[1];

    if method == "CONNECT" {
        handle_connect(client, target);
        return;
    }

    if !target.starts_with("http://") && !target.starts_with("https://") { return; }

    let parsed_url = match url::Url::parse(target) {
        Ok(u) => u,
        Err(_) => return,
    };

    let host = parsed_url.host_str().unwrap_or("").to_string();
    let port = parsed_url.port().unwrap_or(80);
    let path = parsed_url.path().to_string();
    let query = parsed_url.query().map(|q| format!("?{}", q)).unwrap_or_default();
    let request_target = format!("{}{}", path, query);

    let mut server = match TcpStream::connect(format!("{}:{}", host, port)) {
        Ok(s) => s,
        Err(_) => {
            let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n");
            return;
        }
    };
    let _ = server.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = server.set_write_timeout(Some(Duration::from_secs(30)));

    let rewritten = request_str.replacen(target, &request_target, 1);
    if server.write_all(rewritten.as_bytes()).is_err() { return; }

    // Read response headers
    let mut resp_headers = Vec::new();
    let mut hbuf = [0u8; 4096];
    let mut content_length: i64 = -1;
    let mut is_video = false;

    loop {
        match server.read(&mut hbuf) {
            Ok(0) => break,
            Ok(n) => {
                resp_headers.extend_from_slice(&hbuf[..n]);
                if resp_headers.windows(4).any(|w| w == b"\r\n\r\n") {
                    let hdr_str = String::from_utf8_lossy(&resp_headers);
                    for line in hdr_str.lines() {
                        let lc = line.to_lowercase();
                        if lc.starts_with("content-length:") {
                            content_length = lc.trim_start_matches("content-length:").trim().parse().unwrap_or(-1);
                        }
                        if lc.starts_with("content-type:") {
                            let ct = lc.trim_start_matches("content-type:").trim().to_string();
                            for vt in VIDEO_CONTENT_TYPES {
                                if ct.contains(vt) { is_video = true; break; }
                            }
                        }
                        if !is_video {
                            let url_lower = target.to_lowercase();
                            if url_lower.ends_with(".ts") || url_lower.ends_with(".m4s") ||
                               url_lower.ends_with(".m3u8") || url_lower.ends_with(".mpd") ||
                               url_lower.contains(".ts?") || url_lower.contains("seg-") {
                                is_video = true;
                            }
                        }
                    }
                    break;
                }
                if resp_headers.len() > 65536 { return; }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5)); continue;
            }
            Err(_) => return,
        }
    }

    // Forward headers to client
    if client.write_all(&resp_headers).is_err() { return; }

    // Read and forward/save body
    if is_video {
        let seq = seg_count.fetch_add(1, Ordering::SeqCst);
        let ext = if content_type_contains(&String::from_utf8_lossy(&resp_headers), "mpegurl") { "m3u8" }
                  else if content_type_contains(&String::from_utf8_lossy(&resp_headers), "dash+xml") { "mpd" }
                  else if target.ends_with(".ts") || content_type_contains(&String::from_utf8_lossy(&resp_headers), "mp2t") { "ts" }
                  else if target.ends_with(".m4s") { "m4s" }
                  else { "bin" };
        let seg_path = seg_dir.join(format!("{:05}.{}", seq, ext));
        eprintln!("[DarkDM Proxy] Saving: {} -> {}", target.split('?').next().unwrap_or(target), seg_path.display());

        if let Ok(mut file) = std::fs::File::create(&seg_path) {
            let _ = file.write_all(&resp_headers);
            pipe_and_save(&mut server, &mut client, &mut file, content_length);
        } else {
            pipe_forward(&mut server, &mut client, content_length);
        }
    } else {
        pipe_forward(&mut server, &mut client, content_length);
    }
}

fn content_type_contains(headers: &str, needle: &str) -> bool {
    for line in headers.lines() {
        if line.to_lowercase().starts_with("content-type:") && line.to_lowercase().contains(needle) {
            return true;
        }
    }
    false
}

fn pipe_and_save(from: &mut TcpStream, to: &mut TcpStream, file: &mut std::fs::File, content_length: i64) {
    let mut buf = vec![0u8; 65536];
    if content_length > 0 {
        let mut remaining = content_length as usize;
        while remaining > 0 {
            let read_size = buf.len().min(remaining);
            match from.read(&mut buf[..read_size]) {
                Ok(0) => break,
                Ok(n) => {
                    let _ = file.write_all(&buf[..n]);
                    let _ = to.write_all(&buf[..n]);
                    remaining -= n;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    } else {
        loop {
            match from.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let _ = file.write_all(&buf[..n]);
                    if to.write_all(&buf[..n]).is_err() { break; }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    }
}

fn pipe_forward(from: &mut TcpStream, to: &mut TcpStream, content_length: i64) {
    let mut buf = vec![0u8; 65536];
    if content_length > 0 {
        let mut remaining = content_length as usize;
        while remaining > 0 {
            let read_size = buf.len().min(remaining);
            match from.read(&mut buf[..read_size]) {
                Ok(0) => break,
                Ok(n) => { let _ = to.write_all(&buf[..n]); remaining -= n; }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    } else {
        loop {
            match from.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => { if to.write_all(&buf[..n]).is_err() { break; } }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    }
}

fn handle_connect(mut client: TcpStream, target: &str) {
    let parts: Vec<&str> = target.split(':').collect();
    let host = parts[0];
    let port: u16 = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(443);
    match TcpStream::connect(format!("{}:{}", host, port)) {
        Ok(mut server) => {
            let _ = client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n");
            let mut c2 = client.try_clone().ok();
            let mut s2 = server.try_clone().ok();
            if let (Some(mut c), Some(mut s)) = (c2, s2) {
                thread::spawn(move || pipe_connect(&mut c, &mut s));
            }
            pipe_connect(&mut client, &mut server);
        }
        Err(_) => { let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n"); }
    }
}

fn pipe_connect(a: &mut TcpStream, b: &mut TcpStream) {
    let mut buf = [0u8; 65536];
    loop {
        match a.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => { if b.write_all(&buf[..n]).is_err() { break; } }
            Err(_) => break,
        }
    }
}
