use std::env;
use std::io::Cursor;
use std::process::Command;
use std::sync::Arc;
use std::thread;
use tiny_http::{Header, Method, Request, Response, Server};

use crate::log;

const DEFAULT_PORT: u16 = 8765;

pub fn start_server() {
    let port = env::var("DARKDM_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    let addr = format!("127.0.0.1:{}", port);
    let server = match Server::http(&addr) {
        Ok(s) => {
            log::log(&format!("HTTP server listening on {}", addr));
            eprintln!("[DarkDM] HTTP server listening on {}", addr);
            Arc::new(s)
        }
        Err(e) => {
            log::log(&format!("Failed to bind {}: {}", addr, e));
            eprintln!("[DarkDM] Failed to bind {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    for request in server.incoming_requests() {
        let method = request.method().clone();
        let url = request.url().to_string();

        log::log(&format!("{} {}", method, url));

        match (method, url.as_str()) {
            (Method::Options, _) => handle_options(request),
            (Method::Get, "/health") => handle_health(request),
            (Method::Post, "/download") => handle_download(request),
            _ => {
                let body = r#"{"error":"Not found"}"#;
                let response = Response::from_string(body)
                    .with_status_code(404)
                    .with_header(json_header())
                    .with_header(cors_header());
                let _ = request.respond(response);
            }
        }
    }
}

fn handle_options(request: Request) {
    let response = Response::empty(204)
        .with_header(cors_header())
        .with_header(cors_methods_header())
        .with_header(cors_headers_header());
    let _ = request.respond(response);
}

fn handle_health(request: Request) {
    let body = r#"{"status":"ok","version":"1.0.0"}"#;
    let response = Response::from_string(body)
        .with_status_code(200)
        .with_header(json_header())
        .with_header(cors_header());
    let _ = request.respond(response);
}

fn handle_download(mut request: Request) {
    // Read request body
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        let err = r#"{"success":false,"error":"Failed to read request body"}"#;
        let response = Response::from_string(err)
            .with_status_code(400)
            .with_header(json_header())
            .with_header(cors_header());
        let _ = request.respond(response);
        return;
    }

    // Parse JSON
    let data: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => {
            let err = r#"{"success":false,"error":"Invalid JSON"}"#;
            let response = Response::from_string(err)
                .with_status_code(400)
                .with_header(json_header())
                .with_header(cors_header());
            let _ = request.respond(response);
            return;
        }
    };

    // Validate required fields
    let manifest_url = match data.get("manifest_url").and_then(|v| v.as_str()) {
        Some(u) if !u.is_empty() => u.to_string(),
        _ => {
            let err = r#"{"success":false,"error":"manifest_url is required"}"#;
            let response = Response::from_string(err)
                .with_status_code(400)
                .with_header(json_header())
                .with_header(cors_header());
            let _ = request.respond(response);
            return;
        }
    };

    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("video");
    let page_url = data
        .get("page_url")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Extract headers
    let headers = data.get("headers").and_then(|v| v.as_object());
    let user_agent = headers
        .and_then(|h| h.get("user-agent").or(h.get("User-Agent")))
        .and_then(|v| v.as_str())
        .unwrap_or("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36");
    let referer = headers
        .and_then(|h| h.get("referer").or(h.get("Referer")))
        .and_then(|v| v.as_str())
        .unwrap_or(page_url);

    // Build output path
    let home = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let output_dir = std::path::Path::new(&home).join("Descargas/DarkDM");
    std::fs::create_dir_all(&output_dir).unwrap_or_default();

    let safe_title: String = title
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .collect::<String>()
        .trim()
        .chars()
        .take(80)
        .collect();
    let filename = if safe_title.is_empty() {
        "darkdm_video".to_string()
    } else {
        safe_title
    };
    let output_path = output_dir.join(format!("{}.mp4", filename));
    let output_str = output_path.to_string_lossy().to_string();

    log::log(&format!(
        "Download: {} -> {} (referer: {})",
        manifest_url, output_str, referer
    ));

    // Spawn ffmpeg in background
    let manifest_url_clone = manifest_url.clone();
    let user_agent_clone = user_agent.to_string();
    let referer_clone = referer.to_string();
    let output_str_clone = output_str.clone();

    thread::spawn(move || {
        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-user_agent",
            &user_agent_clone,
        ]);

        if !referer_clone.is_empty() {
            cmd.args(["-referer", &referer_clone]);
        }

        cmd.args([
            "-i",
            &manifest_url_clone,
            "-c",
            "copy",
            "-movflags",
            "+faststart",
            &output_str_clone,
        ]);

        log::log(&format!("Running: ffmpeg -i {} -c copy ...", manifest_url_clone));

        match cmd.status() {
            Ok(status) if status.success() => {
                let size = std::fs::metadata(&output_str_clone)
                    .map(|m| m.len())
                    .unwrap_or(0);
                log::log(&format!("ffmpeg OK: {} bytes -> {}", size, output_str_clone));
            }
            Ok(status) => {
                log::log(&format!("ffmpeg failed with status: {}", status));
            }
            Err(e) => {
                log::log(&format!("ffmpeg spawn error: {}", e));
            }
        }
    });

    // Respond immediately
    let resp_body = serde_json::json!({
        "success": true,
        "message": "Download started",
        "output_path": output_str
    });
    let response = Response::from_string(resp_body.to_string())
        .with_status_code(200)
        .with_header(json_header())
        .with_header(cors_header());
    let _ = request.respond(response);
}

fn json_header() -> Header {
    Header::from_bytes("Content-Type", "application/json").unwrap()
}

fn cors_header() -> Header {
    Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap()
}

fn cors_methods_header() -> Header {
    Header::from_bytes("Access-Control-Allow-Methods", "POST, GET, OPTIONS").unwrap()
}

fn cors_headers_header() -> Header {
    Header::from_bytes("Access-Control-Allow-Headers", "Content-Type").unwrap()
}
