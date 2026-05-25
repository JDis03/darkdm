// ============================================================
// darkdm-proxy-mitm — Proxy async con CONNECT tunneling
// Maneja HTTP (captura .ts) + CONNECT (túnel, sin MITM)
// ============================================================
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const HOME_DIR: &str = "Descargas/DarkDM";
const SEGMENT_DIR: &str = "_proxy_segments";

fn is_video_request(target: &str) -> bool {
    let t = target.to_lowercase();
    t.contains(".ts") || t.contains("/seg-") || t.contains("/chunk") || t.contains("segment")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let session = args.get(1).map(|s| s.as_str()).unwrap_or("default");
    let port: u16 = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(8899);

    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let output_dir = PathBuf::from(&home).join(HOME_DIR);
    let seg_dir = output_dir.join(SEGMENT_DIR).join(session);
    tokio::fs::create_dir_all(&seg_dir).await?;
    let seg_dir = Arc::new(seg_dir);
    let seg_count = Arc::new(AtomicU64::new(0));

    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    eprintln!("[Proxy] Listening on {} (session: {})", addr, session);

    loop {
        let (stream, _) = listener.accept().await?;
        let sd = seg_dir.clone();
        let sc = seg_count.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_conn(stream, &sd, &sc).await {
                eprintln!("[Proxy] Error: {}", e);
            }
        });
    }
}

async fn handle_conn(
    mut stream: TcpStream,
    seg_dir: &PathBuf,
    seg_count: &AtomicU64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = [0u8; 4096];
    let n = stream.peek(&mut buf).await?;
    if n == 0 { return Ok(()); }

    let head = String::from_utf8_lossy(&buf[..n.min(256)]);

    if head.starts_with("CONNECT ") {
        // CONNECT tunnel — just forward bytes, can't inspect HTTPS
        let line = head.lines().next().unwrap_or("");
        let target = line.trim_start_matches("CONNECT ").trim_end_matches(" HTTP/1.1").trim();
        let (host, port) = target.split_once(':')
            .map(|(h, p)| (h.to_string(), p.parse::<u16>().unwrap_or(443)))
            .unwrap_or_else(|| (target.to_string(), 443));

        match TcpStream::connect(format!("{}:{}", host, port)).await {
            Ok(mut upstream) => {
                stream.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;
                let _ = tokio::io::copy_bidirectional(&mut stream, &mut upstream).await;
            }
            Err(_) => {
                let _ = stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
            }
        }
        return Ok(());
    }

    // HTTP request
    let mut req = Vec::new();
    stream.read_to_end(&mut req).await?;
    let req_str = String::from_utf8_lossy(&req);
    let first_line = req_str.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 { return Ok(()); }

    let target = if parts[1].starts_with("http://") {
        parts[1].to_string()
    } else if parts[1].starts_with('/') {
        let host = req_str.lines()
            .find(|l| l.to_lowercase().starts_with("host:"))
            .and_then(|l| l.split(':').nth(1))
            .map(|h| h.trim())
            .unwrap_or("localhost");
        format!("http://{}{}", host, parts[1])
    } else {
        return Ok(());
    };

    let parsed = url::Url::parse(&target)?;
    let host = parsed.host_str().unwrap_or("localhost").to_string();
    let port = parsed.port().unwrap_or(80);
    let path = parsed.path().to_string();
    let query = parsed.query().map(|q| format!("?{}", q)).unwrap_or_default();

    let mut server = TcpStream::connect(format!("{}:{}", host, port)).await?;
    let request_target = format!("{}{}", path, query);
    let rewritten = req_str.replacen(&parts[1], &request_target, 1);
    server.write_all(rewritten.as_bytes()).await?;

    let mut resp = Vec::new();
    server.read_to_end(&mut resp).await?;

    if is_video_request(&target) {
        let seq = seg_count.fetch_add(1, Ordering::SeqCst);
        let seg_path = seg_dir.join(format!("seg{:05}.ts", seq));
        if let Ok(mut f) = tokio::fs::File::create(&seg_path).await {
            let _ = f.write_all(&resp).await;
        }
        eprintln!("[Proxy] Captured .ts #{} → {}", seq, seg_path.display());
    }

    stream.write_all(&resp).await?;
    Ok(())
}
