// ============================================================
// DarkDM HTTP Server
// Reemplaza native messaging de Chrome (que falla en MV3)
// Escucha en localhost:8765 y ejecuta ffmpeg para descargar streams
// ============================================================

mod downloader;
mod log;
mod server;

fn main() {
    // Inicializar logging
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let log_dir = std::path::Path::new(&home).join("Descargas/DarkDM");
    std::fs::create_dir_all(&log_dir).unwrap_or_default();
    log::init(&log_dir);

    log::log("DarkDM HTTP Server starting...");
    eprintln!("[DarkDM] Starting HTTP server...");

    // Iniciar servidor HTTP (bloqueante)
    server::start_server();
}
