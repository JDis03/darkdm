// ============================================================
// DarkDM HTTP Proxy — Intercepta tráfico de video a lo IDM
// Corre en localhost:8899, el navegador se configura para usarlo
// ============================================================

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const PROXY_PORT: u16 = 8899;
const VIDEO_CONTENT_TYPES: &[&str] = &[
    "video/mp2t", "video/mp4", "video/webm", "video/x-matroska",
    "video/quicktime", "video/x-flv", "video/3gpp",
    "application/vnd.apple.mpegurl", "application/x-mpegurl",
    "application/dash+xml", "video/mpeg",
];

pub struct DarkDMProxy {
    running: Arc<AtomicBool>,
    output_dir: PathBuf,
    session: String,
    segment_count: Arc<std::sync::atomic::AtomicU64>,
    pub port: u16,
}

impl DarkDMProxy {
    pub fn new(output_dir: &Path, session: &str) -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            output_dir: output_dir.to_path_buf(),
            session: session.to_string(),
            segment_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            port: PROXY_PORT,
        }
    }

    pub fn start(&mut self) -> Result<(), String> {
        if self.running.load(Ordering::SeqCst) {
            return Err("Proxy already running".to_string());
        }

        let addr = format!("127.0.0.1:{}", self.port);
        let listener = TcpListener::bind(&addr)
            .map_err(|e| format!("Failed to bind proxy: {}", e))?;
        listener.set_nonblocking(true)
            .map_err(|e| format!("Failed to set nonblocking: {}", e))?;

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let output_dir = self.output_dir.clone();
        let session = self.session.clone();
        let segment_count = self.segment_count.clone();

        eprintln!("[DarkDM Proxy] Listening on {}", addr);

        thread::spawn(move || {
            let seg_dir = output_dir.join("_proxy_segments").join(&session);
            let _ = std::fs::create_dir_all(&seg_dir);

            for stream in listener.incoming() {
                if !running.load(Ordering::SeqCst) {
                    break;
                }

                match stream {
                    Ok(client) => {
                        let seg_dir = seg_dir.clone();
                        let seg_count = segment_count.clone();
                        thread::spawn(move || {
                            handle_client(client, &seg_dir, &seg_count);
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(e) => {
                        eprintln!("[DarkDM Proxy] Accept error: {}", e);
                    }
                }
            }
            eprintln!("[DarkDM Proxy] Stopped");
        });

        Ok(())
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        eprintln!("[DarkDM Proxy] Stopping...");
    }

    pub fn get_segment_count(&self) -> u64 {
        self.segment_count.load(Ordering::SeqCst)
    }
}

fn handle_client(mut client: TcpStream, seg_dir: &Path, seg_count: &Arc<std::sync::atomic::AtomicU64>) {
    let mut buf = [0u8; 16384];
    let mut request = Vec::new();

    // Read the HTTP request
    loop {
        match client.read(&mut buf) {
            Ok(0) => return, // Connection closed
            Ok(n) => {
                request.extend_from_slice(&buf[..n]);
                if request.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
                if request.len() > 65536 {
                    return; // Too large
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5));
                continue;
            }
            Err(_) => return,
        }
    }

    let request_str = String::from_utf8_lossy(&request);

    // Parse request line: "GET http://host/path HTTP/1.1"
    let first_line = request_str.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 3 {
        return;
    }

    let method = parts[0];
    let target = parts[1];

    // Handle CONNECT (HTTPS tunnel)
    if method == "CONNECT" {
        // Just tunnel the connection — can't inspect HTTPS easily without cert
        handle_connect(client, target);
        return;
    }

    // Regular HTTP proxy request: GET http://host/path HTTP/1.1
    if !target.starts_with("http://") && !target.starts_with("https://") {
        return;
    }

    let parsed_url = match url::Url::parse(target) {
        Ok(u) => u,
        Err(_) => return,
    };

    let host = parsed_url.host_str().unwrap_or("").to_string();
    let port = parsed_url.port().unwrap_or(80);
    let path = parsed_url.path().to_string();
    let query = parsed_url.query().map(|q| format!("?{}", q)).unwrap_or_default();
    let request_target = format!("{}{}", path, query);

    // Connect to the target server
    let mut server = match TcpStream::connect(format!("{}:{}", host, port)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[DarkDM Proxy] Connect to {}:{} failed: {}", host, port, e);
            let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n");
            return;
        }
    };
    let _ = server.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = server.set_write_timeout(Some(Duration::from_secs(30)));

    // Rewrite the request: absolute URL → relative path
    let rewritten = request_str.replacen(target, &request_target, 1);
    
    // Forward to server
    if let Err(e) = server.write_all(rewritten.as_bytes()) {
        eprintln!("[DarkDM Proxy] Forward error: {}", e);
        return;
    }

    // Read response headers
    let mut response_headers = Vec::new();
    let mut header_buf = [0u8; 4096];
    let mut content_length: i64 = -1;
    let mut content_type = String::new();
    let mut is_video = false;

    loop {
        match server.read(&mut header_buf) {
            Ok(0) => break,
            Ok(n) => {
                response_headers.extend_from_slice(&header_buf[..n]);
                if response_headers.windows(4).any(|w| w == b"\r\n\r\n") {
                    // Parse headers
                    let header_str = String::from_utf8_lossy(&response_headers);
                    for line in header_str.lines() {
                        let lower = line.to_lowercase();
                        if lower.starts_with("content-length:") {
                            if let Ok(val) = lower.trim_start_matches("content-length:").trim().parse::<i64>() {
                                content_length = val;
                            }
                        }
                        if lower.starts_with("content-type:") {
                            content_type = lower.trim_start_matches("content-type:").trim().to_string();
                            for vt in VIDEO_CONTENT_TYPES {
                                if content_type.contains(vt) {
                                    is_video = true;
                                    break;
                                }
                            }
                        }
                        // Also check URL for video extensions
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
                if response_headers.len() > 65536 {
                    return; // Too large
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5));
                continue;
            }
            Err(_) => return,
        }
    }

    // Forward headers to client
    if let Err(e) = client.write_all(&response_headers) {
        eprintln!("[DarkDM Proxy] Forward headers error: {}", e);
        return;
    }

    // Read and forward body
    if is_video {
        // Save video content
        let seq = seg_count.fetch_add(1, Ordering::SeqCst);
        let ext = if content_type.contains("mpegurl") { "m3u8" }
                  else if content_type.contains("dash+xml") { "mpd" }
                  else if target.ends_with(".ts") || content_type.contains("mp2t") { "ts" }
                  else if target.ends_with(".m4s") { "m4s" }
                  else { "bin" };
        
        let seg_path = seg_dir.join(format!("{:05}.{}", seq, ext));
        eprintln!("[DarkDM Proxy] Saving video: {} -> {}", target, seg_path.display());

        // Write headers + body to file
        if let Ok(mut file) = std::fs::File::create(&seg_path) {
            let _ = file.write_all(&response_headers);
            // Forward body and save
            if content_length > 0 {
                let mut remaining = content_length as usize;
                let mut body = vec![0u8; remaining.min(10_000_000)]; // Max 10MB per segment
                while remaining > 0 {
                    let read_size = body.len().min(remaining);
                    match server.read(&mut body[..read_size]) {
                        Ok(0) => break,
                        Ok(n) => {
                            let _ = file.write_all(&body[..n]);
                            let _ = client.write_all(&body[..n]);
                            remaining -= n;
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => break,
                    }
                }
            } else {
                // Chunked or unknown length — read until connection closes
                let mut chunk = vec![0u8; 65536];
                loop {
                    match server.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => {
                            let _ = file.write_all(&chunk[..n]);
                            let _ = client.write_all(&chunk[..n]);
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => break,
                    }
                }
            }
        } else {
            // Can't save, just forward
            pipe_stream(&mut server, &mut client, content_length);
        }
    } else {
        // Not video, just forward
        pipe_stream(&mut server, &mut client, content_length);
    }
}

/// Handle CONNECT tunneling for HTTPS — just forward bytes
fn handle_connect(mut client: TcpStream, target: &str) {
    let parts: Vec<&str> = target.split(':').collect();
    let host = parts[0];
    let port: u16 = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(443);

    match TcpStream::connect(format!("{}:{}", host, port)) {
        Ok(mut server) => {
            let _ = client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n");
            // Bidirectional tunnel
            let mut client_clone = client.try_clone().ok();
            let mut server_clone = server.try_clone().ok();
            
            if let (Some(mut c), Some(mut s)) = (client_clone, server_clone) {
                thread::spawn(move || pipe_direct(&mut c, &mut s));
                pipe_direct(&mut client, &mut server);
            }
        }
        Err(e) => {
            eprintln!("[DarkDM Proxy] CONNECT failed: {} - {}", target, e);
            let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n");
        }
    }
}

/// Pipe data from one stream to another (forwarding)
fn pipe_stream(from: &mut TcpStream, to: &mut TcpStream, content_length: i64) {
    let mut buf = vec![0u8; 65536];
    
    if content_length > 0 {
        let mut remaining = content_length as usize;
        while remaining > 0 {
            let read_size = buf.len().min(remaining);
            match from.read(&mut buf[..read_size]) {
                Ok(0) => break,
                Ok(n) => {
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
                    if let Err(_) = to.write_all(&buf[..n]) {
                        break;
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    }
}

/// Bidirectional pipe (for CONNECT)
fn pipe_direct(a: &mut TcpStream, b: &mut TcpStream) {
    let mut buf = [0u8; 65536];
    loop {
        match a.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if b.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}
